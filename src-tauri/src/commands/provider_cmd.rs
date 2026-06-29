use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    CreateTunnelInput, ProviderCommandOutput, ProviderDescriptor, ProviderMetrics, ProviderStatus,
    RuntimeMetrics, RuntimeTrafficSummary, TunnelResource, TunnelRuntimeInfo,
};
use crate::providers::registry;
use crate::services::watchdog_log;
use crate::state::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn list_providers() -> Vec<ProviderDescriptor> {
    registry::descriptors()
}

#[tauri::command]
pub fn connection_order(state: State<'_, AppState>) -> Vec<String> {
    state.config.connection_order()
}

#[tauri::command]
pub async fn reorder_connections(
    app: AppHandle,
    state: State<'_, AppState>,
    keys: Vec<String>,
) -> AppResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry::reorder_tunnels(&app, &state, keys))
        .await
        .map_err(|error| AppError::Msg(format!("Connection reorder task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_status(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
) -> AppResult<ProviderStatus> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry::status(&app, &state, &provider_id))
        .await
        .map_err(|error| AppError::Msg(format!("Provider status task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_login(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
) -> AppResult<ProviderCommandOutput> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry::login(&app, &state, &provider_id))
        .await
        .map_err(|error| AppError::Msg(format!("Provider login task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_tunnels(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
) -> AppResult<Vec<TunnelResource>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry::list_tunnels(&app, &state, &provider_id))
        .await
        .map_err(|error| AppError::Msg(format!("Provider tunnel list task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_create_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    input: CreateTunnelInput,
) -> AppResult<TunnelResource> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::create_tunnel(&app, &state, &provider_id, input)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel create task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_update_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    tunnel: TunnelResource,
) -> AppResult<TunnelResource> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::update_tunnel(&app, &state, &provider_id, tunnel)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel update task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_duplicate_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    id: String,
) -> AppResult<TunnelResource> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::duplicate_tunnel(&app, &state, &provider_id, &id)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel duplicate task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_delete_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    id: String,
    remote: bool,
) -> AppResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::delete_tunnel(&app, &state, &provider_id, &id, remote)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel delete task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_start_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    id: String,
) -> AppResult<TunnelRuntimeInfo> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::start_tunnel(&app, &state, &provider_id, &id)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel start task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_stop_tunnel(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    id: String,
) -> AppResult<TunnelRuntimeInfo> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::stop_tunnel(&app, &state, &provider_id, &id)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel stop task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_tunnel_status(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    id: String,
) -> AppResult<TunnelRuntimeInfo> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::tunnel_status(&app, &state, &provider_id, &id)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel status task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_tunnel_logs(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
    id: String,
) -> AppResult<Vec<String>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        registry::tunnel_logs(&app, &state, &provider_id, &id)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Provider tunnel logs task failed: {error}")))?
}

#[tauri::command]
pub async fn watchdog_logs(state: State<'_, AppState>) -> AppResult<Vec<String>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || Ok(watchdog_log::logs(&state)))
        .await
        .map_err(|error| AppError::Msg(format!("Watchdog logs task failed: {error}")))?
}

#[tauri::command]
pub async fn provider_metrics(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
) -> AppResult<ProviderMetrics> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry::metrics(&app, &state, &provider_id))
        .await
        .map_err(|error| AppError::Msg(format!("Provider metrics task failed: {error}")))?
}

#[tauri::command]
pub async fn runtime_metrics(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<RuntimeMetrics> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry::runtime_metrics(&app, &state))
        .await
        .map_err(|error| AppError::Msg(format!("Runtime metrics task failed: {error}")))
}

#[tauri::command]
pub async fn runtime_traffic_summary(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<RuntimeTrafficSummary> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || registry::runtime_traffic_summary(&app, &state))
        .await
        .map_err(|error| AppError::Msg(format!("Runtime traffic summary task failed: {error}")))
}
