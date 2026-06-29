use super::binary::watchdog_exe;
use super::events::{spawn_event_reader, spawn_stderr_reader};
use super::state::{ProcessWatchdogState, WatchdogHandle, WatchdogWriter};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::Deserialize;
use std::io::ErrorKind;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Runtime};
use tunnelx_watchdog_protocol::{
    WatchdogAttachRequest, WatchdogFrame, WatchdogHeartbeatRequest, WatchdogRequest,
    WatchdogResponse,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const STOP_GRACE: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const OWNER_RECOVERY_TIMEOUT: Duration = Duration::from_secs(180);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const SESSION_FILE: &str = "watchdog-session.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchdogSessionFile {
    pid: u32,
    port: u16,
    token: String,
}

pub fn ensure<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> AppResult<()> {
    let (stdout, stderr, pid, generation) = {
        let mut inner = state.process_watchdog.lock();
        if ProcessWatchdogState::alive_locked(&mut inner) {
            return Ok(());
        }
        drop(inner);
        if recover_if_available(app, state)? {
            return Ok(());
        }

        let mut inner = state.process_watchdog.lock();
        if ProcessWatchdogState::alive_locked(&mut inner) {
            return Ok(());
        }

        let watchdog = watchdog_exe(app)?;
        let session_token = uuid::Uuid::new_v4().to_string();
        let session_path = session_file(app)?;
        let mut command = Command::new(&watchdog);
        command
            .arg("--parent-pid")
            .arg(std::process::id().to_string())
            .arg("--session-path")
            .arg(session_path.to_string_lossy().to_string())
            .arg("--session-token")
            .arg(session_token)
            .arg("--heartbeat-timeout-ms")
            .arg(OWNER_RECOVERY_TIMEOUT.as_millis().to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);

        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Msg("watchdog stdin is unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Msg("watchdog stdout is unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Msg("watchdog stderr is unavailable".into()))?;
        let pid = child.id();
        inner.generation = inner.generation.saturating_add(1);
        let generation = inner.generation;
        inner.handle = Some(WatchdogHandle {
            pid,
            child: Some(child),
            writer: WatchdogWriter::Stdin(stdin),
        });
        (stdout, stderr, pid, generation)
    };

    spawn_event_reader(app.clone(), state.clone(), stdout, generation);
    spawn_stderr_reader(app.clone(), state.clone(), stderr);
    spawn_heartbeat(state.clone(), generation);
    crate::services::watchdog_log::info(
        app,
        state,
        format!(
            "watchdog started; pid={pid}; heartbeat={}s timeout={}s",
            HEARTBEAT_INTERVAL.as_secs(),
            OWNER_RECOVERY_TIMEOUT.as_secs()
        ),
    );
    Ok(())
}

pub fn send<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    watchdog_request: WatchdogRequest,
) -> AppResult<()> {
    request(app, state, watchdog_request).map(|_| ())
}

pub fn request<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    request: WatchdogRequest,
) -> AppResult<WatchdogResponse> {
    let request_name = watchdog_request_name(&request);
    for _ in 0..2 {
        ensure(app, state)?;
        match send_request_once(state, request.clone()) {
            Ok(response) => return Ok(response),
            Err(AppError::WatchdogCommunicationFailed) => {
                mark_communication_failed(state);
                continue;
            }
            Err(error) => return Err(error),
        };
    }
    crate::services::watchdog_log::warning(
        app,
        state,
        Some("watchdog_request_failed"),
        format!("watchdog request {request_name} failed after retry; communication unavailable"),
    );
    Err(AppError::WatchdogCommunicationFailed)
}

fn watchdog_request_name(request: &WatchdogRequest) -> &'static str {
    match request {
        WatchdogRequest::Attach(_) => "attach",
        WatchdogRequest::PrepareRelay(_) => "prepare_relay",
        WatchdogRequest::ReleaseRelay(_) => "release_relay",
        WatchdogRequest::StartProcess(_) => "start_process",
        WatchdogRequest::StopProcess(_) => "stop_process",
        WatchdogRequest::Shutdown => "shutdown",
        WatchdogRequest::Snapshot => "snapshot",
        WatchdogRequest::Heartbeat(_) => "heartbeat",
    }
}

