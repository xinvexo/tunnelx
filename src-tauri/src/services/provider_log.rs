use crate::error::AppResult;
use crate::providers::contract::TunnelRuntimeLogEvent;
use crate::state::AppState;
use std::future::Future;
use tauri::{AppHandle, Emitter, Runtime};

pub const PROVIDER_LOG_EVENT: &str = "provider-tunnel-log";
pub const WATCHDOG_STREAM_CLOSED_MESSAGE: &str =
    "watchdog event stream closed; reconciling connection state";

pub fn emit_system<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    line: impl Into<String>,
) {
    if let Some(line) = system_line(provider_id, line.into()) {
        emit_line(app, state, provider_id, tunnel_id, line);
    }
}

pub fn emit_native<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    line: impl Into<String>,
) {
    for line in native_lines(provider_id, line.into()) {
        emit_line(app, state, provider_id, tunnel_id, line);
    }
}

pub fn emit_watchdog_stream_closed<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
) {
    emit_system(
        app,
        state,
        provider_id,
        tunnel_id,
        WATCHDOG_STREAM_CLOSED_MESSAGE,
    );
}

pub fn watchdog_recovered_line(process_name: &str) -> String {
    format!("{} recovered from watchdog snapshot", process_name.trim())
}

pub fn watchdog_error_line(message: impl AsRef<str>) -> String {
    format!("[watchdog] {}", message.as_ref().trim())
}

pub fn watchdog_exit_state_message(
    process_name: &str,
    success: bool,
    cleanup_success: Option<bool>,
    cleanup_error: Option<&str>,
) -> String {
    watchdog_exit_state_message_with_cleanup(
        process_name,
        "cleanup",
        success,
        cleanup_success,
        cleanup_error,
    )
}

pub fn watchdog_exit_state_message_with_cleanup(
    process_name: &str,
    cleanup_name: &str,
    success: bool,
    cleanup_success: Option<bool>,
    cleanup_error: Option<&str>,
) -> String {
    let process_name = process_name.trim();
    let cleanup_name = cleanup_name.trim();
    if !success {
        let base = format!("{process_name} exited unexpectedly");
        return append_cleanup_failure(base, cleanup_name, cleanup_success, cleanup_error);
    }
    if cleanup_success == Some(false) {
        return cleanup_unconfirmed_line(process_name, cleanup_name, cleanup_error);
    }
    format!("{process_name} stopped")
}

pub fn cleanup_unconfirmed_line(
    process_name: &str,
    cleanup_name: &str,
    cleanup_error: Option<&str>,
) -> String {
    format!(
        "{} {} was not confirmed: {}",
        process_name.trim(),
        cleanup_name.trim(),
        cleanup_failure_detail(cleanup_error)
    )
}

fn cleanup_failure_detail(cleanup_error: Option<&str>) -> &str {
    cleanup_error
        .map(str::trim)
        .filter(|error| !error.is_empty())
        .unwrap_or("watchdog did not report cleanup result")
}

pub fn emit_credential_verification_started<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    provider_name: &str,
    credential_name: &str,
) {
    emit_system(
        app,
        state,
        provider_id,
        tunnel_id,
        credential_verification_line(provider_name, credential_name, "started", None),
    );
}

pub fn emit_credential_verification_succeeded<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    provider_name: &str,
    credential_name: &str,
) {
    emit_system(
        app,
        state,
        provider_id,
        tunnel_id,
        credential_verification_line(provider_name, credential_name, "succeeded", None),
    );
}

pub fn emit_credential_verification_failed<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    provider_name: &str,
    credential_name: &str,
    error: impl std::fmt::Display,
) {
    emit_system(
        app,
        state,
        provider_id,
        tunnel_id,
        credential_verification_line(
            provider_name,
            credential_name,
            "failed",
            Some(error.to_string()),
        ),
    );
}

pub fn credential_verification<R: Runtime, T>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    provider_name: &str,
    credential_name: &str,
    action: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    emit_credential_verification_started(
        app,
        state,
        provider_id,
        tunnel_id,
        provider_name,
        credential_name,
    );
    match action() {
        Ok(value) => {
            emit_credential_verification_succeeded(
                app,
                state,
                provider_id,
                tunnel_id,
                provider_name,
                credential_name,
            );
            Ok(value)
        }
        Err(error) => {
            emit_credential_verification_failed(
                app,
                state,
                provider_id,
                tunnel_id,
                provider_name,
                credential_name,
                &error,
            );
            Err(error)
        }
    }
}

