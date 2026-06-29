use crate::args::Args;
use crate::emitter::Emitter;
use crate::platform::{parent_alive, ProcessGroup};
use crate::process::{
    reap_exited, shutdown_all, spawn_log_reader, spawn_managed_process, terminate_and_cleanup,
    ManagedProcess, ProcessKey,
};
use crate::relay::RelayManager;
use crate::session::{remove_session, write_session, WatchdogSession};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tunnelx_watchdog_protocol::{
    WatchdogAttachRequest, WatchdogEvent, WatchdogFrame, WatchdogRequest, WatchdogResponse,
    WatchdogStartProcessRequest, WatchdogStopProcessRequest,
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const RELAY_STATS_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) enum SupervisorMessage {
    Request {
        id: String,
        request: WatchdogRequest,
    },
    RejectedRequest {
        id: String,
        message: String,
    },
    ParentDisconnected,
    Attached {
        parent_pid: u32,
    },
    ProcessLog {
        key: ProcessKey,
        line: String,
    },
}

pub(crate) fn run(args: Args) -> ! {
    let emitter = Emitter::new();
    let process_group = ProcessGroup::new();
    let (tx, rx) = mpsc::channel::<SupervisorMessage>();
    spawn_stdin_reader(tx.clone());
    let session_path = args.session_path.clone();
    let _control_listener = start_control_listener(&args, emitter.clone(), tx.clone());

    eprintln!(
        "[tunnelx-watchdog] started; supervising parent pid={}",
        args.parent_pid
    );

    let mut children: HashMap<ProcessKey, ManagedProcess> = HashMap::new();
    let mut relays = RelayManager::default();
    let mut started_any = false;
    let mut parent_pid = args.parent_pid;
    let heartbeat_timeout = Duration::from_millis(args.heartbeat_timeout_ms);
    let mut last_heartbeat = Instant::now();
    let mut owner_lost = false;
    let mut last_relay_stats = Instant::now();

    loop {
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(SupervisorMessage::Request { id, request }) => match request {
                WatchdogRequest::Attach(_) => {
                    emitter.send_response(
                        id,
                        WatchdogResponse::Rejected {
                            message: "attach is only allowed on the recovery control channel"
                                .into(),
                        },
                    );
                }
                WatchdogRequest::PrepareRelay(request) => {
                    last_heartbeat = Instant::now();
                    owner_lost = false;
                    let provider_id = request.provider_id;
                    let tunnel_id = request.tunnel_id;
                    let key = ProcessKey::new(provider_id.clone(), tunnel_id.clone());
                    match relays.prepare(key, request.endpoints, &emitter) {
                        Ok(endpoints) => emitter.send_response(
                            id,
                            WatchdogResponse::RelayPrepared {
                                request: "prepare_relay".into(),
                                provider_id,
                                tunnel_id,
                                endpoints,
                            },
                        ),
                        Err(error) => emitter.send_response(
                            id,
                            WatchdogResponse::Failed {
                                message: format!("relay prepare failed: {error}"),
                            },
                        ),
                    }
                }
                WatchdogRequest::ReleaseRelay(request) => {
                    last_heartbeat = Instant::now();
                    owner_lost = false;
                    relays.release(&ProcessKey::new(request.provider_id, request.tunnel_id));
                    emitter.send_response(
                        id,
                        WatchdogResponse::Accepted {
                            request: "release_relay".into(),
                        },
                    );
                }
                WatchdogRequest::StartProcess(request) => {
                    last_heartbeat = Instant::now();
                    owner_lost = false;
                    match start_process(
                        request,
                        &mut children,
                        &process_group,
                        &emitter,
                        &tx,
                        &mut started_any,
                        &mut relays,
                    ) {
                        Ok(()) => emitter.send_response(
                            id,
                            WatchdogResponse::Accepted {
                                request: "start_process".into(),
                            },
                        ),
                        Err(error) => {
                            emitter.send_response(id, WatchdogResponse::Failed { message: error });
                        }
                    }
                }
                WatchdogRequest::StopProcess(request) => {
                    last_heartbeat = Instant::now();
                    owner_lost = false;
                    emitter.send_response(
                        id,
                        WatchdogResponse::Accepted {
                            request: "stop_process".into(),
                        },
                    );
                    stop_process(request, &mut children, &mut relays, &emitter);
                }
                WatchdogRequest::Shutdown => {
                    emitter.send_response(
                        id,
                        WatchdogResponse::Accepted {
                            request: "shutdown".into(),
                        },
                    );
                    exit_after_cleanup(
                        &mut children,
                        &mut relays,
                        &emitter,
                        session_path.as_deref(),
                        0,
                    );
                }
                WatchdogRequest::Snapshot => {
                    last_heartbeat = Instant::now();
                    owner_lost = false;
                    emitter.send_response(
                        id,
                        WatchdogResponse::Accepted {
                            request: "snapshot".into(),
                        },
                    );
                    emit_running_snapshot(&children, &emitter);
                }
                WatchdogRequest::Heartbeat(request) => {
                    parent_pid = request.parent_pid;
                    last_heartbeat = Instant::now();
                    owner_lost = false;
                    emitter.send_response(
                        id,
                        WatchdogResponse::Accepted {
                            request: "heartbeat".into(),
                        },
                    );
                }
            },
            Ok(SupervisorMessage::RejectedRequest { id, message }) => {
                emitter.send_response(id, WatchdogResponse::Rejected { message });
            }
            Ok(SupervisorMessage::ParentDisconnected) => {
                owner_lost = mark_owner_lost(
                    owner_lost,
                    args.heartbeat_timeout_ms,
                    &emitter,
                    "owner connection disconnected",
                );
            }
            Ok(SupervisorMessage::Attached {
                parent_pid: attached_pid,
            }) => {
                parent_pid = attached_pid;
                last_heartbeat = Instant::now();
                owner_lost = false;
                eprintln!(
                    "[tunnelx-watchdog] owner reattached; parent pid={}",
                    parent_pid
                );
                emitter.send_event(WatchdogEvent::OwnerAttached {
                    pid: std::process::id(),
                    active: children.len(),
                });
                emit_running_snapshot(&children, &emitter);
            }
            Ok(SupervisorMessage::ProcessLog { key, line }) => {
                if let Some(process) = children.get_mut(&key) {
                    process.push_recent_log(line);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                exit_after_cleanup(
                    &mut children,
                    &mut relays,
                    &emitter,
                    session_path.as_deref(),
                    0,
                );
            }
        }

        let active_before_reap = children.keys().cloned().collect::<HashSet<_>>();
        reap_exited(&mut children, &emitter);
        for key in active_before_reap {
            if !children.contains_key(&key) {
                relays.release(&key);
            }
        }

        if last_relay_stats.elapsed() >= RELAY_STATS_INTERVAL {
            relays.emit_stats(&emitter);
            last_relay_stats = Instant::now();
        }

        if !parent_alive(parent_pid) {
            owner_lost = mark_owner_lost(
                owner_lost,
                args.heartbeat_timeout_ms,
                &emitter,
                "parent process exited",
            );
        }

        if owner_cleanup_due(owner_lost, last_heartbeat, heartbeat_timeout) {
            mark_owner_lost(
                owner_lost,
                args.heartbeat_timeout_ms,
                &emitter,
                "owner heartbeat timed out",
            );
            eprintln!("[tunnelx-watchdog] heartbeat timed out; cleaning up managed processes");
            emitter.send_event(WatchdogEvent::OwnerHeartbeatExpired);
            exit_after_cleanup(
                &mut children,
                &mut relays,
                &emitter,
                session_path.as_deref(),
                0,
            );
        }

        if started_any && children.is_empty() {
            eprintln!("[tunnelx-watchdog] no managed processes remain; exiting");
            relays.clear();
            remove_session(session_path.as_deref());
            std::process::exit(0);
        }
    }
}

