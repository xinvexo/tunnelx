use crate::state::AppState;
use chrono::Local;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

pub const WATCHDOG_LOG_EVENT: &str = "watchdog-log";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogLogEvent {
    pub line: String,
    pub level: String,
    pub important: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

pub fn emit<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    level: &str,
    important: bool,
    code: Option<&str>,
    line: impl Into<String>,
) {
    let line = system_line(line.into());
    state.process_watchdog.push_log(line.clone());
    let _ = app.emit(
        WATCHDOG_LOG_EVENT,
        WatchdogLogEvent {
            line,
            level: level.to_string(),
            important,
            code: code.map(str::to_string),
        },
    );
}

pub fn info<R: Runtime>(app: &AppHandle<R>, state: &AppState, line: impl Into<String>) {
    emit(app, state, "info", false, None, line);
}

pub fn warning<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    code: Option<&str>,
    line: impl Into<String>,
) {
    emit(app, state, "warning", true, code, line);
}

pub fn danger<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    code: Option<&str>,
    line: impl Into<String>,
) {
    emit(app, state, "danger", true, code, line);
}

pub fn logs(state: &AppState) -> Vec<String> {
    state.process_watchdog.logs()
}

fn system_line(line: String) -> String {
    let line = tunnelx_watchdog_protocol::redaction::redact_sensitive_text(line);
    if has_leading_timestamp(&line) {
        strip_timestamp_millis(&line)
    } else {
        format!("{} {}", Local::now().format("%Y-%m-%d %H:%M:%S"), line)
    }
}

fn strip_timestamp_millis(line: &str) -> String {
    if line.len() <= 19 || line.as_bytes().get(19) != Some(&b'.') {
        return line.to_string();
    }
    let mut rest_start = 20;
    while rest_start < line.len() {
        let Some(ch) = line[rest_start..].chars().next() else {
            break;
        };
        if !ch.is_ascii_digit() {
            break;
        }
        rest_start += ch.len_utf8();
    }
    format!("{}{}", &line[..19], &line[rest_start..])
}

fn has_leading_timestamp(line: &str) -> bool {
    let bytes = line.as_bytes();
    if bytes.len() < 19 {
        return false;
    }
    matches!(
        bytes,
        [y1, y2, y3, y4, b'-', m1, m2, b'-', d1, d2, separator, h1, h2, b':', n1, n2, b':', s1, s2, ..]
            if y1.is_ascii_digit()
                && y2.is_ascii_digit()
                && y3.is_ascii_digit()
                && y4.is_ascii_digit()
                && matches!(separator, b' ' | b'T')
                && m1.is_ascii_digit()
                && m2.is_ascii_digit()
                && d1.is_ascii_digit()
                && d2.is_ascii_digit()
                && h1.is_ascii_digit()
                && h2.is_ascii_digit()
                && n1.is_ascii_digit()
                && n2.is_ascii_digit()
                && s1.is_ascii_digit()
                && s2.is_ascii_digit()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_line_adds_timestamp_to_plain_watchdog_logs() {
        let line = system_line("watchdog started".into());
        assert!(line.ends_with(" watchdog started"));
        assert!(line[..19]
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == ':' || c == ' '));
    }

    #[test]
    fn system_line_strips_existing_timestamp_millis() {
        let line = "2026-06-27 12:00:00.000 watchdog started";
        assert_eq!(
            system_line(line.into()),
            "2026-06-27 12:00:00 watchdog started"
        );
    }

    #[test]
    fn system_line_redacts_sensitive_values() {
        let line = system_line("cleanup failed: Authorization=Bearer abc token=query-token".into());

        assert!(line.contains("Authorization=***"));
        assert!(line.contains("token=***"));
        assert!(!line.contains("Bearer abc"));
        assert!(!line.contains("query-token"));
    }
}