pub async fn credential_verification_async<R: Runtime, T, F>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    provider_name: &str,
    credential_name: &str,
    action: impl FnOnce() -> F,
) -> AppResult<T>
where
    F: Future<Output = AppResult<T>>,
{
    emit_credential_verification_started(
        app,
        state,
        provider_id,
        tunnel_id,
        provider_name,
        credential_name,
    );
    match action().await {
        Ok(value) => {
            emit_credential_verification_succeeded(
                app,
                state,
                provider_id,
                tunnel_id,
                provider_name,
                credential_name,
            );
            Ok(value)
        }
        Err(error) => {
            emit_credential_verification_failed(
                app,
                state,
                provider_id,
                tunnel_id,
                provider_name,
                credential_name,
                &error,
            );
            Err(error)
        }
    }
}

pub fn clear<R: Runtime>(app: &AppHandle<R>, state: &AppState, provider_id: &str, tunnel_id: &str) {
    state.provider_runtime.clear_logs(provider_id, tunnel_id);
    let _ = app.emit(
        PROVIDER_LOG_EVENT,
        TunnelRuntimeLogEvent {
            provider_id: provider_id.to_string(),
            tunnel_id: tunnel_id.to_string(),
            line: String::new(),
            reset: true,
        },
    );
}

fn emit_line<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    line: String,
) {
    let pushed = state
        .provider_runtime
        .push_log(provider_id, tunnel_id, line.clone());
    if !pushed {
        return;
    }
    let _ = app.emit(
        PROVIDER_LOG_EVENT,
        TunnelRuntimeLogEvent {
            provider_id: provider_id.to_string(),
            tunnel_id: tunnel_id.to_string(),
            line,
            reset: false,
        },
    );
}

pub fn logs(state: &AppState, provider_id: &str, tunnel_id: &str) -> Vec<String> {
    state.provider_runtime.logs(provider_id, tunnel_id)
}

fn credential_verification_line(
    provider_name: &str,
    credential_name: &str,
    status: &str,
    error: Option<String>,
) -> String {
    let base = format!(
        "{} {} verification {}",
        provider_name.trim(),
        credential_name.trim(),
        status.trim()
    );
    match error {
        Some(error) if !error.trim().is_empty() => format!("{base}: {error}"),
        _ => base,
    }
}

mod native;

