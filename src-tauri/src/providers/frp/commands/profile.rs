//! Profile 的 #[tauri::command] 入口，薄封装 profile_service。
use crate::error::AppResult;
use crate::providers::frp::domain::{Profile, ProfileSummary};
use crate::providers::frp::services::profile_service as svc;
use crate::state::AppState;
use tauri::{AppHandle, State};

#[tauri::command]
pub fn list_profiles(state: State<AppState>) -> Vec<ProfileSummary> {
    svc::list_profiles(&state)
}

#[tauri::command]
pub fn get_profile(state: State<AppState>, id: String) -> AppResult<Profile> {
    svc::get_profile(&state, &id)
}

#[tauri::command]
pub fn update_profile(
    app: AppHandle,
    state: State<AppState>,
    profile: Profile,
) -> AppResult<Profile> {
    svc::update_profile(&app, &state, profile)
}

#[tauri::command]
pub fn reorder_profiles(app: AppHandle, state: State<AppState>, ids: Vec<String>) -> AppResult<()> {
    svc::reorder_profiles(&app, &state, ids)
}
