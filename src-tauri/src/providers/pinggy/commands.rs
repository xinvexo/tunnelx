use crate::error::{AppError, AppResult};
use crate::providers::contract::{ProviderRuntimeUpdateStatus, ProviderStatus, TunnelResource};
use crate::providers::pinggy::{credentials, environment};
use crate::providers::registry;
use crate::state::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn pinggy_install_runtime(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ProviderStatus> {
    environment::install(app, state.inner().clone()).await
}

#[tauri::command]
pub fn pinggy_uninstall_runtime(
    app: AppHandle,
    state: State<AppState>,
) -> AppResult<ProviderStatus> {
    environment::uninstall(&app, &state)
}

#[tauri::command]
pub async fn pinggy_check_runtime_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ProviderRuntimeUpdateStatus> {
    environment::check_update(app, state.inner().clone()).await
}

#[tauri::command]
pub async fn pinggy_authenticate_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    tunnel: TunnelResource,
) -> AppResult<TunnelResource> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        credentials::authenticate_tunnel(&app, &state, tunnel.into())
            .map(Into::into)
            .map(registry::redact_tunnel_resource)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Pinggy authentication task failed: {error}")))?
}
