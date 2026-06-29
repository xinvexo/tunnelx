use super::{paths, runtime};
use crate::error::{AppError, AppResult};
use crate::providers::contract::{ProviderRuntimeUpdateStatus, ProviderStatus, PINGGY_PROVIDER_ID};
use crate::providers::runtime_environment::{
    command_output_text, download_with_progress, replace_file, set_executable, temp_file,
    validate_executable,
};
use crate::state::AppState;
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use tunnelx_watchdog_protocol::WatchdogEnvVar;

const MAX_DOWNLOAD_BYTES: u64 = 180 * 1024 * 1024;
const RUNTIME_NAME: &str = "Pinggy";
static ENVIRONMENT_OPERATION_BUSY: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

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
        .map_err(|_| AppError::Msg("Pinggy runtime operation is already in progress".into()))
}

pub fn resolve_command<R: Runtime>(app: &AppHandle<R>) -> AppResult<String> {
    let managed = paths::managed_exe(app)?;
    if managed.exists() {
        Ok(managed.to_string_lossy().to_string())
    } else {
        Err(AppError::Msg(
            "Pinggy runtime is not installed. Install it from Runtime first".into(),
        ))
    }
}

pub fn runtime_env<R: Runtime>(app: &AppHandle<R>) -> AppResult<Vec<WatchdogEnvVar>> {
    let home = paths::home_dir(app)?;
    let config = paths::config_home_dir(app)?;
    let state = paths::state_dir(app)?;
    fs::create_dir_all(&home)?;
    fs::create_dir_all(&config)?;
    fs::create_dir_all(&state)?;
    let env = if cfg!(target_os = "windows") {
        vec![
            env_var("USERPROFILE", &home),
            env_var("APPDATA", &config),
            env_var("LOCALAPPDATA", &state),
        ]
    } else {
        vec![
            env_var("HOME", &home),
            env_var("XDG_CONFIG_HOME", &config),
            env_var("XDG_STATE_HOME", &state),
        ]
    };
    Ok(env)
}

fn env_var(name: &str, path: &Path) -> WatchdogEnvVar {
    WatchdogEnvVar {
        name: name.into(),
        value: path.to_string_lossy().to_string(),
    }
}

