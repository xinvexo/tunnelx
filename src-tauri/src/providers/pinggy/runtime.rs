use super::data;
use super::domain::{PinggyEndpoint, PinggyTunnel};
use super::environment;
use super::paths;
use crate::error::{AppError, AppResult};
use crate::providers::cli::normalize_path;
use crate::providers::contract::{
    watchdog_exit_clean, ProviderStatus, TunnelRuntimeInfo, TunnelRuntimeState,
    TunnelRuntimeStatusEvent, PINGGY_PROVIDER_ID,
};
use crate::providers::runtime_public_url::{
    public_url_details, public_urls_from_details, RuntimePublicUrl,
};
use crate::services::{process_watchdog, provider_lifecycle, provider_log, watchdog_relay};
use crate::state::AppState;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, Signal, System};
use tauri::{AppHandle, Emitter, Runtime};
use tunnelx_watchdog_protocol::{
    WatchdogEvent, WatchdogRelayProtocol, WatchdogRequest, WatchdogStartProcessRequest,
    WatchdogStopProcessRequest,
};

const PINGGY_STOP_GRACE: Duration = Duration::from_secs(2);
const PINGGY_KILL_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
struct PinggyProcess {
    pid: u32,
    program_path: PathBuf,
}

pub fn status(app: &AppHandle, state: &AppState) -> ProviderStatus {
    environment::status(app, state)
}

pub fn cleanup_on_start(app: &AppHandle, _state: &AppState) -> AppResult<()> {
    let _ = environment::stop_daemon(app);
    cleanup_managed_pinggy_processes(app).map(|_| ())
}