fn owner_cleanup_due(
    owner_lost: bool,
    last_heartbeat: Instant,
    heartbeat_timeout: Duration,
) -> bool {
    owner_lost && last_heartbeat.elapsed() >= heartbeat_timeout
}

fn mark_owner_lost(current: bool, timeout_ms: u64, emitter: &Emitter, reason: &str) -> bool {
    if !current {
        eprintln!("[tunnelx-watchdog] {reason}; waiting for heartbeat ({timeout_ms}ms)");
        emitter.send_event(WatchdogEvent::OwnerLost { timeout_ms });
    }
    true
}

fn start_process(
    request: WatchdogStartProcessRequest,
    children: &mut HashMap<ProcessKey, ManagedProcess>,
    process_group: &ProcessGroup,
    emitter: &Emitter,
    tx: &Sender<SupervisorMessage>,
    started_any: &mut bool,
    relays: &mut RelayManager,
) -> Result<(), String> {
    let WatchdogStartProcessRequest {
        provider_id,
        tunnel_id,
        program,
        args,
        env,
        stop_strategy,
        cleanup,
    } = request;
    let key = ProcessKey::new(provider_id, tunnel_id);
    match children.entry(key) {
        Entry::Occupied(_) => Ok(()),
        Entry::Vacant(entry) => {
            let key = entry.key().clone();
            match spawn_managed_process(&program, &args, &env) {
                Ok(mut child) => {
                    process_group.assign(&child);
                    let pid = child.id();
                    let stdout = child.stdout.take();
                    let stderr = child.stderr.take();
                    emitter.send_event(WatchdogEvent::ProcessStarted {
                        provider_id: key.provider_id.clone(),
                        tunnel_id: key.tunnel_id.clone(),
                        pid,
                    });
                    entry.insert(ManagedProcess::new(
                        key.clone(),
                        args,
                        child,
                        stop_strategy,
                        cleanup,
                    ));
                    if let Some(out) = stdout {
                        spawn_log_reader(emitter.clone(), key.clone(), out, tx.clone());
                    }
                    if let Some(err) = stderr {
                        spawn_log_reader(emitter.clone(), key.clone(), err, tx.clone());
                    }
                    *started_any = true;
                    Ok(())
                }
                Err(error) => {
                    relays.release(&key);
                    Err(format!("managed process failed to start: {error}"))
                }
            }
        }
    }
}

