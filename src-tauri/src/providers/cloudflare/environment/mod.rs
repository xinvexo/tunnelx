pub(crate) mod cli;

use self::cli::status;
use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::domain::CloudflaredStatus;
use crate::providers::cloudflare::paths;
use crate::providers::cloudflare::tunnel::runtime;
use crate::providers::contract::{
    emit_runtime_install_progress, ProviderRuntimeUpdateStatus, CLOUDFLARE_PROVIDER_ID,
};
use crate::providers::runtime_environment::validate_executable;
use crate::state::AppState;
use futures_util::StreamExt;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::AppHandle;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

const RELEASE_API: &str = "https://api.github.com/repos/cloudflare/cloudflared/releases/latest";
const UA: &str = "tunnelx";
static ENVIRONMENT_OPERATION_BUSY: AtomicBool = AtomicBool::new(false);

struct EnvironmentOperationGuard;

impl Drop for EnvironmentOperationGuard {
    fn drop(&mut self) {
        ENVIRONMENT_OPERATION_BUSY.store(false, Ordering::SeqCst);
    }
}

fn begin_environment_operation() -> AppResult<EnvironmentOperationGuard> {
    ENVIRONMENT_OPERATION_BUSY
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| EnvironmentOperationGuard)
        .map_err(|_| AppError::Msg("cloudflared runtime operation is already in progress".into()))
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Deserialize, Clone)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

fn asset_name() -> AppResult<String> {
    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "x86") {
        "386"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else {
        return Err(AppError::Msg(
            "Automatic cloudflared installation is not supported on this CPU architecture".into(),
        ));
    };
    let name = if cfg!(target_os = "macos") {
        format!("cloudflared-darwin-{arch}.tgz")
    } else if cfg!(target_os = "windows") {
        format!("cloudflared-windows-{arch}.exe")
    } else if cfg!(target_os = "linux") {
        format!("cloudflared-linux-{arch}")
    } else {
        return Err(AppError::Msg(
            "Automatic cloudflared installation is not supported on this operating system".into(),
        ));
    };
    Ok(name)
}

async fn latest_release() -> AppResult<GhRelease> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()?;
    let release = client
        .get(RELEASE_API)
        .send()
        .await?
        .error_for_status()?
        .json::<GhRelease>()
        .await?;
    Ok(release)
}

async fn latest_asset() -> AppResult<GhAsset> {
    let wanted = asset_name()?;
    let release = latest_release().await?;
    release
        .assets
        .into_iter()
        .find(|asset| asset.name == wanted)
        .ok_or_else(|| AppError::Msg(format!("cloudflared release does not include {wanted}")))
}

pub async fn check_update(
    app: AppHandle,
    state: AppState,
) -> AppResult<ProviderRuntimeUpdateStatus> {
    let current = status(&app, &state)?;
    let release = latest_release().await?;
    let current_version = current.version.as_deref().and_then(extract_version);
    let latest_version = Some(release.tag_name.trim_start_matches('v').to_string());
    let update_available = current.available
        && current_version.is_some()
        && latest_version.as_deref() != current_version.as_deref();
    Ok(ProviderRuntimeUpdateStatus {
        provider_id: CLOUDFLARE_PROVIDER_ID.into(),
        runtime: "cloudflared".into(),
        current_version,
        latest_version,
        update_available,
        message: String::new(),
    })
}

fn extract_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(|part| {
            part.trim_matches(|ch: char| ch == ',' || ch == ')')
                .to_string()
        })
}

async fn download(app: &AppHandle, url: &str, dest: &Path, expected_size: u64) -> AppResult<()> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let resp = client.get(url).send().await?.error_for_status()?;
    let mut file = tokio::fs::File::create(dest).await?;
    let mut stream = resp.bytes_stream();
    let mut received = 0_u64;
    let mut last_emitted = 0_u64;
    let total = (expected_size > 0).then_some(expected_size);
    emit_runtime_install_progress(app, CLOUDFLARE_PROVIDER_ID, "cloudflared", 0, total);
    let max_size = expected_size.saturating_add(1024 * 1024);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        received += chunk.len() as u64;
        if expected_size > 0 && received > max_size {
            return Err(AppError::Msg(
                "cloudflared download is larger than expected".into(),
            ));
        }
        file.write_all(&chunk).await?;
        if received == expected_size || received.saturating_sub(last_emitted) >= 256 * 1024 {
            emit_runtime_install_progress(
                app,
                CLOUDFLARE_PROVIDER_ID,
                "cloudflared",
                received,
                total,
            );
            last_emitted = received;
        }
    }
    file.flush().await?;
    if expected_size > 0 && received != expected_size {
        return Err(AppError::Msg(
            "cloudflared download size does not match the release metadata".into(),
        ));
    }
    emit_runtime_install_progress(app, CLOUDFLARE_PROVIDER_ID, "cloudflared", received, total);
    Ok(())
}