fn send_request_once(state: &AppState, request: WatchdogRequest) -> AppResult<WatchdogResponse> {
    let (id, seq, rx) = state.process_watchdog.next_request();
    let line = serde_json::to_string(&WatchdogFrame::request(id.clone(), seq, request))?;
    let sent = {
        let mut inner = state.process_watchdog.lock();
        ProcessWatchdogState::send_locked(&mut inner, &line)
    };
    if !sent {
        state.process_watchdog.remove_pending(&id);
        return Err(AppError::WatchdogCommunicationFailed);
    }
    match rx.recv_timeout(REQUEST_TIMEOUT) {
        Ok(response) if response.is_accepted() => Ok(response),
        Ok(response) => Err(AppError::Msg(
            response
                .message()
                .unwrap_or("watchdog rejected request")
                .to_string(),
        )),
        Err(_) => {
            state.process_watchdog.remove_pending(&id);
            Err(AppError::WatchdogCommunicationFailed)
        }
    }
}

pub fn recover_if_available<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> AppResult<bool> {
    {
        let mut inner = state.process_watchdog.lock();
        if ProcessWatchdogState::alive_locked(&mut inner) {
            return Ok(true);
        }
    }
    let session_path = session_file(app)?;
    let body = match std::fs::read_to_string(&session_path) {
        Ok(body) => body,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            discard_recovery_session(
                app,
                state,
                &session_path,
                format!("watchdog recovery session could not be read: {error}"),
            );
            return Ok(false);
        }
    };
    let session: WatchdogSessionFile = match serde_json::from_str(&body) {
        Ok(session) => session,
        Err(_) => {
            discard_recovery_session(
                app,
                state,
                &session_path,
                "watchdog recovery session is invalid",
            );
            return Ok(false);
        }
    };
    let mut stream = match TcpStream::connect(("127.0.0.1", session.port)) {
        Ok(stream) => stream,
        Err(_) => {
            discard_recovery_session(
                app,
                state,
                &session_path,
                format!(
                    "watchdog recovery control port is unavailable: {}",
                    session.port
                ),
            );
            return Ok(false);
        }
    };
    let attach = WatchdogFrame::request(
        "attach-1",
        1,
        WatchdogRequest::Attach(WatchdogAttachRequest {
            token: session.token,
            parent_pid: std::process::id(),
        }),
    );
    let Ok(attach) = serde_json::to_string(&attach) else {
        discard_recovery_session(
            app,
            state,
            &session_path,
            "watchdog recovery attach request could not be encoded",
        );
        return Ok(false);
    };
    let Ok(reader_stream) = stream.try_clone() else {
        discard_recovery_session(
            app,
            state,
            &session_path,
            "watchdog recovery control stream could not be cloned",
        );
        return Ok(false);
    };
    let mut reader = BufReader::new(reader_stream);
    if reader
        .get_ref()
        .set_read_timeout(Some(Duration::from_secs(2)))
        .is_err()
        || stream.write_all(attach.as_bytes()).is_err()
        || stream.write_all(b"\n").is_err()
        || stream.flush().is_err()
    {
        discard_recovery_session(
            app,
            state,
            &session_path,
            "watchdog recovery attach request could not be sent",
        );
        return Ok(false);
    }
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        discard_recovery_session(
            app,
            state,
            &session_path,
            "watchdog recovery attach response could not be read",
        );
        return Ok(false);
    }
    let Ok(frame) = serde_json::from_str::<WatchdogFrame>(line.trim()) else {
        discard_recovery_session(
            app,
            state,
            &session_path,
            "watchdog recovery attach response was invalid",
        );
        return Ok(false);
    };
    if !frame.supported_version()
        || !matches!(
            frame,
            WatchdogFrame::Response {
                response: WatchdogResponse::Accepted { .. },
                ..
            }
        )
    {
        discard_recovery_session(
            app,
            state,
            &session_path,
            "watchdog recovery attach was rejected",
        );
        return Ok(false);
    }
    if reader.get_ref().set_read_timeout(None).is_err() {
        discard_recovery_session(
            app,
            state,
            &session_path,
            "watchdog recovery control stream could not be restored",
        );
        return Ok(false);
    }
    let generation = {
        let mut inner = state.process_watchdog.lock();
        // A concurrent ensure()/recover() may have already attached while we did the (unlocked)
        // handshake above; if so, don't clobber its handle or spawn duplicate readers.
        if ProcessWatchdogState::alive_locked(&mut inner) {
            return Ok(true);
        }
        inner.generation = inner.generation.saturating_add(1);
        let generation = inner.generation;
        inner.handle = Some(WatchdogHandle {
            pid: session.pid,
            child: None,
            writer: WatchdogWriter::Tcp(stream),
        });
        generation
    };
    spawn_event_reader(app.clone(), state.clone(), reader, generation);
    spawn_heartbeat(state.clone(), generation);
    crate::services::watchdog_log::info(
        app,
        state,
        format!(
            "watchdog session recovered; pid={}; control port={}",
            session.pid, session.port
        ),
    );
    Ok(true)
}

