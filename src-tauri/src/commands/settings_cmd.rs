use crate::domain::AppSettings;
use crate::error::{AppError, AppResult};
use crate::providers::registry;
use crate::state::AppState;
use crate::store::save_app_data;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsOptions {
    silent_start: bool,
    auto_connect: bool,
    lightweight_mode: bool,
    auto_update: bool,
    traffic_stats_enabled: bool,
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> AppSettings {
    state.config.settings()
}

#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<AppState>,
    options: AppSettingsOptions,
) -> AppResult<()> {
    let before = state.config.snapshot()?;
    let previous = before.settings.clone();
    state.config.set_options(
        options.silent_start,
        options.auto_connect,
        options.lightweight_mode,
        options.auto_update,
        options.traffic_stats_enabled,
    )?;
    if let Err(error) = save_app_data(&app, &state) {
        state.config.replace(before);
        return Err(error);
    }
    let next = state.config.settings();
    let _ = app.emit("settings-updated", next.clone());
    registry::settings_changed(&app, &state, &previous, &next);
    Ok(())
}

#[tauri::command]
pub fn set_tray_locale(app: AppHandle, locale: String) -> AppResult<()> {
    crate::set_tray_locale(&app, &locale).map_err(|e| AppError::Msg(e.to_string()))?;
    Ok(())
}