fn stop_process(
    request: WatchdogStopProcessRequest,
    children: &mut HashMap<ProcessKey, ManagedProcess>,
    relays: &mut RelayManager,
    emitter: &Emitter,
) {
    let key = ProcessKey::new(request.provider_id, request.tunnel_id);
    relays.release(&key);
    if let Some(mut process) = children.remove(&key) {
        let outcome = terminate_and_cleanup(&mut process);
        emitter.send_event(WatchdogEvent::ProcessExit {
            provider_id: process.key.provider_id,
            tunnel_id: process.key.tunnel_id,
            success: true,
            code: Some(0),
            forced: outcome.forced,
            cleanup_success: Some(outcome.cleanup.is_ok()),
            cleanup_error: outcome.cleanup.err(),
        });
    } else {
        emitter.send_event(already_stopped_event(key));
    }
}

fn already_stopped_event(key: ProcessKey) -> WatchdogEvent {
    WatchdogEvent::ProcessExit {
        provider_id: key.provider_id,
        tunnel_id: key.tunnel_id,
        success: true,
        forced: false,
        code: None,
        cleanup_success: None,
        cleanup_error: None,
    }
}

fn spawn_stdin_reader(tx: Sender<SupervisorMessage>) {
    thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut handle = stdin.lock();
        let mut line = String::new();
        loop {
            line.clear();
            match handle.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<WatchdogFrame>(trimmed) {
                        Ok(WatchdogFrame::Request {
                            version,
                            id,
                            request,
                            ..
                        }) if version == tunnelx_watchdog_protocol::WATCHDOG_PROTOCOL_VERSION => {
                            if tx.send(SupervisorMessage::Request { id, request }).is_err() {
                                return;
                            }
                        }
                        Ok(WatchdogFrame::Request { version, id, .. }) => {
                            if tx
                                .send(SupervisorMessage::RejectedRequest {
                                    id,
                                    message: format!(
                                        "unsupported watchdog protocol version: {version}"
                                    ),
                                })
                                .is_err()
                            {
                                return;
                            }
                        }
                        Ok(_) => eprintln!("[tunnelx-watchdog] ignored non-request frame"),
                        Err(error) => {
                            eprintln!("[tunnelx-watchdog] failed to parse request ({error})")
                        }
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(SupervisorMessage::ParentDisconnected);
    });
}

fn start_control_listener(
    args: &Args,
    emitter: Emitter,
    tx: Sender<SupervisorMessage>,
) -> Option<TcpListener> {
    let (Some(session_path), Some(token)) =
        (args.session_path.as_deref(), args.session_token.clone())
    else {
        return None;
    };
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[tunnelx-watchdog] failed to bind recovery control port: {error}");
            return None;
        }
    };
    let port = match listener.local_addr() {
        Ok(addr) => addr.port(),
        Err(error) => {
            eprintln!("[tunnelx-watchdog] failed to read recovery control port: {error}");
            return None;
        }
    };
    if let Err(error) = write_session(
        Path::new(session_path),
        &WatchdogSession {
            pid: std::process::id(),
            port,
            token: token.clone(),
        },
    ) {
        eprintln!("[tunnelx-watchdog] failed to write recovery session: {error}");
        return None;
    }
    let worker = match listener.try_clone() {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[tunnelx-watchdog] failed to clone recovery control listener: {error}");
            return None;
        }
    };
    thread::spawn(move || accept_control_connections(worker, token, emitter, tx));
    Some(listener)
}

fn accept_control_connections(
    listener: TcpListener,
    token: String,
    emitter: Emitter,
    tx: Sender<SupervisorMessage>,
) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let token = token.clone();
        let emitter = emitter.clone();
        let tx = tx.clone();
        thread::spawn(move || handle_control_stream(stream, &token, emitter, tx));
    }
}