fn discard_recovery_session<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    session_path: &PathBuf,
    reason: impl Into<String>,
) {
    let _ = std::fs::remove_file(session_path);
    crate::services::watchdog_log::warning(
        app,
        state,
        Some("watchdog_recovery_failed"),
        format!("{}; session file removed", reason.into()),
    );
}

fn spawn_heartbeat(state: AppState, generation: u64) {
    std::thread::spawn(move || loop {
        std::thread::sleep(HEARTBEAT_INTERVAL);
        if !state.process_watchdog.is_generation(generation) {
            return;
        }
        let request = WatchdogRequest::Heartbeat(WatchdogHeartbeatRequest {
            parent_pid: std::process::id(),
        });
        match request_if_alive(&state, request) {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => return,
        }
    });
}

pub fn send_if_alive(state: &AppState, request: WatchdogRequest) -> AppResult<bool> {
    {
        let mut inner = state.process_watchdog.lock();
        if !ProcessWatchdogState::alive_locked(&mut inner) {
            return Ok(false);
        }
    }
    match send_request_once(state, request) {
        Ok(_) => Ok(true),
        Err(AppError::WatchdogCommunicationFailed) => {
            mark_communication_failed(state);
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

pub fn request_if_alive(
    state: &AppState,
    request: WatchdogRequest,
) -> AppResult<Option<WatchdogResponse>> {
    {
        let mut inner = state.process_watchdog.lock();
        if !ProcessWatchdogState::alive_locked(&mut inner) {
            return Ok(None);
        }
    }
    match send_request_once(state, request) {
        Ok(response) => Ok(Some(response)),
        Err(AppError::WatchdogCommunicationFailed) => {
            mark_communication_failed(state);
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

pub fn shutdown<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    if send_shutdown_if_alive(state) {
        crate::services::watchdog_log::info(app, state, "watchdog shutdown requested");
    }
    cleanup_handle(state);
    crate::refresh_connection_icon(app);
}

fn send_shutdown_if_alive(state: &AppState) -> bool {
    {
        let mut inner = state.process_watchdog.lock();
        if !ProcessWatchdogState::alive_locked(&mut inner) {
            return false;
        }
    }
    send_request_once(state, WatchdogRequest::Shutdown).is_ok()
}

fn mark_communication_failed(state: &AppState) {
    state.process_watchdog.clear();
}

fn cleanup_handle(state: &AppState) {
    if let Some(WatchdogHandle { mut child, .. }) = state.process_watchdog.take_handle() {
        if let Some(mut child) = child.take() {
            if !wait_child(&mut child, STOP_GRACE) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

fn session_file<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(crate::paths::data_dir(app)?.join(SESSION_FILE))
}

fn wait_child(child: &mut Child, grace: Duration) -> bool {
    let deadline = Instant::now() + grace;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => {}
            Err(_) => return false,
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tunnelx_watchdog_protocol::WatchdogStopProcessRequest;

    #[test]
    fn watchdog_request_name_is_stable_for_diagnostics() {
        assert_eq!(
            watchdog_request_name(&WatchdogRequest::Shutdown),
            "shutdown"
        );
        assert_eq!(
            watchdog_request_name(&WatchdogRequest::Snapshot),
            "snapshot"
        );
        assert_eq!(
            watchdog_request_name(&WatchdogRequest::StopProcess(WatchdogStopProcessRequest {
                provider_id: "provider-a".into(),
                tunnel_id: "conn-1".into(),
            })),
            "stop_process"
        );
    }

    #[test]
    fn owner_recovery_window_allows_desktop_restart() {
        assert_eq!(HEARTBEAT_INTERVAL.as_secs(), 5);
        assert_eq!(OWNER_RECOVERY_TIMEOUT.as_secs(), 180);
        assert!(OWNER_RECOVERY_TIMEOUT > HEARTBEAT_INTERVAL);
    }
}
