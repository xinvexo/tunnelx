//! 应用自更新：版本号查询 + 经 tauri-plugin-updater 检查 / 安装。
use crate::error::{AppError, AppResult};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

pub const APP_UPDATE_PROGRESS_EVENT: &str = "app-update-progress";

/// 检查更新的结果（字段名对齐 SystemSettings.vue）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub latest_version: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateProgress {
    pub received_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: Option<f64>,
}

/// 当前应用版本（取自打包元信息）。
pub fn app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

/// 查询是否有新版本可用。
pub async fn check_update(app: &AppHandle) -> AppResult<UpdateInfo> {
    let current = app_version(app);
    let updater = app
        .updater()
        .map_err(|e| AppError::Msg(format!("Updater initialization failed: {e}")))?;
    match updater
        .check()
        .await
        .map_err(|e| AppError::Msg(format!("Update check failed: {e}")))?
    {
        Some(update) => Ok(UpdateInfo {
            available: true,
            current_version: update.current_version.clone(),
            latest_version: update.version.clone(),
            notes: update.body.clone().unwrap_or_default(),
        }),
        None => Ok(UpdateInfo {
            available: false,
            current_version: current.clone(),
            latest_version: current,
            notes: String::new(),
        }),
    }
}

/// 下载并安装最新版本，随后重启应用。无可用更新时静默返回。
pub async fn install_update(app: &AppHandle) -> AppResult<()> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Msg(format!("Updater initialization failed: {e}")))?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| AppError::Msg(format!("Update check failed: {e}")))?
    else {
        return Ok(());
    };
    let mut received_bytes = 0_u64;
    let app_for_progress = app.clone();
    update
        .download_and_install(
            move |chunk: usize, total: Option<u64>| {
                received_bytes = received_bytes.saturating_add(chunk as u64);
                emit_progress(&app_for_progress, received_bytes, total);
            },
            || {
                emit_progress(app, received_bytes, Some(received_bytes));
            },
        )
        .await
        .map_err(|e| AppError::Msg(format!("Update installation failed: {e}")))?;
    app.restart();
}

fn emit_progress(app: &AppHandle, received_bytes: u64, total_bytes: Option<u64>) {
    let percent = total_bytes
        .filter(|total| *total > 0)
        .map(|total| (received_bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0));
    let _ = app.emit(
        APP_UPDATE_PROGRESS_EVENT,
        AppUpdateProgress {
            received_bytes,
            total_bytes,
            percent,
        },
    );
}
