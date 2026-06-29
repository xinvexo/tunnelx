use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    TunnelRuntimeInfo, TunnelRuntimeState, TunnelRuntimeStatusEvent, PROVIDER_STOP_WAIT_POLL,
    PROVIDER_STOP_WAIT_TIMEOUT,
};
use crate::services::provider_log;
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Runtime};

pub const START_CONFIRMATION_TIMED_OUT: &str = "start confirmation timed out";

pub fn start_confirmation_timed_out_message(reason: impl AsRef<str>) -> String {
    let reason = reason.as_ref().trim();
    if reason.is_empty() {
        START_CONFIRMATION_TIMED_OUT.to_string()
    } else {
        format!("{START_CONFIRMATION_TIMED_OUT}: {reason}")
    }
}

pub fn mark_start_confirmation_timed_out<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    reason: impl AsRef<str>,
) -> TunnelRuntimeInfo {
    let message = start_confirmation_timed_out_message(reason);
    let current = state.provider_runtime.info(provider_id, tunnel_id);
    if current.status != TunnelRuntimeState::Starting {
        provider_log::emit_system(app, state, provider_id, tunnel_id, message);
        return current;
    }
    provider_log::emit_system(app, state, provider_id, tunnel_id, message.clone());
    let info = state.provider_runtime.mark_status(
        provider_id,
        tunnel_id,
        TunnelRuntimeState::Warning,
        message,
    );
    emit_status(app, info.clone());
    info
}

pub fn wait_for_inactive_before_delete<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    current: TunnelRuntimeInfo,
) -> AppResult<TunnelRuntimeInfo> {
    if !current.status.is_active() {
        return Ok(current);
    }
    match state.provider_runtime.wait_for_inactive(
        provider_id,
        tunnel_id,
        PROVIDER_STOP_WAIT_TIMEOUT,
        PROVIDER_STOP_WAIT_POLL,
    ) {
        Some(info) => Ok(info),
        None => {
            let message = format!("provider {provider_id} connection did not stop before delete");
            provider_log::emit_system(app, state, provider_id, tunnel_id, message.clone());
            let info = state.provider_runtime.mark_status(
                provider_id,
                tunnel_id,
                TunnelRuntimeState::Errored,
                message.clone(),
            );
            emit_status(app, info);
            Err(AppError::Msg(message))
        }
    }
}

pub fn mark_errored_with_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    message: impl Into<String>,
) -> TunnelRuntimeInfo {
    let message = message.into();
    provider_log::emit_system(app, state, provider_id, tunnel_id, message.clone());
    let info = state.provider_runtime.mark_status(
        provider_id,
        tunnel_id,
        TunnelRuntimeState::Errored,
        message,
    );
    emit_status(app, info.clone());
    info
}

fn emit_status<R: Runtime>(app: &AppHandle<R>, info: TunnelRuntimeInfo) {
    let _ = app.emit(
        "provider-tunnel-status-changed",
        TunnelRuntimeStatusEvent { info },
    );
    crate::refresh_connection_icon(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_confirmation_timeout_message_has_common_shape() {
        assert_eq!(
            start_confirmation_timed_out_message("public URLs were not reported"),
            "start confirmation timed out: public URLs were not reported"
        );
        assert_eq!(
            start_confirmation_timed_out_message(""),
            START_CONFIRMATION_TIMED_OUT
        );
    }

    // 回归：移除按时间假装连上的 grace 后，启动确认超时必须把仍在 Starting
    // 的隧道降级为 Warning（真实失败信号能落地）；而已确认为 Running 的隧道
    // 不被超时降级——这正说明旧的 2s grace 先置 Running 会掩盖失败。
    #[test]
    fn start_confirmation_timeout_downgrades_only_starting() {
        let state = AppState::default();
        let app = tauri::test::mock_builder()
            .manage(state.clone())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app_handle = app.handle().clone();

        let provider = "ngrok";

        // 进程刚起来仍是 Starting；超时 → Warning。
        state
            .provider_runtime
            .begin_start(provider, "t1", "starting")
            .unwrap();
        let info = mark_start_confirmation_timed_out(&app_handle, &state, provider, "t1", "no url");
        assert_eq!(info.status, TunnelRuntimeState::Warning);

        // 已确认 Running 的隧道，超时不应降级。
        state
            .provider_runtime
            .begin_start(provider, "t2", "starting")
            .unwrap();
        state
            .provider_runtime
            .mark_status(provider, "t2", TunnelRuntimeState::Running, "running");
        let info = mark_start_confirmation_timed_out(&app_handle, &state, provider, "t2", "no url");
        assert_eq!(info.status, TunnelRuntimeState::Running);
    }
}
