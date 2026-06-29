use serde_json::Value;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tunnelx_watchdog_protocol::{
    redaction::redact_sensitive_text, WatchdogCleanupAction, WatchdogCleanupPlan, WatchdogEnvVar,
    WatchdogHttpRequest, WatchdogJsonExpectation,
};

const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const CLEANUP_ATTEMPTS: usize = 3;
const CLEANUP_RETRY_DELAY: Duration = Duration::from_millis(800);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) fn cleanup_plan(cleanup: Option<&WatchdogCleanupPlan>) -> Result<(), String> {
    let Some(cleanup) = cleanup else {
        return Ok(());
    };
    let attempts = cleanup.retry_attempts.unwrap_or(CLEANUP_ATTEMPTS).max(1);
    let retry_delay = cleanup
        .retry_delay_ms
        .map(Duration::from_millis)
        .unwrap_or(CLEANUP_RETRY_DELAY);
    let mut last_error = String::new();
    for attempt in 0..attempts {
        if attempt > 0 {
            thread::sleep(retry_delay);
        }
        match run_cleanup_actions(&cleanup.actions) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = sanitize_error(error),
        }
    }
    if !last_error.is_empty() {
        eprintln!("[tunnelx-watchdog] cleanup failed: {last_error}");
        Err(last_error)
    } else {
        Ok(())
    }
}