pub(crate) use native::sanitize_line;
#[cfg(test)]
use native::*;
use native::{append_cleanup_failure, native_lines, system_line};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_line_adds_timestamp_to_plain_logs() {
        let line = system_line("cloudflare", "cloudflared starting".into()).unwrap();

        assert!(has_leading_timestamp(&line));
        assert!(line.ends_with(" [INFO] cloudflared starting"));
    }

    #[test]
    fn system_line_standardizes_existing_timestamp_and_level() {
        let line = "2026-06-26 12:34:56.789 [I] [frpc] [-] frpc starting";

        assert_eq!(
            system_line("frp", line.into()).unwrap(),
            "2026-06-26 12:34:56 [INFO] [frpc] [-] frpc starting"
        );
    }

    #[test]
    fn system_line_standardizes_rfc3339_like_timestamp() {
        let line = "2026-06-26T12:34:56Z cloudflared started";

        assert_eq!(
            system_line("cloudflare", line.into()).unwrap(),
            "2026-06-26 12:34:56 [INFO] cloudflared started"
        );
    }

    #[test]
    fn system_line_drops_messages_that_normalize_to_empty() {
        assert!(system_line("cpolar", "2026-06-28 03:52:05 [INFO]".into()).is_none());
    }

    #[test]
    fn native_lines_standardize_frp_source_logs() {
        let line = "2026/06/26 12:34:56 [I] [service.go:308] [web] start proxy success";

        assert_eq!(
            native_lines("frp", line.into()),
            vec!["2026-06-26 12:34:56 [INFO] [web] start proxy success"]
        );
    }

    #[test]
    fn credential_verification_line_uses_common_english_shape() {
        assert_eq!(
            credential_verification_line(
                "Cloudflare",
                "API token",
                "failed",
                Some("denied".into())
            ),
            "Cloudflare API token verification failed: denied"
        );
    }

    #[test]
    fn watchdog_lifecycle_lines_use_common_shape() {
        assert_eq!(
            watchdog_recovered_line("cloudflared"),
            "cloudflared recovered from watchdog snapshot"
        );
        assert_eq!(
            watchdog_error_line("spawn failed"),
            "[watchdog] spawn failed"
        );
        assert_eq!(
            watchdog_exit_state_message("ngrok", true, Some(true), None),
            "ngrok stopped"
        );
        assert_eq!(
            watchdog_exit_state_message("ngrok", false, Some(true), None),
            "ngrok exited unexpectedly"
        );
        assert_eq!(
            watchdog_exit_state_message("ngrok", true, Some(false), Some("remote cleanup failed")),
            "ngrok cleanup was not confirmed: remote cleanup failed"
        );
        assert_eq!(
            watchdog_exit_state_message("ngrok", false, Some(false), Some("remote cleanup failed")),
            "ngrok exited unexpectedly; cleanup was not confirmed: remote cleanup failed"
        );
    }

    #[test]
    fn native_line_removes_ansi_sequences_and_control_chars() {
        let line = native_line("\x1b[31mred\x1b[0m\u{7}\tOK".into());

        assert!(has_leading_timestamp(&line));
        assert!(line.ends_with(" [INFO] red OK"));
    }

    #[test]
    fn native_line_handles_bare_escape_before_multibyte_char() {
        let line = native_line("before \x1b中文 after".into());

        assert!(has_leading_timestamp(&line));
        assert!(line.ends_with(" [INFO] before 文 after"));
    }

    #[test]
    fn native_line_redacts_sensitive_cli_and_json_values() {
        let line = r#"starting --token secret-123 --authtoken abc https://api.example.test?token=query-token&ok=1 {"token":"value","authorization":"Bearer xyz","Authorization":"Basic abc","TunnelSecret":"secret-value","client_secret":"client-value"}"#;

        let cleaned = native_line(line.into());

        assert!(cleaned.contains("--token ***"));
        assert!(cleaned.contains("--authtoken ***"));
        assert!(cleaned.contains("token=***"));
        assert!(cleaned.contains(r#""token":"***""#));
        assert!(cleaned.contains(r#""authorization":"***""#));
        assert!(cleaned.contains(r#""Authorization":"***""#));
        assert!(cleaned.contains(r#""TunnelSecret":"***""#));
        assert!(cleaned.contains(r#""client_secret":"***""#));
        assert!(!cleaned.contains("secret-123"));
        assert!(!cleaned.contains("query-token"));
        assert!(!cleaned.contains("Bearer xyz"));
        assert!(!cleaned.contains("Basic abc"));
        assert!(!cleaned.contains("secret-value"));
        assert!(!cleaned.contains("client-value"));
    }

    #[test]
    fn native_line_compacts_pinggy_daemon_spawn_options() {
        let line = "2026-06-26T22:20:43.590Z [\x1b[32minfo\x1b[39m] Spawning daemon process with options: {\"args\":[\"--token\",\"secret\"]}";

        assert_eq!(
            native_line(line.into()),
            "2026-06-26 22:20:43 [INFO] Spawning daemon process"
        );
    }

    #[test]
    fn native_lines_standardize_cpolar_logfmt() {
        let line = r#"time="2026-06-26T12:34:56Z" level=debug msg="request accepted" obj="{\"token\":\"secret\"}""#;

        assert!(native_lines("cpolar", line.into()).is_empty());
    }

    #[test]
    fn native_lines_standardize_cloudflared_logs_and_hide_paths() {
        let line = "2026-06-26T12:34:56Z INF Registered tunnel connection connIndex=0 credentialsFile=/Users/example/Library/Application Support/TunnelX/cloudflared.json";

        let lines = native_lines("cloudflare", line.into());

        assert_eq!(
            lines,
            vec!["2026-06-26 12:34:56 [INFO] Registered tunnel connection"]
        );
        assert!(!lines[0].contains("/Users/example"));
    }

    #[test]
    fn native_lines_standardize_pinggy_tui_output() {
        let line = "\rwaiting for connection...\r✔ Tunnel: tcp://example.tcp.pinggy.io:12345\rAuthenticated as: user@example.test";

        let lines = native_lines("pinggy", line.into());

        assert_eq!(lines.len(), 1);
        assert!(lines[0].ends_with(" [INFO] authenticated"));
        assert!(!lines.join("\n").contains("user@example.test"));
        assert!(!lines.join("\n").contains("waiting for connection"));
    }

    #[test]
    fn native_lines_drop_cpolar_protocol_chatter() {
        let line = r#"time="2026-06-26T12:34:56Z" level=debug msg="[ctl:ctl:1d1d5d18] Read message {\"Type\":\"Pong\",\"Payload\":{\"ReqId\":\"\",\"Msg\":\"\"}}""#;

        assert!(native_lines("cpolar", line.into()).is_empty());
    }

    #[test]
    fn native_lines_summarize_frp_config_paths() {
        let line = "2026-06-28 03:25:35.969 [I] [service.go:308] start frpc service for config file [<path> Support/com.xin.tunnelx/runtime/profile.json] with aggregated configuration";

        assert_eq!(
            native_lines("frp", line.into()),
            vec!["2026-06-28 03:25:35 [INFO] start frpc service with aggregated configuration"]
        );
    }

    #[test]
    fn native_lines_drop_pinggy_cli_noise() {
        let line = "[INFO] [pinggy] 2026-06-27T19:25:36.136Z [info] Spawning daemon child {\"command\":\"<path> Support/com.xin.tunnelx/pinggy/bin/pinggy\"}\n[INFO] [pinggy] ________________________________\n[INFO] [pinggy] public URL ready: https://dashboard.pinggy.io";

        assert!(native_lines("pinggy", line.into()).is_empty());
    }

    #[test]
    fn native_lines_drop_prefixed_cpolar_logfmt_noise() {
        let line = r#"[DEBUG] time="2026-06-28T03:34:47+08:00" level=debug msg="event <-updates""#;

        assert!(native_lines("cpolar", line.into()).is_empty());
    }

    #[test]
    fn native_lines_drop_cpolar_timestamp_debug_noise() {
        let line = "2026-06-28 03:34:45 [DEBUG] New connection to: 198.18.111.238:4443";

        assert!(native_lines("cpolar", line.into()).is_empty());
    }

    #[test]
    fn native_lines_drop_cpolar_proxy_close_noise() {
        let close_line = "2026-06-28 03:34:45 [INFO] Close with connection pxy:abc123";
        let eof_line =
            "2026-06-28 03:34:45 [ERROR] Server failed to read StartProxy with error: EOF";
        let closed_line = "2026-06-28 03:34:45 [ERROR] Server failed to read StartProxy with error: read tcp 127.0.0.1:1->127.0.0.1:2: use of closed network connection";

        assert!(native_lines("cpolar", close_line.into()).is_empty());
        assert!(native_lines("cpolar", eof_line.into()).is_empty());
        assert!(native_lines("cpolar", closed_line.into()).is_empty());
    }

    #[test]
    fn native_lines_drop_prefixed_cloudflared_boot_noise() {
        let line = "[INFO] 2026-06-27T19:34:51Z INF Version 2026.6.1 (Checksum abc)";

        assert_eq!(
            strip_leading_level_prefix(line),
            Some("2026-06-27T19:34:51Z INF Version 2026.6.1 (Checksum abc)")
        );
        assert!(take_leading_timestamp("2026-06-27T19:34:51Z INF Version").is_some());
        assert!(cleanup_cloudflared_message(NativeLogLevel::Info, "Version 2026.6.1").is_none());
        assert!(native_lines("cloudflare", line.into()).is_empty());
    }

    #[test]
    fn native_lines_drop_cloudflared_precheck_table() {
        let line = "2026-06-27 19:35:01 [INFO] | UDP Connectivity region1.v2.argotunnel.com FAIL QUIC connection failed |";

        assert!(native_lines("cloudflare", line.into()).is_empty());
    }

    #[test]
    fn native_lines_remove_cloudflared_internal_fields() {
        let line = "2026-06-27T19:34:56Z ERR Failed to dial a quic connection error=\"failed to dial to edge with quic: timeout: no recent network activity\" connIndex=0 event=0 ip=198.18.111.139";

        assert_eq!(
            native_lines("cloudflare", line.into()),
            vec![
                "2026-06-27 19:34:56 [ERROR] Failed to dial a quic connection error=\"failed to dial to edge with quic: timeout: no recent network activity\""
            ]
        );
    }
}
