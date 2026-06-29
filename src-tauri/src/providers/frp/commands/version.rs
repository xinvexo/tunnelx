use crate::error::AppResult;
use crate::providers::contract::ProviderRuntimeUpdateStatus;
use crate::providers::frp::services::version_service::{self as svc, VersionList};
use crate::state::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn list_frpc_versions(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<VersionList> {
    let state = state.inner().clone();
    svc::list(app, state).await
}

#[tauri::command]
pub async fn check_frpc_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ProviderRuntimeUpdateStatus> {
    let state = state.inner().clone();
    svc::check_update(app, state).await
}

#[tauri::command]
pub async fn install_frpc_version(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
) -> AppResult<()> {
    let state = state.inner().clone();
    svc::install(app, state, version).await
}

#[tauri::command]
pub fn activate_frpc_version(
    app: AppHandle,
    state: State<AppState>,
    version: String,
) -> AppResult<()> {
    svc::activate(&app, &state, &version)
}

#[tauri::command]
pub fn remove_frpc_version(
    app: AppHandle,
    state: State<AppState>,
    version: String,
) -> AppResult<()> {
    svc::remove(&app, &state, &version)
}

#[tauri::command]
pub fn uninstall_active_frpc_version(app: AppHandle, state: State<AppState>) -> AppResult<()> {
    svc::uninstall_active(&app, &state)
}

#[tauri::command]
pub fn active_frpc_version(state: State<AppState>) -> Option<String> {
    svc::active_version(&state)
}

/// 读取 frpc 下载镜像前缀（空字符串=直连）。
#[tauri::command]
pub fn get_frpc_mirror(state: State<AppState>) -> String {
    state.config.frp_settings().frpc_mirror.unwrap_or_default()
}

/// 设置 frpc 下载镜像前缀并持久化。
#[tauri::command]
pub fn set_frpc_mirror(app: AppHandle, state: State<AppState>, mirror: String) -> AppResult<()> {
    let before = state.config.snapshot()?;
    state.config.set_frpc_mirror(mirror)?;
    if let Err(error) = crate::store::save_app_data(&app, &state) {
        state.config.replace(before);
        return Err(error);
    }
    Ok(())
}
