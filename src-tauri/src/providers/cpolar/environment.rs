use super::{paths, runtime};
use crate::error::{AppError, AppResult};
use crate::providers::contract::{ProviderRuntimeUpdateStatus, ProviderStatus, CPOLAR_PROVIDER_ID};
use crate::providers::runtime_environment::{
    command_output_text, download_with_progress, replace_file, set_executable, temp_file,
    validate_executable,
};
use crate::state::AppState;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Runtime};

const CPOLAR_VERSION: &str = "3.3.18";
const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;
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
        .map_err(|_| AppError::Msg("cpolar runtime operation is already in progress".into()))
}

pub fn resolve_command<R: Runtime>(app: &AppHandle<R>) -> AppResult<String> {
    let managed = paths::managed_exe(app)?;
    if managed.exists() {
        Ok(managed.to_string_lossy().to_string())
    } else {
        Err(AppError::Msg(
            "cpolar runtime is not installed. Install it from Runtime first".into(),
        ))
    }
}

pub fn status(app: &AppHandle, _state: &AppState) -> ProviderStatus {
    let managed_exe = paths::managed_exe(app).ok();
    let version_output = managed_exe
        .as_ref()
        .filter(|path| path.exists())
        .and_then(|path| {
            std::process::Command::new(path)
                .arg("version")
                .output()
                .ok()
        });
    let (available, version, message) = match version_output {
        Some(output) if output.status.success() => {
            let text = command_output_text(&output);
            (
                true,
                (!text.is_empty()).then_some(text),
                "cpolar runtime is installed".into(),
            )
        }
        Some(output) => {
            let text = command_output_text(&output);
            (
                false,
                (!text.is_empty()).then_some(text.clone()),
                if text.is_empty() {
                    "cpolar runtime is unavailable".into()
                } else {
                    text
                },
            )
        }
        None => (false, None, "cpolar runtime is not installed".into()),
    };
    ProviderStatus {
        provider_id: CPOLAR_PROVIDER_ID.into(),
        available,
        version,
        message,
        details: json!({
            "managed": managed_exe.as_ref().map(|path| path.exists()).unwrap_or(false),
            "path": managed_exe.map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        }),
    }
}

pub async fn install(app: AppHandle, state: AppState) -> AppResult<ProviderStatus> {
    let _guard = begin_environment_operation()?;
    ensure_not_running(&state)?;
    let url = download_url()?;
    let bin_dir = paths::bin_dir(&app)?;
    tokio::fs::create_dir_all(&bin_dir).await?;
    let dest = paths::managed_exe(&app)?;
    let download_tmp = temp_file(&bin_dir, paths::exe_name(), "download");
    let exe_tmp = temp_file(&bin_dir, paths::exe_name(), "exe");
    if let Err(error) = download_with_progress(
        &app,
        CPOLAR_PROVIDER_ID,
        "cpolar",
        &url,
        &download_tmp,
        MAX_DOWNLOAD_BYTES,
    )
    .await
    {
        let _ = tokio::fs::remove_file(&download_tmp).await;
        return Err(error);
    }
    let archive = download_tmp.clone();
    let output = exe_tmp.clone();
    let result = tokio::task::spawn_blocking(move || unpack_zip(&archive, &output))
        .await
        .map_err(|error| AppError::Msg(error.to_string()))?;
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
    command.arg("version");
    if let Err(error) = validate_executable(&mut command, "cpolar") {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    if let Err(error) = replace_file(&exe_tmp, &dest) {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    Ok(status(&app, &state))
}

pub fn uninstall(app: &AppHandle, state: &AppState) -> AppResult<ProviderStatus> {
    let _guard = begin_environment_operation()?;
    runtime::stop_all_tunnels(app, state)?;
    let exe = paths::managed_exe(app)?;
    if exe.exists() {
        fs::remove_file(&exe)?;
    }
    Ok(status(app, state))
}

pub fn check_update(app: &AppHandle, state: &AppState) -> ProviderRuntimeUpdateStatus {
    let current = status(app, state);
    let current_version = current.version.as_deref().and_then(extract_version);
    let latest_version = Some(CPOLAR_VERSION.to_string());
    let update_available = current.available
        && current_version.is_some()
        && current_version.as_deref() != latest_version.as_deref();
    ProviderRuntimeUpdateStatus {
        provider_id: CPOLAR_PROVIDER_ID.into(),
        runtime: "cpolar".into(),
        current_version,
        latest_version,
        update_available,
        message: String::new(),
    }
}

fn extract_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(|part| {
            part.trim_matches(|ch: char| ch == ',' || ch == ')')
                .to_string()
        })
}

fn ensure_not_running(state: &AppState) -> AppResult<()> {
    let running = state
        .provider_runtime
        .active_keys()
        .iter()
        .any(|(provider_id, _)| provider_id == CPOLAR_PROVIDER_ID);
    if running {
        Err(AppError::Msg(
            "Stop running cpolar connections first".into(),
        ))
    } else {
        Ok(())
    }
}

fn download_url() -> AppResult<String> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(AppError::Msg(
            "Automatic cpolar installation is not supported on this operating system".into(),
        ));
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        if cfg!(target_os = "windows") {
            return Err(AppError::Msg(
                "Automatic cpolar installation is not available for Windows ARM64".into(),
            ));
        }
        "arm64"
    } else {
        return Err(AppError::Msg(
            "Automatic cpolar installation is not supported on this CPU architecture".into(),
        ));
    };
    Ok(format!(
        "https://www.cpolar.com/static/downloads/releases/{CPOLAR_VERSION}/cpolar-stable-{os}-{arch}.zip"
    ))
}

fn unpack_zip(archive: &Path, dest: &Path) -> AppResult<()> {
    let file = fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|error| AppError::Msg(error.to_string()))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| AppError::Msg(error.to_string()))?;
        if entry.is_dir() || !is_expected_exe(entry.name()) {
            continue;
        }
        let mut out = fs::File::create(dest)?;
        std::io::copy(&mut entry, &mut out)?;
        return Ok(());
    }
    Err(AppError::Msg(
        "cpolar executable was not found in the downloaded archive".into(),
    ))
}

fn is_expected_exe(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case(paths::exe_name()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_download_url_uses_https() {
        if let Ok(url) = download_url() {
            assert!(url.starts_with("https://"));
        }
    }
}