pub fn start_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let mut tunnel = data::current_tunnel(state, id)?;
    let info = state
        .provider_runtime
        .begin_start(PINGGY_PROVIDER_ID, id, "Pinggy starting")?;
    emit_status(app, info.clone());
    if let Err(error) = validate_tunnel(&tunnel) {
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_system_log(app, state, id, "resolving Pinggy executable".into());
    let program = match environment::resolve_command(app) {
        Ok(program) => program,
        Err(error) => {
            mark_start_error(app, state, id, &error);
            return Err(error);
        }
    };
    emit_system_log(app, state, id, "Pinggy executable ready".into());
    let env = match environment::runtime_env(app) {
        Ok(env) => env,
        Err(error) => {
            mark_start_error(app, state, id, &error);
            return Err(error);
        }
    };
    emit_system_log(
        app,
        state,
        id,
        "Pinggy runtime state directory ready".into(),
    );
    if let Err(error) = prepare_traffic_relay(app, state, id, &mut tunnel) {
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_forwarding_target_logs(app, state, id, &tunnel);
    let with_details =
        state
            .provider_runtime
            .set_details(PINGGY_PROVIDER_ID, id, public_url_details(Vec::new()));
    emit_status(app, with_details.clone());
    emit_system_log(app, state, id, "sending Pinggy start command".into());
    let command = WatchdogRequest::StartProcess(WatchdogStartProcessRequest {
        provider_id: PINGGY_PROVIDER_ID.into(),
        tunnel_id: id.into(),
        program,
        args: match command_args(&tunnel) {
            Ok(args) => args,
            Err(error) => {
                mark_start_error(app, state, id, &error);
                return Err(error);
            }
        },
        env,
        stop_strategy: None,
        cleanup: None,
    });
    if let Err(error) = process_watchdog::send(app, state, command) {
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_system_log(app, state, id, "Pinggy start command accepted".into());
    Ok(state.provider_runtime.reconcile(PINGGY_PROVIDER_ID, id))
}

fn command_args(tunnel: &PinggyTunnel) -> AppResult<Vec<String>> {
    let endpoint = enabled_endpoint(tunnel)
        .ok_or_else(|| AppError::Msg("Pinggy tunnel has no enabled endpoint".into()))?;
    Ok(vec![
        "--noTui".into(),
        "--loglevel".into(),
        "INFO".into(),
        "--v".into(),
        "--force".into(),
        "--token".into(),
        tunnel.token.trim().to_string(),
        "--type".into(),
        endpoint.tunnel_type.trim().to_ascii_lowercase(),
        "--localport".into(),
        endpoint.local_addr.trim().to_string(),
        "-p".into(),
        tunnel.server_port.to_string(),
        "-o".into(),
        "StrictHostKeyChecking=no".into(),
        "-o".into(),
        "ServerAliveInterval=30".into(),
    ])
}

fn emit_forwarding_target_logs(app: &AppHandle, state: &AppState, id: &str, tunnel: &PinggyTunnel) {
    for endpoint in enabled_endpoints(tunnel) {
        emit_system_log(
            app,
            state,
            id,
            format!(
                "Pinggy forwarding target: {} -> {}",
                endpoint.name.trim(),
                endpoint.local_addr.trim()
            ),
        );
    }
}

fn mark_start_error(app: &AppHandle, state: &AppState, id: &str, error: &AppError) {
    watchdog_relay::release(state, PINGGY_PROVIDER_ID, id);
    provider_lifecycle::mark_errored_with_log(
        app,
        state,
        PINGGY_PROVIDER_ID,
        id,
        error.to_string(),
    );
}

pub fn stop_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let current = state.provider_runtime.reconcile(PINGGY_PROVIDER_ID, id);
    if !current.status.is_active() {
        watchdog_relay::release(state, PINGGY_PROVIDER_ID, id);
        emit_system_log(app, state, id, "Pinggy is not running".into());
        return Ok(current);
    }
    emit_system_log(
        app,
        state,
        id,
        "stopping Pinggy tunnel; public URLs will be released".into(),
    );
    let info = state.provider_runtime.mark_status(
        PINGGY_PROVIDER_ID,
        id,
        TunnelRuntimeState::Stopping,
        "Pinggy stopping",
    );
    emit_status(app, info.clone());
    let command = WatchdogRequest::StopProcess(WatchdogStopProcessRequest {
        provider_id: PINGGY_PROVIDER_ID.into(),
        tunnel_id: id.into(),
    });
    match process_watchdog::send_if_alive(state, command) {
        Ok(true) => Ok(info),
        Ok(false) => {
            cleanup_pinggy_for_runtime_pid(app, current.pid)?;
            watchdog_relay::release(state, PINGGY_PROVIDER_ID, id);
            emit_system_log(app, state, id, "Pinggy tunnel stopped".into());
            emit_system_log(app, state, id, "public URLs released".into());
            let stopped = state.provider_runtime.mark_status(
                PINGGY_PROVIDER_ID,
                id,
                TunnelRuntimeState::Stopped,
                "Pinggy stopped",
            );
            emit_status(app, stopped.clone());
            Ok(stopped)
        }
        Err(error) => {
            watchdog_relay::release(state, PINGGY_PROVIDER_ID, id);
            provider_lifecycle::mark_errored_with_log(
                app,
                state,
                PINGGY_PROVIDER_ID,
                id,
                error.to_string(),
            );
            Err(error)
        }
    }
}

pub fn stop_all_tunnels(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let mut errors = Vec::new();
    for tunnel in data::data(state).tunnels {
        if let Err(error) = stop_tunnel(app, state, &tunnel.id) {
            errors.push(format!("{}({}): {error}", tunnel.name, tunnel.id));
        }
    }
    if let Err(error) = cleanup_pinggy(app) {
        errors.push(format!("managed Pinggy process cleanup failed: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Msg(errors.join("; ")))
    }
}

pub fn cleanup_for_delete(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    let current = state.provider_runtime.reconcile(PINGGY_PROVIDER_ID, id);
    let stopped = stop_tunnel(app, state, id)?;
    provider_lifecycle::wait_for_inactive_before_delete(
        app,
        state,
        PINGGY_PROVIDER_ID,
        id,
        stopped,
    )?;
    cleanup_pinggy_for_runtime_pid(app, current.pid)?;
    watchdog_relay::release(state, PINGGY_PROVIDER_ID, id);
    let info = state.provider_runtime.mark_status(
        PINGGY_PROVIDER_ID,
        id,
        TunnelRuntimeState::Stopped,
        "Pinggy stopped",
    );
    emit_status(app, info);
    Ok(())
}

pub fn tunnel_status(_app: &AppHandle, state: &AppState, id: &str) -> TunnelRuntimeInfo {
    state.provider_runtime.reconcile(PINGGY_PROVIDER_ID, id)
}

pub fn handle_watchdog_event<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    event: WatchdogEvent,
) {
    match event {
        WatchdogEvent::OwnerAttached { .. }
        | WatchdogEvent::OwnerLost { .. }
        | WatchdogEvent::OwnerHeartbeatExpired
        | WatchdogEvent::RelayError { .. }
        | WatchdogEvent::RelayStats { .. } => {}
        WatchdogEvent::ProcessStarted { tunnel_id, pid, .. } => {
            let info = state.provider_runtime.mark_spawned(
                PINGGY_PROVIDER_ID,
                &tunnel_id,
                pid,
                "Pinggy process started",
            );
            emit_status(app, info);
        }
        WatchdogEvent::ProcessSnapshot {
            tunnel_id,
            pid,
            recent_logs,
            ..
        } => {
            state.provider_runtime.mark_spawned(
                PINGGY_PROVIDER_ID,
                &tunnel_id,
                pid,
                provider_log::watchdog_recovered_line("Pinggy"),
            );
            for line in recent_logs {
                capture_public_urls_from_log(app, state, &tunnel_id, &line);
            }
            let info = state.provider_runtime.mark_status(
                PINGGY_PROVIDER_ID,
                &tunnel_id,
                TunnelRuntimeState::Running,
                provider_log::watchdog_recovered_line("Pinggy"),
            );
            emit_status(app, info);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_recovered_line("Pinggy"),
            );
        }
        WatchdogEvent::ProcessLog {
            tunnel_id, line, ..
        } => {
            capture_runtime_warning(app, state, &tunnel_id, &line);
            capture_public_urls_from_log(app, state, &tunnel_id, &line);
            emit_native_log(app, state, &tunnel_id, line);
        }
        WatchdogEvent::ProcessExit {
            tunnel_id,
            success,
            cleanup_success,
            cleanup_error,
            ..
        } => {
            watchdog_relay::clear_local(state, PINGGY_PROVIDER_ID, &tunnel_id);
            let ok = watchdog_exit_clean(success, cleanup_success);
            let message = provider_log::watchdog_exit_state_message(
                "Pinggy",
                success,
                cleanup_success,
                cleanup_error.as_deref(),
            );
            if ok {
                emit_system_log(
                    app,
                    state,
                    &tunnel_id,
                    "process exited; public URLs released".into(),
                );
            } else {
                emit_system_log(app, state, &tunnel_id, message.clone());
            }
            let info =
                state
                    .provider_runtime
                    .mark_exit(PINGGY_PROVIDER_ID, &tunnel_id, ok, message);
            emit_status(app, info);
        }
        WatchdogEvent::ProcessError {
            tunnel_id, message, ..
        } => {
            watchdog_relay::clear_local(state, PINGGY_PROVIDER_ID, &tunnel_id);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_error_line(&message),
            );
            let info = state.provider_runtime.mark_status(
                PINGGY_PROVIDER_ID,
                &tunnel_id,
                TunnelRuntimeState::Errored,
                message,
            );
            emit_status(app, info);
        }
    }
}