fn handle_control_stream(
    stream: TcpStream,
    token: &str,
    emitter: Emitter,
    tx: Sender<SupervisorMessage>,
) {
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    let Some((request_id, parent_pid)) = read_attach(&mut reader, token) else {
        return;
    };
    emitter.attach_stream(writer);
    emitter.send_response(
        request_id,
        WatchdogResponse::Accepted {
            request: "attach".into(),
        },
    );
    if tx.send(SupervisorMessage::Attached { parent_pid }).is_err() {
        return;
    }
    read_control_requests(reader, tx);
}

fn read_attach(reader: &mut BufReader<TcpStream>, token: &str) -> Option<(String, u32)> {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return None,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<WatchdogFrame>(trimmed) {
                    Ok(WatchdogFrame::Request {
                        id,
                        version,
                        request:
                            WatchdogRequest::Attach(WatchdogAttachRequest {
                                token: candidate,
                                parent_pid,
                            }),
                        ..
                    }) if version == tunnelx_watchdog_protocol::WATCHDOG_PROTOCOL_VERSION
                        && candidate == token =>
                    {
                        return Some((id, parent_pid))
                    }
                    _ => return None,
                }
            }
        }
    }
}

fn read_control_requests(mut reader: BufReader<TcpStream>, tx: Sender<SupervisorMessage>) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => {
                let _ = tx.send(SupervisorMessage::ParentDisconnected);
                return;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<WatchdogFrame>(trimmed) {
                    Ok(WatchdogFrame::Request {
                        version,
                        id,
                        request,
                        ..
                    }) if version == tunnelx_watchdog_protocol::WATCHDOG_PROTOCOL_VERSION => {
                        if tx.send(SupervisorMessage::Request { id, request }).is_err() {
                            return;
                        }
                    }
                    Ok(WatchdogFrame::Request { version, id, .. }) => {
                        if tx
                            .send(SupervisorMessage::RejectedRequest {
                                id,
                                message: format!(
                                    "unsupported watchdog protocol version: {version}"
                                ),
                            })
                            .is_err()
                        {
                            return;
                        }
                    }
                    Ok(_) => eprintln!("[tunnelx-watchdog] ignored non-request control frame"),
                    Err(error) => {
                        eprintln!("[tunnelx-watchdog] failed to parse control request ({error})")
                    }
                }
            }
        }
    }
}

fn emit_running_snapshot(children: &HashMap<ProcessKey, ManagedProcess>, emitter: &Emitter) {
    for process in children.values() {
        emitter.send_event(WatchdogEvent::ProcessSnapshot {
            provider_id: process.key.provider_id.clone(),
            tunnel_id: process.key.tunnel_id.clone(),
            pid: process.pid(),
            args: process.args.clone(),
            recent_logs: process.recent_logs(),
        });
    }
}

fn exit_after_cleanup(
    children: &mut HashMap<ProcessKey, ManagedProcess>,
    relays: &mut RelayManager,
    emitter: &Emitter,
    session_path: Option<&str>,
    code: i32,
) -> ! {
    shutdown_all(children, emitter);
    relays.clear();
    remove_session(session_path);
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_cleanup_depends_on_lost_owner_and_heartbeat_timeout_only() {
        let timeout = Duration::from_secs(180);
        let stale = Instant::now() - Duration::from_secs(181);
        let fresh = Instant::now() - Duration::from_secs(1);

        assert!(owner_cleanup_due(true, stale, timeout));
        assert!(!owner_cleanup_due(true, fresh, timeout));
        assert!(!owner_cleanup_due(false, stale, timeout));
    }

    #[test]
    fn start_process_reports_spawn_failure() {
        let mut children = HashMap::new();
        let mut relays = RelayManager::default();
        let mut started_any = false;
        let (tx, _rx) = mpsc::channel();
        let missing_program = std::env::temp_dir()
            .join("tunnelx-watchdog-missing-program")
            .join("missing");

        let result = start_process(
            WatchdogStartProcessRequest {
                provider_id: "provider-a".into(),
                tunnel_id: "conn-1".into(),
                program: missing_program.to_string_lossy().to_string(),
                args: Vec::new(),
                env: Vec::new(),
                stop_strategy: None,
                cleanup: None,
            },
            &mut children,
            &ProcessGroup::new(),
            &Emitter::new(),
            &tx,
            &mut started_any,
            &mut relays,
        );

        assert!(result.is_err());
        assert!(!started_any);
        assert!(children.is_empty());
    }

    #[test]
    fn missing_stop_target_reports_terminal_event_without_cleanup_claim() {
        let event = already_stopped_event(ProcessKey::new("provider-a".into(), "conn-1".into()));

        assert_eq!(
            event,
            WatchdogEvent::ProcessExit {
                provider_id: "provider-a".into(),
                tunnel_id: "conn-1".into(),
                success: true,
                forced: false,
                code: None,
                cleanup_success: None,
                cleanup_error: None,
            }
        );
    }
}