fn run_cleanup_actions(actions: &[WatchdogCleanupAction]) -> Result<(), String> {
    let mut errors = Vec::new();
    let mut files_to_remove = Vec::new();
    for (index, action) in actions.iter().enumerate() {
        match action {
            WatchdogCleanupAction::Http { request } => {
                if let Err(error) = execute_http_request(request) {
                    errors.push(cleanup_action_error(index, action, error));
                }
            }
            WatchdogCleanupAction::Command {
                program,
                args,
                env,
                timeout_ms,
                accepted_codes,
            } => {
                if let Err(error) = execute_command(program, args, env, *timeout_ms, accepted_codes)
                {
                    errors.push(cleanup_action_error(index, action, error));
                }
            }
            WatchdogCleanupAction::HttpJsonDeleteMatches {
                list_request,
                items_field,
                id_field,
                match_field,
                match_value,
                match_ignore_ascii_case,
                match_trim_trailing_dot,
                delete_request,
                id_placeholder,
            } => {
                if let Err(error) = execute_http_json_delete_matches(
                    list_request,
                    items_field,
                    id_field,
                    match_field,
                    match_value,
                    *match_ignore_ascii_case,
                    *match_trim_trailing_dot,
                    delete_request,
                    id_placeholder,
                ) {
                    errors.push(cleanup_action_error(index, action, error));
                }
            }
            WatchdogCleanupAction::RemoveFile { path } => {
                let path = path.trim();
                if !path.is_empty() {
                    files_to_remove.push((index, path.to_string()));
                }
            }
        }
    }
    if errors.is_empty() {
        for (index, path) in files_to_remove {
            if let Err(error) = remove_cleanup_file(&path) {
                errors.push(cleanup_action_error(
                    index,
                    &WatchdogCleanupAction::RemoveFile { path },
                    error,
                ));
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn remove_cleanup_file(path: &str) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(sanitize_error(format!(
            "cleanup remove_file `{path}` failed: {error}"
        ))),
    }
}

fn cleanup_action_error(index: usize, action: &WatchdogCleanupAction, error: String) -> String {
    sanitize_error(format!(
        "cleanup action #{} ({}) failed: {}",
        index + 1,
        cleanup_action_name(action),
        error
    ))
}

fn cleanup_action_name(action: &WatchdogCleanupAction) -> &'static str {
    match action {
        WatchdogCleanupAction::Http { .. } => "http",
        WatchdogCleanupAction::Command { .. } => "command",
        WatchdogCleanupAction::HttpJsonDeleteMatches { .. } => "http_json_delete_matches",
        WatchdogCleanupAction::RemoveFile { .. } => "remove_file",
    }
}

fn execute_command(
    program: &str,
    args: &[String],
    env: &[WatchdogEnvVar],
    timeout_ms: Option<u64>,
    accepted_codes: &[i32],
) -> Result<(), String> {
    let program = program.trim();
    if program.is_empty() {
        return Err("cleanup command program is empty".into());
    }

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for item in env {
        if !item.name.trim().is_empty() {
            command.env(item.name.trim(), item.value.as_str());
        }
    }
    crate::platform::configure_command(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cleanup command failed to start `{program}`: {error}"))?;

    let timeout = timeout_ms
        .map(Duration::from_millis)
        .unwrap_or(COMMAND_TIMEOUT);
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status
                    .code()
                    .unwrap_or(if status.success() { 0 } else { -1 });
                let accepted =
                    accepted_codes.is_empty() && status.success() || accepted_codes.contains(&code);
                if accepted {
                    return Ok(());
                }
                let output = child.wait_with_output().ok();
                let stderr = output
                    .as_ref()
                    .map(|item| String::from_utf8_lossy(&item.stderr).trim().to_string())
                    .unwrap_or_default();
                return Err(format!(
                    "cleanup command `{program}` exited with code {code}: {stderr}"
                ));
            }
            Ok(None) => {}
            Err(error) => return Err(format!("cleanup command wait failed `{program}`: {error}")),
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("cleanup command timed out `{program}`"));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub(crate) fn execute_http_request(request: &WatchdogHttpRequest) -> Result<(), String> {
    execute_http_request_capture(request).map(|_| ())
}

fn execute_http_request_capture(request: &WatchdogHttpRequest) -> Result<String, String> {
    let method = reqwest::Method::from_bytes(request.method.trim().as_bytes())
        .map_err(|error| format!("invalid HTTP method: {error}"))?;
    let client = reqwest::blocking::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let mut builder = client.request(method, request.url.trim());
    for header in &request.headers {
        if !header.name.trim().is_empty() {
            builder = builder.header(header.name.trim(), header.value.as_str());
        }
    }
    if let Some(timeout_ms) = request.timeout_ms {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }
    if let Some(body) = request.body.as_ref() {
        builder = builder.body(body.clone());
    }
    let response = builder.send().map_err(|error| error.to_string())?;
    let status = response.status().as_u16();
    let body = response.text().unwrap_or_default();
    if !status_accepted(status, &request.accepted_statuses) {
        return Err(sanitize_error(format!(
            "HTTP {} {} returned status {status}: {}",
            request.method, request.url, body
        )));
    }
    if let Some(expectation) = request.json_expectation.as_ref() {
        if body.trim().is_empty() {
            return Ok(body);
        }
        let json: Value = serde_json::from_str(&body).map_err(|error| {
            format!(
                "HTTP {} {} returned invalid JSON: {error}",
                request.method, request.url
            )
        })?;
        if json_expectation_met(&json, expectation) {
            return Ok(body);
        }
        return Err(sanitize_error(format!(
            "HTTP {} {} did not match JSON expectation: {}",
            request.method, request.url, body
        )));
    }
    Ok(body)
}

#[allow(clippy::too_many_arguments)]
fn execute_http_json_delete_matches(
    list_request: &WatchdogHttpRequest,
    items_field: &str,
    id_field: &str,
    match_field: &str,
    match_value: &str,
    match_ignore_ascii_case: bool,
    match_trim_trailing_dot: bool,
    delete_request: &WatchdogHttpRequest,
    id_placeholder: &str,
) -> Result<(), String> {
    let body = execute_http_request_capture(list_request)?;
    let ids = matching_json_item_ids(
        &body,
        items_field,
        id_field,
        match_field,
        match_value,
        match_ignore_ascii_case,
        match_trim_trailing_dot,
    )?;
    for id in ids {
        let request = request_with_placeholder(delete_request, id_placeholder, &id)?;
        execute_http_request(&request)?;
    }
    Ok(())
}

fn matching_json_item_ids(
    body: &str,
    items_field: &str,
    id_field: &str,
    match_field: &str,
    match_value: &str,
    match_ignore_ascii_case: bool,
    match_trim_trailing_dot: bool,
) -> Result<Vec<String>, String> {
    let json: Value = serde_json::from_str(body)
        .map_err(|error| format!("failed to parse JSON list response: {error}"))?;
    let items = json_array_field(&json, items_field).ok_or_else(|| {
        format!(
            "JSON list response is missing array field `{}`",
            items_field.trim()
        )
    })?;
    let mut ids = Vec::new();
    for item in items {
        let Some(candidate) = json_path(item, match_field).and_then(Value::as_str) else {
            continue;
        };
        if !cleanup_match_value(
            candidate,
            match_value,
            match_ignore_ascii_case,
            match_trim_trailing_dot,
        ) {
            continue;
        }
        let id = json_path(item, id_field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("matched item is missing ID field `{}`", id_field.trim()))?
            .trim()
            .to_string();
        if !id.is_empty() {
            ids.push(id);
        }
    }
    Ok(ids)
}

fn cleanup_match_value(
    left: &str,
    right: &str,
    ignore_ascii_case: bool,
    trim_trailing_dot: bool,
) -> bool {
    let mut left = left.trim().to_string();
    let mut right = right.trim().to_string();
    if trim_trailing_dot {
        left = left.trim_end_matches('.').to_string();
        right = right.trim_end_matches('.').to_string();
    }
    if ignore_ascii_case {
        left.eq_ignore_ascii_case(&right)
    } else {
        left == right
    }
}

fn request_with_placeholder(
    template: &WatchdogHttpRequest,
    placeholder: &str,
    id: &str,
) -> Result<WatchdogHttpRequest, String> {
    let placeholder = placeholder.trim();
    if placeholder.is_empty() {
        return Err("HTTP delete template is missing the ID placeholder".into());
    }

    let mut changed = false;
    let mut request = template.clone();
    request.url = replace_placeholder(&request.url, placeholder, id, &mut changed);
    request.body = request
        .body
        .map(|body| replace_placeholder(&body, placeholder, id, &mut changed));
    for header in &mut request.headers {
        header.value = replace_placeholder(&header.value, placeholder, id, &mut changed);
    }

    if changed {
        Ok(request)
    } else {
        Err(format!(
            "HTTP delete template did not use placeholder `{placeholder}`"
        ))
    }
}

fn replace_placeholder(value: &str, placeholder: &str, id: &str, changed: &mut bool) -> String {
    if value.contains(placeholder) {
        *changed = true;
        value.replace(placeholder, id)
    } else {
        value.to_string()
    }
}

fn status_accepted(status: u16, accepted_statuses: &[u16]) -> bool {
    if accepted_statuses.is_empty() {
        status_is_success(status)
    } else {
        accepted_statuses.contains(&status)
    }
}

fn status_is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

fn json_expectation_met(json: &Value, expectation: &WatchdogJsonExpectation) -> bool {
    if json_bool_field(json, &expectation.success_field).unwrap_or(false) {
        return true;
    }
    let Some(errors) = json_array_field(json, &expectation.errors_field) else {
        return false;
    };
    !errors.is_empty()
        && errors.iter().all(|error| {
            let code_ok = json_u32_field(error, &expectation.error_code_field)
                .map(|code| expectation.ignored_error_codes.contains(&code))
                .unwrap_or(false);
            let message_ok = json_string_field(error, &expectation.error_message_field)
                .map(|message| {
                    let message = message.to_ascii_lowercase();
                    expectation
                        .ignored_error_message_contains
                        .iter()
                        .any(|needle| message.contains(&needle.to_ascii_lowercase()))
                })
                .unwrap_or(false);
            code_ok || message_ok
        })
}

fn json_bool_field(json: &Value, path: &str) -> Option<bool> {
    json_path(json, path).and_then(Value::as_bool)
}

fn json_u32_field(json: &Value, path: &str) -> Option<u32> {
    json_path(json, path)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn json_string_field<'a>(json: &'a Value, path: &str) -> Option<&'a str> {
    json_path(json, path).and_then(Value::as_str)
}