fn capture_runtime_warning<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: &str,
) {
    let Some(message) = pinggy_warning_message(line) else {
        return;
    };
    let current = state.provider_runtime.info(PINGGY_PROVIDER_ID, tunnel_id);
    if current.status == TunnelRuntimeState::Errored || current.message == message {
        return;
    }
    let info = state.provider_runtime.mark_status(
        PINGGY_PROVIDER_ID,
        tunnel_id,
        TunnelRuntimeState::Warning,
        message,
    );
    emit_status(app, info);
}

fn pinggy_warning_message(line: &str) -> Option<String> {
    let message = line.trim();
    if message.is_empty() {
        return None;
    }
    let lower = message.to_ascii_lowercase();
    if lower.contains("warning:") && lower.contains("not authenticated") {
        return Some(message.to_string());
    }
    None
}

pub fn handle_watchdog_eof<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    for (provider_id, tunnel_id) in state.provider_runtime.active_keys() {
        if provider_id != PINGGY_PROVIDER_ID {
            continue;
        }
        let info = state.provider_runtime.mark_status(
            PINGGY_PROVIDER_ID,
            &tunnel_id,
            TunnelRuntimeState::Errored,
            provider_log::WATCHDOG_STREAM_CLOSED_MESSAGE,
        );
        watchdog_relay::clear_local(state, PINGGY_PROVIDER_ID, &tunnel_id);
        provider_log::emit_watchdog_stream_closed(app, state, PINGGY_PROVIDER_ID, &tunnel_id);
        emit_status(app, info);
    }
}

