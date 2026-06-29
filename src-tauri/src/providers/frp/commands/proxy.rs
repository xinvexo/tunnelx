use crate::error::{AppError, AppResult};
use crate::providers::frp::domain::ProfileProxy;
use crate::providers::frp::services::profile_service as svc;
use crate::state::AppState;
use tauri::{AppHandle, State};

/// 整体覆盖某个 Profile 的隧道列表（新增/编辑/删除/排序都走这里）。
#[tauri::command]
pub async fn update_proxies(
    app: AppHandle,
    state: State<'_, AppState>,
    profile_id: String,
    proxies: Vec<ProfileProxy>,
) -> AppResult<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        svc::update_proxies(&app, &state, &profile_id, proxies)
    })
    .await
    .map_err(|error| AppError::Msg(format!("Tunnel configuration update task failed: {error}")))?
}
