use crate::state::AppState;
use std::io::{BufRead, BufReader, Read};
use tauri::{AppHandle, Runtime};
use tunnelx_watchdog_protocol::{WatchdogEvent, WatchdogFrame};

pub(super) fn spawn_event_reader<R: Runtime>(
    app: AppHandle<R>,
    state: AppState,
    reader: impl Read + Send + 'static,
    generation: u64,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let Ok(line) = line else { break };
            let frame = match serde_json::from_str::<WatchdogFrame>(&line) {
                Ok(frame) => frame,
                Err(error) => {
                    crate::diag::warn(
                        "watchdog",
                        format!(
                            "failed to parse frame: {error}; line={}",
                            safe_frame_excerpt(&line)
                        ),
                    );
                    crate::services::watchdog_log::warning(
                        &app,
                        &state,
                        Some("parse_failed"),
                        format!("watchdog frame parse failed: {error}"),
                    );
                    continue;
                }
            };
            if !frame.supported_version() {
                crate::diag::warn(
                    "watchdog",
                    format!("ignored unsupported protocol version: {}", frame.version()),
                );
                crate::services::watchdog_log::warning(
                    &app,
                    &state,
                    Some("unsupported_protocol"),
                    format!(
                        "watchdog frame ignored because protocol version is unsupported: {}",
                        frame.version()
                    ),
                );
                continue;
            }
            match frame {
                WatchdogFrame::Response { id, response, .. } => {
                    if !state.process_watchdog.complete_response(&id, response) {
                        crate::diag::warn("watchdog", format!("unmatched response id={id}"));
                        crate::services::watchdog_log::warning(
                            &app,
                            &state,
                            Some("unmatched_response"),
                            format!("watchdog response did not match a pending request: {id}"),
                        );
                    }
                }
                WatchdogFrame::Event { event, .. } => dispatch_event(&app, &state, event),
                WatchdogFrame::Request { .. } => {
                    crate::diag::warn("watchdog", "ignored request frame from watchdog");
                    crate::services::watchdog_log::warning(
                        &app,
                        &state,
                        Some("unexpected_request"),
                        "watchdog sent an unexpected request frame",
                    );
                }
            }
        }

        if state.process_watchdog.clear_if_generation(generation) {
            crate::providers::registry::watchdog_eof(&app, &state);
        }
    });
}

pub(super) fn spawn_stderr_reader<R: Runtime>(
    app: AppHandle<R>,
    state: AppState,
    reader: impl Read + Send + 'static,
) {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines() {
            let Ok(line) = line else { break };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if watchdog_stderr_is_warning(line) {
                crate::services::watchdog_log::warning(&app, &state, Some("watchdog_stderr"), line);
            } else {
                crate::services::watchdog_log::info(&app, &state, line);
            }
        }
    });
}

fn dispatch_event<R: Runtime>(app: &AppHandle<R>, state: &AppState, event: WatchdogEvent) {
    crate::providers::registry::watchdog_event(app, state, event);
}

fn watchdog_stderr_is_warning(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("failed")
        || lower.contains("error")
        || lower.contains("timed out")
        || lower.contains("panic")
        || lower.contains("force kill")
}

fn safe_frame_excerpt(line: &str) -> String {
    const MAX_CHARS: usize = 500;
    let sanitized = crate::services::provider_log::sanitize_line(line.to_string());
    let mut excerpt = sanitized.chars().take(MAX_CHARS).collect::<String>();
    if sanitized.chars().count() > MAX_CHARS {
        excerpt.push_str("...");
    }
    excerpt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_watchdog_frame_excerpt_redacts_sensitive_values() {
        let line = r#"{"args":["--token","secret"],"Authorization":"Bearer abc"}"#;

        let excerpt = safe_frame_excerpt(line);

        assert!(excerpt.contains("--token"));
        assert!(!excerpt.contains("secret"));
        assert!(!excerpt.contains("Bearer abc"));
    }

    #[test]
    fn failed_watchdog_frame_excerpt_is_bounded() {
        let excerpt = safe_frame_excerpt(&"x".repeat(600));

        assert_eq!(excerpt.chars().count(), 503);
        assert!(excerpt.ends_with("..."));
    }

    #[test]
    fn watchdog_stderr_problem_lines_are_warnings() {
        assert!(watchdog_stderr_is_warning(
            "[tunnelx-watchdog] cleanup failed: denied"
        ));
        assert!(watchdog_stderr_is_warning(
            "[tunnelx-watchdog] heartbeat timed out; cleaning up"
        ));
        assert!(watchdog_stderr_is_warning(
            "[tunnelx-watchdog] falling back to force kill"
        ));
        assert!(!watchdog_stderr_is_warning(
            "[tunnelx-watchdog] gracefully stopped cloudflare:abc"
        ));
    }
}