fn cleanup_pinggy(app: &AppHandle) -> AppResult<()> {
    let _ = environment::stop_daemon(app);
    cleanup_managed_pinggy_processes(app).map(|_| ())
}

fn cleanup_pinggy_for_runtime_pid(app: &AppHandle, pid: Option<u32>) -> AppResult<()> {
    let Some(pid) = pid else {
        return Ok(());
    };
    let managed_exe = normalize_path(&paths::managed_exe(app)?);
    let mut system = System::new_all();
    let Some(process) = system.process(Pid::from_u32(pid)) else {
        return Ok(());
    };
    let Some(program_path) = pinggy_program_path_from_cmd(process.cmd()) else {
        return Ok(());
    };
    if normalize_path(&program_path) != managed_exe {
        return Ok(());
    }
    terminate_pinggy_process(&mut system, Pid::from_u32(pid), &program_path)
}

fn cleanup_managed_pinggy_processes(app: &AppHandle) -> AppResult<Vec<PinggyProcess>> {
    let exe = normalize_path(&paths::managed_exe(app)?);
    let mut system = System::new_all();
    let targets = find_pinggy_processes(&system, &exe);
    terminate_pinggy_processes(&mut system, &targets)?;
    Ok(targets)
}

fn find_pinggy_processes(system: &System, managed_exe: &Path) -> Vec<PinggyProcess> {
    let current_pid = std::process::id();
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            if pid.as_u32() == current_pid {
                return None;
            }
            let program_path = pinggy_program_path_from_cmd(process.cmd())?;
            if normalize_path(&program_path) != managed_exe {
                return None;
            }
            Some(PinggyProcess {
                pid: pid.as_u32(),
                program_path,
            })
        })
        .collect()
}

fn pinggy_program_path_from_cmd(cmd: &[OsString]) -> Option<PathBuf> {
    cmd.first().map(PathBuf::from)
}

fn terminate_pinggy_processes(system: &mut System, targets: &[PinggyProcess]) -> AppResult<()> {
    for target in targets {
        crate::diag::info(
            &crate::diag::provider_scope(PINGGY_PROVIDER_ID),
            format!(
                "cleaning orphan Pinggy process pid={} program={}",
                target.pid,
                target.program_path.display()
            ),
        );
        terminate_pinggy_process(system, Pid::from_u32(target.pid), &target.program_path)?;
    }
    Ok(())
}

fn terminate_pinggy_process(system: &mut System, pid: Pid, program_path: &Path) -> AppResult<()> {
    let Some(process) = system.process(pid) else {
        return Ok(());
    };
    let sent = process
        .kill_with(Signal::Term)
        .unwrap_or_else(|| process.kill());
    if !sent {
        return Err(AppError::Msg(format!(
            "failed to stop Pinggy process: pid={} program={}",
            pid.as_u32(),
            program_path.display()
        )));
    }
    if wait_pinggy_exit(system, pid, PINGGY_STOP_GRACE) {
        return Ok(());
    }

    let Some(process) = system.process(pid) else {
        return Ok(());
    };
    if !process.kill() {
        return Err(AppError::Msg(format!(
            "failed to kill Pinggy process: pid={} program={}",
            pid.as_u32(),
            program_path.display()
        )));
    }
    if wait_pinggy_exit(system, pid, PINGGY_KILL_GRACE) {
        Ok(())
    } else {
        Err(AppError::Msg(format!(
            "Pinggy process did not exit; stop it manually and retry: pid={} program={}",
            pid.as_u32(),
            program_path.display()
        )))
    }
}