pub fn status(app: &AppHandle, _state: &AppState) -> ProviderStatus {
    let managed_exe = paths::managed_exe(app).ok();
    let version_output = managed_exe
        .as_ref()
        .filter(|path| path.exists())
        .and_then(|path| {
            let mut command = Command::new(path);
            if let Ok(env) = runtime_env(app) {
                apply_env(&mut command, &env);
            }
            command.arg("--version").output().ok()
        });
    let (available, version, message) = match version_output {
        Some(output) if output.status.success() => {
            let text = command_output_text(&output);
            (
                true,
                (!text.is_empty()).then_some(text),
                "Pinggy runtime is installed".into(),
            )
        }
        Some(output) => {
            let text = command_output_text(&output);
            (
                false,
                (!text.is_empty()).then_some(text.clone()),
                if text.is_empty() {
                    "Pinggy runtime is unavailable".into()
                } else {
                    text
                },
            )
        }
        None => (false, None, "Pinggy runtime is not installed".into()),
    };
    ProviderStatus {
        provider_id: PINGGY_PROVIDER_ID.into(),
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
    let release = latest_release().await?;
    let asset = select_asset(&release)?;
    let bin_dir = paths::bin_dir(&app)?;
    tokio::fs::create_dir_all(&bin_dir).await?;
    let dest = paths::managed_exe(&app)?;
    let exe_tmp = temp_file(&bin_dir, paths::exe_name(), "exe");
    if let Err(error) = download_with_progress(
        &app,
        PINGGY_PROVIDER_ID,
        RUNTIME_NAME,
        &asset.browser_download_url,
        &exe_tmp,
        MAX_DOWNLOAD_BYTES,
    )
    .await
    {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    if let Err(error) = set_executable(&exe_tmp) {
        let _ = tokio::fs::remove_file(&exe_tmp).await;
        return Err(error);
    }
    let mut command = Command::new(&exe_tmp);
    let env = match runtime_env(&app) {
        Ok(env) => env,
        Err(error) => {
            let _ = tokio::fs::remove_file(&exe_tmp).await;
            return Err(error);
        }
    };
    apply_env(&mut command, &env);
    command.arg("--version");
    if let Err(error) = validate_executable(&mut command, RUNTIME_NAME) {
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
    stop_daemon(app)?;
    let exe = paths::managed_exe(app)?;
    if exe.exists() {
        fs::remove_file(&exe)?;
    }
    Ok(status(app, state))
}

pub async fn check_update(
    app: AppHandle,
    state: AppState,
) -> AppResult<ProviderRuntimeUpdateStatus> {
    let current = status(&app, &state);
    let release = latest_release().await?;
    let current_version = current.version.as_deref().and_then(extract_version);
    let latest_version = Some(release.tag_name.trim_start_matches('v').to_string());
    let update_available = current.available
        && current_version.is_some()
        && current_version.as_deref() != latest_version.as_deref();
    Ok(ProviderRuntimeUpdateStatus {
        provider_id: PINGGY_PROVIDER_ID.into(),
        runtime: RUNTIME_NAME.into(),
        current_version,
        latest_version,
        update_available,
        message: String::new(),
    })
}

fn extract_version(text: &str) -> Option<String> {
    text.split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(|part| part.trim_start_matches('v').to_string())
}

pub fn stop_daemon<R: Runtime>(app: &AppHandle<R>) -> AppResult<()> {
    let exe = paths::managed_exe(app)?;
    if !exe.exists() {
        return Ok(());
    }
    let env = runtime_env(app)?;
    let mut command = Command::new(exe);
    apply_env(&mut command, &env);
    let output = command.arg("daemon").arg("stop").output()?;
    if output.status.success() {
        return Ok(());
    }
    let text = command_output_text(&output);
    if text.to_ascii_lowercase().contains("not running") {
        return Ok(());
    }
    Err(AppError::Msg(if text.is_empty() {
        "Pinggy daemon stop failed".into()
    } else {
        text
    }))
}

fn apply_env(command: &mut Command, env: &[WatchdogEnvVar]) {
    for item in env {
        if !item.name.trim().is_empty() {
            command.env(item.name.trim(), item.value.as_str());
        }
    }
}

fn ensure_not_running(state: &AppState) -> AppResult<()> {
    let running = state
        .provider_runtime
        .active_keys()
        .iter()
        .any(|(provider_id, _)| provider_id == PINGGY_PROVIDER_ID);
    if running {
        Err(AppError::Msg(
            "Stop running Pinggy connections first".into(),
        ))
    } else {
        Ok(())
    }
}

async fn latest_release() -> AppResult<GithubRelease> {
    let client = reqwest::Client::builder()
        .user_agent("tunnelx")
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()?;
    let release = client
        .get("https://api.github.com/repos/Pinggy-io/cli-js/releases/latest")
        .send()
        .await?
        .error_for_status()?
        .json::<GithubRelease>()
        .await?;
    Ok(release)
}

fn select_asset(release: &GithubRelease) -> AppResult<&GithubAsset> {
    let expected = asset_name()?;
    release
        .assets
        .iter()
        .find(|asset| asset.name == expected)
        .ok_or_else(|| {
            AppError::Msg(format!(
                "Pinggy release {} does not include asset {expected}",
                release.tag_name
            ))
        })
}

fn asset_name() -> AppResult<String> {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "win"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(AppError::Msg(
            "Automatic Pinggy installation is not supported on this operating system".into(),
        ));
    };
    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        return Err(AppError::Msg(
            "Automatic Pinggy installation is not supported on this CPU architecture".into(),
        ));
    };
    let suffix = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    Ok(format!("pinggy-{os}-{arch}{suffix}"))
}
