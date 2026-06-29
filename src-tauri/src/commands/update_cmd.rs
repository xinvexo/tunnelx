//! 应用自更新的 #[tauri::command] 入口，薄封装 update_service。
use crate::error::AppResult;
use crate::services::update_service::{self as svc, UpdateInfo};
use tauri::AppHandle;

#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    svc::app_version(&app)
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> AppResult<UpdateInfo> {
    svc::check_update(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> AppResult<()> {
    svc::install_update(&app).await
}
