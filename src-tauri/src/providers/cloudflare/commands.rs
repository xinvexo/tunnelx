use crate::error::AppResult;
use crate::providers::cloudflare::domain::{
    CloudflareAccount, CloudflareData, CloudflareTunnel, CloudflareZone, CloudflaredStatus,
};
use crate::providers::cloudflare::services as svc;
use crate::providers::contract::{
    ProviderCommandOutput, ProviderRuntimeUpdateStatus, CLOUDFLARE_PROVIDER_ID,
};
use crate::services::provider_log;
use crate::state::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn cloudflare_data(state: State<AppState>) -> CloudflareData {
    svc::data(&state)
}

#[tauri::command]
pub async fn cloudflare_login_connection(
    app: AppHandle,
    state: State<'_, AppState>,
    tunnel_id: String,
) -> AppResult<ProviderCommandOutput> {
    let state = state.inner().clone();
    svc::login_connection(&app, &state, tunnel_id)
        .await
        .map(Into::into)
}

#[tauri::command]
pub async fn cloudflare_install_cloudflared(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<CloudflaredStatus> {
    svc::install(app, state.inner().clone()).await
}

#[tauri::command]
pub async fn cloudflare_check_cloudflared_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ProviderRuntimeUpdateStatus> {
    svc::check_update(app, state.inner().clone()).await
}

#[tauri::command]
pub fn cloudflare_uninstall_cloudflared(
    app: AppHandle,
    state: State<AppState>,
) -> AppResult<CloudflaredStatus> {
    svc::uninstall(&app, &state)
}

#[tauri::command]
pub async fn cloudflare_verify_token(
    app: AppHandle,
    state: State<'_, AppState>,
    tunnel_id: String,
) -> AppResult<bool> {
    let state = state.inner().clone();
    provider_log::credential_verification_async(
        &app,
        &state,
        CLOUDFLARE_PROVIDER_ID,
        &tunnel_id,
        "Cloudflare",
        "API token",
        || svc::verify_token(state.clone(), tunnel_id.clone()),
    )
    .await
}

#[tauri::command]
pub async fn cloudflare_list_accounts(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> AppResult<Vec<CloudflareAccount>> {
    svc::list_accounts(state.inner().clone(), tunnel_id).await
}

#[tauri::command]
pub async fn cloudflare_list_zones(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> AppResult<Vec<CloudflareZone>> {
    svc::list_zones(state.inner().clone(), tunnel_id).await
}

#[tauri::command]
pub async fn cloudflare_list_remote_tunnels(
    state: State<'_, AppState>,
    tunnel_id: String,
) -> AppResult<Vec<CloudflareTunnel>> {
    svc::list_remote_tunnels(state.inner().clone(), tunnel_id).await
}