fn wait_pinggy_exit(system: &mut System, pid: Pid, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        match system.process(pid) {
            None => return true,
            Some(process) if process.status() == ProcessStatus::Zombie => return true,
            Some(_) => {}
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn validate_tunnel(tunnel: &PinggyTunnel) -> AppResult<()> {
    if tunnel.token.trim().is_empty() {
        return Err(AppError::Msg("Pinggy token is required".into()));
    }
    let mut names = HashSet::new();
    let mut active = 0usize;
    for endpoint in enabled_endpoints(tunnel) {
        active += 1;
        if endpoint.name.trim().is_empty() {
            return Err(AppError::Msg("Pinggy tunnel name is required".into()));
        }
        if endpoint.local_addr.trim().is_empty() {
            return Err(AppError::Msg("Pinggy local address is required".into()));
        }
        let tunnel_type = endpoint.tunnel_type.trim();
        if !matches!(tunnel_type, "http" | "tcp" | "udp" | "tls" | "tlstcp") {
            return Err(AppError::Msg(
                "Pinggy tunnel type must be http, tcp, udp, tls, or tlstcp".into(),
            ));
        }
        if !names.insert(endpoint.name.trim().to_ascii_lowercase()) {
            return Err(AppError::Msg("Pinggy tunnel names must be unique".into()));
        }
    }
    if active == 0 {
        return Err(AppError::Msg(
            "at least one enabled Pinggy tunnel is required".into(),
        ));
    }
    if active > 1 {
        return Err(AppError::Msg(
            "Pinggy currently supports one enabled tunnel per connection".into(),
        ));
    }
    if tunnel.server.trim().is_empty() {
        return Err(AppError::Msg("Pinggy server is required".into()));
    }
    if tunnel.server_port == 0 {
        return Err(AppError::Msg("Pinggy server port is required".into()));
    }
    Ok(())
}

fn enabled_endpoint(tunnel: &PinggyTunnel) -> Option<&PinggyEndpoint> {
    enabled_endpoints(tunnel).next()
}

fn enabled_endpoints(tunnel: &PinggyTunnel) -> impl Iterator<Item = &PinggyEndpoint> {
    tunnel.endpoints.iter().filter(|endpoint| endpoint.enabled)
}

fn capture_public_urls_from_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: &str,
) -> bool {
    let endpoint_name = pinggy_endpoint_name_for_log(state, tunnel_id, line);
    let new_urls = pinggy_public_urls_from_log(line, &endpoint_name);
    if new_urls.is_empty() {
        return false;
    }
    let current = state.provider_runtime.info(PINGGY_PROVIDER_ID, tunnel_id);
    let mut urls = public_urls_from_details(&current.details);
    let mut added = Vec::new();
    for item in new_urls {
        if urls
            .iter()
            .any(|existing| existing.public_url == item.public_url)
        {
            continue;
        }
        urls.push(item.clone());
        added.push(item);
    }
    if added.is_empty() {
        return false;
    }
    state
        .provider_runtime
        .set_details(PINGGY_PROVIDER_ID, tunnel_id, public_url_details(urls));
    for item in added {
        emit_system_log(
            app,
            state,
            tunnel_id,
            format!("public URL ready: {} -> {}", item.name, item.public_url),
        );
    }
    let current = state.provider_runtime.info(PINGGY_PROVIDER_ID, tunnel_id);
    let info = if current.status == TunnelRuntimeState::Starting {
        state.provider_runtime.mark_status(
            PINGGY_PROVIDER_ID,
            tunnel_id,
            TunnelRuntimeState::Running,
            "Pinggy running",
        )
    } else {
        current
    };
    emit_status(app, info);
    true
}

fn pinggy_endpoint_name_for_log(state: &AppState, tunnel_id: &str, line: &str) -> String {
    data::current_tunnel(state, tunnel_id)
        .ok()
        .and_then(|tunnel| {
            enabled_endpoints(&tunnel)
                .find(|endpoint| {
                    let local = endpoint.local_addr.trim();
                    !local.is_empty() && line.contains(local)
                })
                .or_else(|| enabled_endpoint(&tunnel))
                .map(|endpoint| endpoint.name.trim().to_string())
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_default()
}

fn pinggy_public_urls_from_log(line: &str, endpoint_name: &str) -> Vec<RuntimePublicUrl> {
    extract_public_urls(line)
        .into_iter()
        .filter_map(|public_url| {
            let proto = url_proto(&public_url).to_string();
            RuntimePublicUrl::new(endpoint_name, proto, public_url)
        })
        .collect()
}

fn extract_public_urls(line: &str) -> Vec<String> {
    const SCHEMES: [&str; 5] = ["https://", "http://", "tcp://", "udp://", "tls://"];
    let mut urls = Vec::new();
    let mut offset = 0usize;

    while offset < line.len() {
        let Some((start, _scheme)) = SCHEMES
            .iter()
            .filter_map(|scheme| {
                line[offset..]
                    .find(scheme)
                    .map(|index| (offset + index, *scheme))
            })
            .min_by_key(|(index, _)| *index)
        else {
            break;
        };
        let tail = &line[start..];
        let end = tail
            .char_indices()
            .find(|(_, ch)| is_url_boundary(*ch))
            .map(|(index, _)| index)
            .unwrap_or(tail.len());
        let public_url = trim_url_token(&tail[..end]);
        if is_public_url(&public_url) && !urls.iter().any(|item| item == &public_url) {
            urls.push(public_url);
        }
        offset = start + end.max(1);
    }

    urls
}

fn is_url_boundary(ch: char) -> bool {
    ch.is_whitespace()
        || ch.is_control()
        || matches!(
            ch,
            '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
        )
}

fn trim_url_token(value: &str) -> String {
    value
        .trim_matches(|ch| matches!(ch, ',' | ';' | '.' | ':' | ')' | ']' | '}' | '"' | '\''))
        .to_string()
}

fn is_public_url(value: &str) -> bool {
    let Some(host) = url_host(value) else {
        return false;
    };
    !is_local_host(&host) && !is_pinggy_control_host(&host)
}

fn url_proto(value: &str) -> &str {
    value
        .split_once("://")
        .map(|(proto, _)| proto)
        .unwrap_or("")
}

fn url_host(value: &str) -> Option<String> {
    let after_scheme = value.split_once("://")?.1;
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or_else(|| {
            after_scheme
                .split(['/', '?', '#'])
                .next()
                .unwrap_or_default()
        });
    if authority.starts_with('[') {
        return authority
            .trim_start_matches('[')
            .split(']')
            .next()
            .map(|host| host.to_ascii_lowercase())
            .filter(|host| !host.is_empty());
    }
    authority
        .split(':')
        .next()
        .map(|host| host.to_ascii_lowercase())
        .filter(|host| !host.is_empty())
}

fn is_local_host(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "127.0.0.1"
        || host.starts_with("127.")
        || host == "0.0.0.0"
        || host == "::1"
}

fn is_pinggy_control_host(host: &str) -> bool {
    matches!(
        host,
        "pinggy.io" | "www.pinggy.io" | "dashboard.pinggy.io" | "docs.pinggy.io" | "api.pinggy.io"
    )
}

fn prepare_traffic_relay(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    tunnel: &mut PinggyTunnel,
) -> AppResult<()> {
    if !state.config.settings().traffic_stats_enabled {
        watchdog_relay::release(state, PINGGY_PROVIDER_ID, id);
        return Ok(());
    }
    let endpoints = enabled_endpoints(tunnel)
        .map(|endpoint| watchdog_relay::RelayEndpointPlan {
            endpoint_id: endpoint.id.clone(),
            endpoint_name: endpoint.name.clone(),
            endpoint_type: endpoint.tunnel_type.clone(),
            protocol: if endpoint.tunnel_type.trim().eq_ignore_ascii_case("udp") {
                WatchdogRelayProtocol::Udp
            } else {
                WatchdogRelayProtocol::Tcp
            },
            target: endpoint.local_addr.clone(),
        })
        .collect::<Vec<_>>();
    let bindings = watchdog_relay::prepare(app, state, PINGGY_PROVIDER_ID, id, endpoints)?;
    for binding in bindings {
        if let Some(endpoint) = tunnel
            .endpoints
            .iter_mut()
            .find(|endpoint| endpoint.id == binding.endpoint_id)
        {
            watchdog_relay::apply_binding(&mut endpoint.local_addr, &binding);
        }
    }
    Ok(())
}

fn emit_system_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: String,
) {
    provider_log::emit_system(app, state, PINGGY_PROVIDER_ID, tunnel_id, line);
}

fn emit_native_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: String,
) {
    provider_log::emit_native(app, state, PINGGY_PROVIDER_ID, tunnel_id, line);
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
    fn pinggy_command_args_use_runtime_endpoint() {
        let mut tunnel = PinggyTunnel::new("edge");
        tunnel.token = "qui4SbhssUz".into();
        tunnel.server = "free.pinggy.io".into();
        tunnel.server_port = 443;
        tunnel.endpoints = vec![PinggyEndpoint {
            name: "web".into(),
            tunnel_type: "http".into(),
            local_addr: "http://127.0.0.1:53136".into(),
            enabled: true,
            ..Default::default()
        }];

        let args = command_args(&tunnel).unwrap();

        assert_eq!(
            args,
            vec![
                "--noTui",
                "--loglevel",
                "INFO",
                "--v",
                "--force",
                "--token",
                "qui4SbhssUz",
                "--type",
                "http",
                "--localport",
                "http://127.0.0.1:53136",
                "-p",
                "443",
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "ServerAliveInterval=30",
            ]
        );
        assert!(!args.iter().any(|arg| arg.contains("@free.pinggy.io")));
        assert!(!args.iter().any(|arg| arg == "--conf"));
    }

    #[test]
    fn pinggy_command_args_rejects_missing_enabled_endpoint() {
        let mut tunnel = PinggyTunnel::new("edge");
        tunnel.token = "qui4SbhssUz".into();

        let error = command_args(&tunnel).unwrap_err().to_string();

        assert!(error.contains("no enabled endpoint"));
    }

    #[test]
    fn pinggy_validate_rejects_multiple_enabled_endpoints() {
        let mut tunnel = PinggyTunnel::new("edge");
        tunnel.token = "LkWtbpY5TOf".into();
        tunnel.endpoints = vec![
            PinggyEndpoint {
                name: "web".into(),
                local_addr: "localhost:8080".into(),
                enabled: true,
                ..Default::default()
            },
            PinggyEndpoint {
                name: "api".into(),
                local_addr: "localhost:8081".into(),
                enabled: true,
                ..Default::default()
            },
        ];

        assert!(validate_tunnel(&tunnel).is_err());
    }

    #[test]
    fn pinggy_log_extracts_public_urls() {
        let urls = pinggy_public_urls_from_log(
            "Forwarding https://demo.a.free.pinggy.link -> http://localhost:8080",
            "web",
        );

        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].name, "web");
        assert_eq!(urls[0].proto, "https");
        assert_eq!(urls[0].public_url, "https://demo.a.free.pinggy.link");
    }

    #[test]
    fn pinggy_log_filters_local_urls() {
        let urls = pinggy_public_urls_from_log(
            "Debugger http://127.0.0.1:4300 Forwarding http://localhost:8080",
            "web",
        );

        assert!(urls.is_empty());
    }

    #[test]
    fn pinggy_log_filters_control_urls() {
        let urls =
            pinggy_public_urls_from_log("Manage tunnels at https://dashboard.pinggy.io", "web");

        assert!(urls.is_empty());
    }

    #[test]
    fn pinggy_log_keeps_tcp_public_urls() {
        let urls = pinggy_public_urls_from_log("Forwarding tcp://demo.tcp.pinggy.io:12345", "ssh");

        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].proto, "tcp");
        assert_eq!(urls[0].public_url, "tcp://demo.tcp.pinggy.io:12345");
    }

    #[test]
    fn pinggy_warning_detects_unauthenticated_runtime_limit() {
        let message = "Warning: You are not authenticated. Your tunnel will expire in 60 minutes.";

        assert_eq!(pinggy_warning_message(message).as_deref(), Some(message));
        assert!(pinggy_warning_message("Forwarding https://demo.free.pinggy.net").is_none());
    }
}