fn unpack_targz(archive: &Path, dest: &Path) -> AppResult<()> {
    let file = fs::File::open(archive)?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut ar = tar::Archive::new(gz);
    for entry in ar.entries()? {
        let mut entry = entry?;
        let is_cloudflared = entry
            .path()?
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name == "cloudflared")
            .unwrap_or(false);
        if is_cloudflared {
            let mut out = fs::File::create(dest)?;
            std::io::copy(&mut entry, &mut out)?;
            return Ok(());
        }
    }
    Err(AppError::Msg(
        "cloudflared executable was not found in the downloaded archive".into(),
    ))
}

fn temp_file(bin_dir: &Path, label: &str) -> PathBuf {
    bin_dir.join(format!(
        ".{}-{}-{label}.tmp",
        paths::exe_name(),
        Uuid::new_v4()
    ))
}

#[cfg(unix)]
fn replace_file(temp: &Path, target: &Path) -> AppResult<()> {
    fs::rename(temp, target)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(temp: &Path, target: &Path) -> AppResult<()> {
    let backup = target.with_file_name(format!(
        ".{}-{}.backup",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("cloudflared"),
        Uuid::new_v4()
    ));
    let had_target = target.exists();
    if had_target {
        fs::rename(target, &backup)?;
    }
    match fs::rename(temp, target) {
        Ok(()) => {
            if had_target {
                let _ = fs::remove_file(&backup);
            }
            Ok(())
        }
        Err(error) => {
            if had_target {
                let _ = fs::rename(&backup, target);
            }
            Err(error.into())
        }
    }
}

#[cfg(unix)]
fn set_executable(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(path)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> AppResult<()> {
    Ok(())
}

fn has_running_cloudflare(state: &AppState) -> bool {
    state
        .provider_runtime
        .active_keys()
        .iter()
        .any(|(provider_id, _)| provider_id == CLOUDFLARE_PROVIDER_ID)
}

pub async fn install(app: AppHandle, state: AppState) -> AppResult<CloudflaredStatus> {
    let _guard = begin_environment_operation()?;
    if has_running_cloudflare(&state) {
        return Err(AppError::Msg(
            "Stop running Cloudflare connections first".into(),
        ));
    }
    let asset = latest_asset().await?;
    let bin_dir = paths::bin_dir(&app)?;
    tokio::fs::create_dir_all(&bin_dir).await?;
    let dest = paths::managed_exe(&app)?;
    let download_tmp = temp_file(&bin_dir, "download");
    let exe_tmp = temp_file(&bin_dir, "exe");
    if let Err(error) = download(&app, &asset.browser_download_url, &download_tmp, asset.size).await
    {
        let _ = tokio::fs::remove_file(&download_tmp).await;
        return Err(error);
    }

    let result = if asset.name.ends_with(".tgz") {
        let exe_tmp_clone = exe_tmp.clone();
        let download_tmp_clone = download_tmp.clone();
        tokio::task::spawn_blocking(move || unpack_targz(&download_tmp_clone, &exe_tmp_clone))
            .await
            .map_err(|error| AppError::Msg(error.to_string()))?
    } else {
        tokio::fs::copy(&download_tmp, &exe_tmp)
            .await
            .map(|_| ())
            .map_err(Into::into)
    };
    let _ = tokio::fs::remove_file(&download_tmp).await;
    if let Err(error) = result {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    if let Err(error) = set_executable(&exe_tmp) {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    let mut command = Command::new(&exe_tmp);
    command.arg("--version");
    if let Err(error) = validate_executable(&mut command, "cloudflared") {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    if let Err(error) = replace_file(&exe_tmp, &dest) {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    status(&app, &state)
}

pub fn uninstall(app: &AppHandle, state: &AppState) -> AppResult<CloudflaredStatus> {
    let _guard = begin_environment_operation()?;
    runtime::stop_all_tunnels(app, state)?;
    let exe = paths::managed_exe(app)?;
    if exe.exists() {
        fs::remove_file(&exe)?;
    }
    status(app, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tunnelx-cloudflare-env-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replace_file_swaps_completed_temp_into_place() {
        let dir = temp_test_dir("replace-success");
        let target = dir.join(paths::exe_name());
        let temp = dir.join(".cloudflared.tmp");
        fs::write(&target, b"old").unwrap();
        fs::write(&temp, b"new").unwrap();

        replace_file(&temp, &target).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!temp.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn replace_file_keeps_existing_target_when_temp_is_missing() {
        let dir = temp_test_dir("replace-fail");
        let target = dir.join(paths::exe_name());
        let temp = dir.join(".missing.tmp");
        fs::write(&target, b"old").unwrap();

        assert!(replace_file(&temp, &target).is_err());

        assert_eq!(fs::read(&target).unwrap(), b"old");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn environment_operation_guard_serializes_installs_and_uninstalls() {
        let guard = begin_environment_operation().unwrap();
        assert!(begin_environment_operation().is_err());
        drop(guard);
        assert!(begin_environment_operation().is_ok());
    }
}
