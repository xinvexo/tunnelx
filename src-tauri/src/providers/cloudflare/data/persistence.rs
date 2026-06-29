use crate::error::AppResult;
use crate::providers::cloudflare::account;
use crate::providers::contract::{ProviderTunnelsUpdatedEvent, CLOUDFLARE_PROVIDER_ID};
use crate::state::AppState;
use crate::store::save_app_data;
use tauri::{AppHandle, Emitter, Runtime};

pub(crate) fn emit_update<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let _ = app.emit("cloudflare-updated", account::data(state));
    let _ = app.emit(
        "provider-tunnels-updated",
        ProviderTunnelsUpdatedEvent {
            provider_id: CLOUDFLARE_PROVIDER_ID.into(),
        },
    );
}

pub(crate) fn save_change<T>(
    app: &AppHandle<impl Runtime>,
    state: &AppState,
    change: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let before = state.config.snapshot()?;
    let result = change()?;
    if let Err(error) = save_app_data(app, state) {
        state.config.replace(before);
        return Err(error);
    }
    emit_update(app, state);
    Ok(result)
}