fn json_array_field<'a>(json: &'a Value, path: &str) -> Option<&'a [Value]> {
    json_path(json, path)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
}

fn json_path<'a>(json: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = json;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = current.get(segment)?;
    }
    Some(current)
}

fn sanitize_error(error: String) -> String {
    redact_sensitive_text(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_expectation_accepts_ignored_error_messages() {
        let json = serde_json::json!({
            "success": false,
            "errors": [{ "code": 1003, "message": "resource does not exist" }]
        });
        let expectation = WatchdogJsonExpectation {
            success_field: "success".into(),
            errors_field: "errors".into(),
            error_code_field: "code".into(),
            error_message_field: "message".into(),
            ignored_error_codes: vec![1003],
            ignored_error_message_contains: vec!["not exist".into()],
        };

        assert!(json_expectation_met(&json, &expectation));
    }

    #[test]
    fn matching_json_item_ids_supports_dns_style_normalization() {
        let body = serde_json::json!({
            "result": [
                { "id": "keep", "content": "other.example.test" },
                { "id": "delete", "content": "ABCD.cfargotunnel.com." }
            ]
        })
        .to_string();

        let ids = matching_json_item_ids(
            &body,
            "result",
            "id",
            "content",
            "abcd.cfargotunnel.com",
            true,
            true,
        )
        .unwrap();

        assert_eq!(ids, vec!["delete".to_string()]);
    }

    #[test]
    fn request_with_placeholder_rewrites_delete_template() {
        let request = WatchdogHttpRequest {
            method: "DELETE".into(),
            url: "https://api.example.test/resources/{id}".into(),
            headers: Vec::new(),
            body: None,
            accepted_statuses: vec![200, 404],
            json_expectation: None,
            timeout_ms: None,
        };

        let rewritten = request_with_placeholder(&request, "{id}", "abc123").unwrap();

        assert_eq!(rewritten.url, "https://api.example.test/resources/abc123");
    }

    #[test]
    fn cleanup_keeps_files_when_remote_action_fails() {
        let dir = unique_temp_dir();
        let file = dir.join("credentials.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, b"secret").unwrap();

        let actions = vec![
            WatchdogCleanupAction::Http {
                request: Box::new(WatchdogHttpRequest {
                    method: "BAD METHOD".into(),
                    url: "https://api.example.test/fail".into(),
                    headers: Vec::new(),
                    body: None,
                    accepted_statuses: Vec::new(),
                    json_expectation: None,
                    timeout_ms: None,
                }),
            },
            WatchdogCleanupAction::RemoveFile {
                path: file.to_string_lossy().to_string(),
            },
        ];

        let error = run_cleanup_actions(&actions).unwrap_err();
        assert!(error.contains("cleanup action #1 (http) failed"));
        assert!(file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cleanup_removes_files_after_successful_remote_actions() {
        let dir = unique_temp_dir();
        let file = dir.join("credentials.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, b"secret").unwrap();

        let actions = vec![WatchdogCleanupAction::RemoveFile {
            path: file.to_string_lossy().to_string(),
        }];

        assert!(run_cleanup_actions(&actions).is_ok());
        assert!(!file.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cleanup_reports_file_removal_failure() {
        let dir = unique_temp_dir();
        std::fs::create_dir_all(&dir).unwrap();

        let actions = vec![WatchdogCleanupAction::RemoveFile {
            path: dir.to_string_lossy().to_string(),
        }];

        let error = run_cleanup_actions(&actions).unwrap_err();
        assert!(error.contains("cleanup action #1 (remove_file) failed"));
        assert!(error.contains("cleanup remove_file"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cleanup_accepts_already_removed_file() {
        let path = unique_temp_dir().join("missing.json");
        let actions = vec![WatchdogCleanupAction::RemoveFile {
            path: path.to_string_lossy().to_string(),
        }];

        assert!(run_cleanup_actions(&actions).is_ok());
    }

    #[test]
    fn cleanup_errors_redact_sensitive_values() {
        let error = sanitize_error(
            r#"HTTP DELETE https://api.example.test/items?token=query-token&ok=1 returned status 403: {"Authorization":"Bearer abc","token":"body-token","TunnelSecret":"secret-value"} --authtoken cli-token"#
                .into(),
        );

        assert!(error.contains("token=***"));
        assert!(error.contains(r#""Authorization":"***""#));
        assert!(error.contains(r#""token":"***""#));
        assert!(error.contains(r#""TunnelSecret":"***""#));
        assert!(error.contains("--authtoken ***"));
        assert!(!error.contains("query-token"));
        assert!(!error.contains("Bearer abc"));
        assert!(!error.contains("body-token"));
        assert!(!error.contains("secret-value"));
        assert!(!error.contains("cli-token"));
    }

    fn unique_temp_dir() -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "tunnelx-watchdog-test-{}-{suffix}-{serial}",
            std::process::id()
        ))
    }
}
