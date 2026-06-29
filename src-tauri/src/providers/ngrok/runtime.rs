use super::data;
use super::domain::{NgrokEndpoint, NgrokTunnel};
use super::environment;
use super::paths;
use crate::error::{AppError, AppResult};
use crate::providers::cli;
use crate::providers::contract::{
    watchdog_exit_clean, ProviderStatus, TunnelRuntimeInfo, TunnelRuntimeState,
    TunnelRuntimeStatusEvent, NGROK_PROVIDER_ID,
};
use crate::providers::runtime_public_url::{
    agent_runtime_details, allocate_local_agent_addr, fetch_agent_public_urls,
    inspect_url_from_details,
};
use crate::services::{process_watchdog, provider_lifecycle, provider_log, watchdog_relay};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};
use tunnelx_watchdog_protocol::{
    WatchdogCleanupAction, WatchdogCleanupPlan, WatchdogEvent, WatchdogRelayProtocol,
    WatchdogRequest, WatchdogStartProcessRequest, WatchdogStopProcessRequest,
};

const NGROK_STOP_GRACE: Duration = Duration::from_secs(2);
const NGROK_KILL_GRACE: Duration = Duration::from_secs(1);
const NGROK_AGENT_API_POLL_ATTEMPTS: usize = 30;
const NGROK_AGENT_API_POLL_INTERVAL: Duration = Duration::from_secs(1);
const NGROK_AGENT_API_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Deserialize, Serialize)]
struct NgrokConfig {
    #[serde(default)]
    version: String,
    #[serde(default)]
    authtoken: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    region: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    web_addr: String,
    #[serde(default)]
    tunnels: BTreeMap<String, NgrokEndpointConfig>,
}

#[derive(Deserialize, Serialize)]
struct NgrokEndpointConfig {
    #[serde(default)]
    proto: String,
    #[serde(default)]
    addr: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "String::is_empty")]
    domain: String,
}

pub fn status(app: &AppHandle, state: &AppState) -> ProviderStatus {
    environment::status(app, state)
}

pub fn cleanup_on_start(app: &AppHandle, _state: &AppState) -> AppResult<()> {
    let targets = cleanup_managed_ngrok_processes(app)?;
    for target in targets {
        remove_config_file(app, &target.config_path);
    }
    Ok(())
}

pub fn start_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let mut tunnel = data::current_tunnel(state, id)?;
    let info = state
        .provider_runtime
        .begin_start(NGROK_PROVIDER_ID, id, "ngrok starting")?;
    emit_status(app, info.clone());
    if let Err(error) = validate_tunnel(&tunnel) {
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_system_log(app, state, id, "resolving ngrok executable".into());
    let program = match environment::resolve_command(app) {
        Ok(program) => program,
        Err(error) => {
            mark_start_error(app, state, id, &error);
            return Err(error);
        }
    };
    emit_system_log(app, state, id, "ngrok executable ready".into());
    emit_system_log(app, state, id, "resolving ngrok local api".into());
    let inspect_addr = match allocate_local_agent_addr() {
        Ok(addr) => addr,
        Err(error) => {
            mark_start_error(app, state, id, &error);
            return Err(error);
        }
    };
    emit_system_log(
        app,
        state,
        id,
        format!("ngrok local api reserved: http://{inspect_addr}"),
    );
    if let Err(error) = prepare_traffic_relay(app, state, id, &mut tunnel) {
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    let with_details = state.provider_runtime.set_details(
        NGROK_PROVIDER_ID,
        id,
        agent_runtime_details(&inspect_addr, Vec::new()),
    );
    emit_status(app, with_details.clone());
    emit_system_log(app, state, id, "writing ngrok config".into());
    let config_file = match write_config(app, &tunnel, &inspect_addr) {
        Ok(path) => path,
        Err(error) => {
            mark_start_error(app, state, id, &error);
            return Err(error);
        }
    };
    if let Err(error) = data::save_config_path(app, state, id, config_file.clone()) {
        remove_config_file(app, Path::new(&config_file));
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_system_log(app, state, id, "ngrok config ready".into());
    emit_system_log(app, state, id, "sending ngrok start command".into());
    let command = WatchdogRequest::StartProcess(WatchdogStartProcessRequest {
        provider_id: NGROK_PROVIDER_ID.into(),
        tunnel_id: id.into(),
        program,
        args: vec![
            "start".into(),
            "--all".into(),
            "--config".into(),
            config_file.clone(),
        ],
        env: Vec::new(),
        stop_strategy: None,
        cleanup: Some(Box::new(WatchdogCleanupPlan {
            actions: vec![WatchdogCleanupAction::RemoveFile {
                path: config_file.clone(),
            }],
            retry_attempts: Some(1),
            retry_delay_ms: Some(0),
        })),
    });
    if let Err(error) = process_watchdog::send(app, state, command) {
        remove_config_file(app, Path::new(&config_file));
        let _ = data::clear_config_path(app, state, id);
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_system_log(app, state, id, "ngrok start command accepted".into());
    Ok(state.provider_runtime.reconcile(NGROK_PROVIDER_ID, id))
}

fn mark_start_error(app: &AppHandle, state: &AppState, id: &str, error: &AppError) {
    watchdog_relay::release(state, NGROK_PROVIDER_ID, id);
    provider_lifecycle::mark_errored_with_log(app, state, NGROK_PROVIDER_ID, id, error.to_string());
}

pub fn stop_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let current = state.provider_runtime.reconcile(NGROK_PROVIDER_ID, id);
    if !current.status.is_active() {
        watchdog_relay::release(state, NGROK_PROVIDER_ID, id);
        emit_system_log(app, state, id, "ngrok is not running".into());
        return Ok(current);
    }
    emit_system_log(app, state, id, "stopping ngrok agent".into());
    let info = state.provider_runtime.mark_status(
        NGROK_PROVIDER_ID,
        id,
        TunnelRuntimeState::Stopping,
        "ngrok stopping",
    );
    emit_status(app, info.clone());
    let command = WatchdogRequest::StopProcess(WatchdogStopProcessRequest {
        provider_id: NGROK_PROVIDER_ID.into(),
        tunnel_id: id.into(),
    });
    match process_watchdog::send_if_alive(state, command) {
        Ok(true) => Ok(info),
        Ok(false) => {
            cleanup_ngrok_for_tunnel(app, state, id)?;
            watchdog_relay::release(state, NGROK_PROVIDER_ID, id);
            emit_system_log(app, state, id, "ngrok agent stopped".into());
            emit_system_log(app, state, id, "public URLs released".into());
            let stopped = state.provider_runtime.mark_status(
                NGROK_PROVIDER_ID,
                id,
                TunnelRuntimeState::Stopped,
                "ngrok stopped",
            );
            emit_status(app, stopped.clone());
            Ok(stopped)
        }
        Err(error) => {
            watchdog_relay::release(state, NGROK_PROVIDER_ID, id);
            provider_lifecycle::mark_errored_with_log(
                app,
                state,
                NGROK_PROVIDER_ID,
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
    if let Err(error) = cleanup_managed_ngrok_processes(app) {
        errors.push(format!("managed ngrok process cleanup failed: {error}"));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Msg(errors.join("; ")))
    }
}

pub fn cleanup_for_delete(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    let stopped = stop_tunnel(app, state, id)?;
    provider_lifecycle::wait_for_inactive_before_delete(
        app,
        state,
        NGROK_PROVIDER_ID,
        id,
        stopped,
    )?;
    cleanup_ngrok_for_tunnel(app, state, id)?;
    watchdog_relay::release(state, NGROK_PROVIDER_ID, id);
    let info = state.provider_runtime.mark_status(
        NGROK_PROVIDER_ID,
        id,
        TunnelRuntimeState::Stopped,
        "ngrok stopped",
    );
    emit_status(app, info);
    Ok(())
}

pub fn tunnel_status(_app: &AppHandle, state: &AppState, id: &str) -> TunnelRuntimeInfo {
    state.provider_runtime.reconcile(NGROK_PROVIDER_ID, id)
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
                NGROK_PROVIDER_ID,
                &tunnel_id,
                pid,
                "ngrok process started",
            );
            emit_status(app, info);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                "ngrok process started; waiting for public URLs".into(),
            );
            // 成功与否完全交给公网地址轮询判定：拿到地址才 Running，
            // 超时则 Warning。进程刚起来时保持 Starting，不按时间假装连上。
            spawn_public_url_poll(app, state, tunnel_id);
        }
        WatchdogEvent::ProcessSnapshot {
            tunnel_id,
            pid,
            args,
            ..
        } => {
            state.provider_runtime.mark_spawned(
                NGROK_PROVIDER_ID,
                &tunnel_id,
                pid,
                provider_log::watchdog_recovered_line("ngrok"),
            );
            restore_runtime_details_from_snapshot(state, &tunnel_id, &args);
            let info = state.provider_runtime.mark_status(
                NGROK_PROVIDER_ID,
                &tunnel_id,
                TunnelRuntimeState::Running,
                provider_log::watchdog_recovered_line("ngrok"),
            );
            emit_status(app, info);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_recovered_line("ngrok"),
            );
            spawn_public_url_poll(app, state, tunnel_id);
        }
        WatchdogEvent::ProcessLog {
            tunnel_id, line, ..
        } => {
            emit_native_log(app, state, &tunnel_id, line);
        }
        WatchdogEvent::ProcessExit {
            tunnel_id,
            success,
            cleanup_success,
            cleanup_error,
            ..
        } => {
            watchdog_relay::clear_local(state, NGROK_PROVIDER_ID, &tunnel_id);
            let ok = watchdog_exit_clean(success, cleanup_success);
            let message = provider_log::watchdog_exit_state_message(
                "ngrok",
                success,
                cleanup_success,
                cleanup_error.as_deref(),
            );
            let info = state
                .provider_runtime
                .mark_exit(NGROK_PROVIDER_ID, &tunnel_id, ok, message);
            emit_system_log(app, state, &tunnel_id, info.message.clone());
            if info.status == TunnelRuntimeState::Stopped {
                emit_system_log(app, state, &tunnel_id, "public URLs released".into());
            }
            emit_status(app, info);
        }
        WatchdogEvent::ProcessError {
            tunnel_id, message, ..
        } => {
            watchdog_relay::clear_local(state, NGROK_PROVIDER_ID, &tunnel_id);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_error_line(&message),
            );
            let info = state.provider_runtime.mark_status(
                NGROK_PROVIDER_ID,
                &tunnel_id,
                TunnelRuntimeState::Errored,
                message,
            );
            emit_status(app, info);
        }
    }
}

pub fn handle_watchdog_eof<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    for (provider_id, tunnel_id) in state.provider_runtime.active_keys() {
        if provider_id != NGROK_PROVIDER_ID {
            continue;
        }
        let info = state.provider_runtime.mark_status(
            NGROK_PROVIDER_ID,
            &tunnel_id,
            TunnelRuntimeState::Errored,
            provider_log::WATCHDOG_STREAM_CLOSED_MESSAGE,
        );
        watchdog_relay::clear_local(state, NGROK_PROVIDER_ID, &tunnel_id);
        provider_log::emit_watchdog_stream_closed(app, state, NGROK_PROVIDER_ID, &tunnel_id);
        emit_status(app, info);
    }
}

fn cleanup_ngrok_for_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    let tunnel = data::current_tunnel(state, id)?;
    let config_file = if tunnel.config_file.trim().is_empty() {
        paths::config_file(app, id)?
    } else {
        PathBuf::from(tunnel.config_file.trim())
    };
    let targets = cleanup_ngrok_for_config(&config_file)?;
    if !targets.is_empty() {
        remove_config_file(app, &config_file);
    }
    Ok(())
}

fn ngrok_process_spec() -> cli::CliProcessSpec {
    cli::CliProcessSpec {
        label: "ngrok",
        exe_name: paths::exe_name(),
        binary_stem: "ngrok",
        stop_grace: NGROK_STOP_GRACE,
        kill_grace: NGROK_KILL_GRACE,
    }
}

fn cleanup_managed_ngrok_processes(app: &AppHandle) -> AppResult<Vec<cli::CliProcess>> {
    ngrok_process_spec().cleanup_under_dir(&paths::configs_dir(app)?)
}

fn cleanup_ngrok_for_config(config_path: &Path) -> AppResult<Vec<cli::CliProcess>> {
    ngrok_process_spec().cleanup_for_config(config_path)
}

fn remove_config_file(app: &AppHandle, path: &Path) {
    if let Ok(root) = paths::root(app) {
        crate::paths::remove_file_if_under(path, &root);
    }
}

fn write_config(app: &AppHandle, tunnel: &NgrokTunnel, inspect_addr: &str) -> AppResult<String> {
    validate_tunnel(tunnel)?;
    let path = paths::config_file(app, &tunnel.id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_yaml_ng::to_string(&config(tunnel, inspect_addr))
        .map_err(|error| AppError::Msg(format!("ngrok config serialization failed: {error}")))?;
    crate::paths::write_secret_file(&path, text.as_bytes())?;
    Ok(path.to_string_lossy().to_string())
}

fn validate_tunnel(tunnel: &NgrokTunnel) -> AppResult<()> {
    if tunnel.authtoken.trim().is_empty() {
        return Err(AppError::Msg("ngrok authtoken is required".into()));
    }
    let mut names = HashSet::new();
    let mut active = 0usize;
    for endpoint in enabled_endpoints(tunnel) {
        active += 1;
        if endpoint.name.trim().is_empty() {
            return Err(AppError::Msg("ngrok tunnel name is required".into()));
        }
        if endpoint.addr.trim().is_empty() {
            return Err(AppError::Msg("ngrok tunnel address is required".into()));
        }
        let proto = endpoint.proto.trim();
        if !matches!(proto, "http" | "tcp" | "tls") {
            return Err(AppError::Msg(
                "ngrok proto must be http, tcp, or tls".into(),
            ));
        }
        if !names.insert(endpoint.name.trim().to_ascii_lowercase()) {
            return Err(AppError::Msg("ngrok tunnel names must be unique".into()));
        }
    }
    if active == 0 {
        return Err(AppError::Msg(
            "at least one enabled ngrok tunnel is required".into(),
        ));
    }
    Ok(())
}

fn config(tunnel: &NgrokTunnel, inspect_addr: &str) -> NgrokConfig {
    let tunnels = enabled_endpoints(tunnel)
        .map(|endpoint| {
            (
                endpoint.name.trim().to_string(),
                NgrokEndpointConfig {
                    proto: endpoint.proto.trim().to_ascii_lowercase(),
                    addr: endpoint.addr.trim().to_string(),
                    domain: endpoint.domain.trim().to_string(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    NgrokConfig {
        version: "2".into(),
        authtoken: tunnel.authtoken.trim().to_string(),
        region: tunnel.region.trim().to_string(),
        web_addr: inspect_addr.trim().to_string(),
        tunnels,
    }
}

fn restore_runtime_details_from_snapshot(state: &AppState, tunnel_id: &str, args: &[String]) {
    let Some(inspect_addr) = snapshot_inspect_addr(state, tunnel_id, args) else {
        return;
    };
    state.provider_runtime.set_details(
        NGROK_PROVIDER_ID,
        tunnel_id,
        agent_runtime_details(&inspect_addr, Vec::new()),
    );
}

fn snapshot_inspect_addr(state: &AppState, tunnel_id: &str, args: &[String]) -> Option<String> {
    let config_file = ngrok_config_path_from_args(args).or_else(|| {
        data::current_tunnel(state, tunnel_id)
            .ok()
            .and_then(|tunnel| {
                let path = tunnel.config_file.trim();
                (!path.is_empty()).then(|| PathBuf::from(path))
            })
    })?;
    ngrok_inspect_addr_from_config(&config_file).ok()
}

fn ngrok_config_path_from_args(args: &[String]) -> Option<PathBuf> {
    let cmd = std::iter::once(OsString::from("ngrok"))
        .chain(args.iter().cloned().map(OsString::from))
        .collect::<Vec<_>>();
    cli::config_path_from_cmd(&cmd)
}

fn ngrok_inspect_addr_from_config(path: &Path) -> AppResult<String> {
    let text = std::fs::read_to_string(path)?;
    let config = serde_yaml_ng::from_str::<NgrokConfig>(&text)
        .map_err(|error| AppError::Msg(format!("ngrok config parse failed: {error}")))?;
    let addr = config.web_addr.trim();
    if addr.is_empty() {
        return Err(AppError::Msg(
            "ngrok config does not include web_addr".into(),
        ));
    }
    Ok(addr.to_string())
}

fn spawn_public_url_poll<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: String,
) {
    let app = app.clone();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        poll_public_urls(app, state, tunnel_id).await;
    });
}

async fn poll_public_urls<R: Runtime>(app: AppHandle<R>, state: AppState, tunnel_id: String) {
    let inspect_url = inspect_url_from_details(
        &state
            .provider_runtime
            .info(NGROK_PROVIDER_ID, &tunnel_id)
            .details,
    );
    let Some(inspect_url) = inspect_url else {
        provider_lifecycle::mark_start_confirmation_timed_out(
            &app,
            &state,
            NGROK_PROVIDER_ID,
            &tunnel_id,
            "local api is not available",
        );
        return;
    };
    let api_url = format!("{}/api/tunnels", inspect_url.trim_end_matches('/'));
    let mut last_error = None;
    for _ in 0..NGROK_AGENT_API_POLL_ATTEMPTS {
        if !state
            .provider_runtime
            .info(NGROK_PROVIDER_ID, &tunnel_id)
            .status
            .is_active()
        {
            return;
        }
        match fetch_agent_public_urls(&api_url, NGROK_AGENT_API_TIMEOUT).await {
            Ok(public_urls) if !public_urls.is_empty() => {
                let details = agent_runtime_details(&inspect_url, public_urls.clone());
                state
                    .provider_runtime
                    .set_details(NGROK_PROVIDER_ID, &tunnel_id, details);
                for item in public_urls {
                    emit_system_log(
                        &app,
                        &state,
                        &tunnel_id,
                        format!("public URL ready: {} -> {}", item.name, item.public_url),
                    );
                }
                let info = state.provider_runtime.mark_status(
                    NGROK_PROVIDER_ID,
                    &tunnel_id,
                    TunnelRuntimeState::Running,
                    "ngrok running",
                );
                emit_status(&app, info);
                return;
            }
            Ok(_) => {}
            Err(error) => last_error = Some(error.to_string()),
        }
        tokio::time::sleep(NGROK_AGENT_API_POLL_INTERVAL).await;
    }
    let message = last_error
        .map(|error| format!("public URLs were not reported before timeout: {error}"))
        .unwrap_or_else(|| "public URLs were not reported before timeout".into());
    provider_lifecycle::mark_start_confirmation_timed_out(
        &app,
        &state,
        NGROK_PROVIDER_ID,
        &tunnel_id,
        message,
    );
}

fn enabled_endpoints(tunnel: &NgrokTunnel) -> impl Iterator<Item = &NgrokEndpoint> {
    tunnel.endpoints.iter().filter(|endpoint| endpoint.enabled)
}

fn prepare_traffic_relay(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    tunnel: &mut NgrokTunnel,
) -> AppResult<()> {
    if !state.config.settings().traffic_stats_enabled {
        watchdog_relay::release(state, NGROK_PROVIDER_ID, id);
        return Ok(());
    }
    let endpoints = enabled_endpoints(tunnel)
        .map(|endpoint| watchdog_relay::RelayEndpointPlan {
            endpoint_id: endpoint.id.clone(),
            endpoint_name: endpoint.name.clone(),
            endpoint_type: endpoint.proto.clone(),
            protocol: WatchdogRelayProtocol::Tcp,
            target: endpoint.addr.clone(),
        })
        .collect::<Vec<_>>();
    let bindings = watchdog_relay::prepare(app, state, NGROK_PROVIDER_ID, id, endpoints)?;
    for binding in bindings {
        if let Some(endpoint) = tunnel
            .endpoints
            .iter_mut()
            .find(|endpoint| endpoint.id == binding.endpoint_id)
        {
            watchdog_relay::apply_binding(&mut endpoint.addr, &binding);
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
    provider_log::emit_system(app, state, NGROK_PROVIDER_ID, tunnel_id, line);
}

fn emit_native_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: String,
) {
    provider_log::emit_native(app, state, NGROK_PROVIDER_ID, tunnel_id, line);
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
    fn ngrok_config_contains_enabled_tunnels_only() {
        let mut tunnel = NgrokTunnel::new("edge");
        tunnel.authtoken = "token".into();
        tunnel.endpoints = vec![
            NgrokEndpoint {
                name: "web".into(),
                proto: "http".into(),
                addr: "http://localhost:8080".into(),
                domain: "app.example.com".into(),
                enabled: true,
                ..Default::default()
            },
            NgrokEndpoint {
                name: "off".into(),
                enabled: false,
                ..Default::default()
            },
        ];

        let text = serde_yaml_ng::to_string(&config(&tunnel, "127.0.0.1:49152")).unwrap();

        assert!(text.contains("authtoken: token"));
        assert!(text.contains("web_addr: 127.0.0.1:49152"));
        assert!(text.contains("web:"));
        assert!(text.contains("domain: app.example.com"));
        assert!(!text.contains("off:"));
    }

    #[test]
    fn ngrok_config_rejects_unknown_proto() {
        let mut tunnel = NgrokTunnel::new("edge");
        tunnel.authtoken = "token".into();
        tunnel.endpoints.push(NgrokEndpoint::default());
        tunnel.endpoints[0].proto = "udp".into();

        assert!(validate_tunnel(&tunnel).is_err());
    }

    #[test]
    fn ngrok_inspect_addr_restores_from_config_file() {
        let path = std::env::temp_dir().join(format!(
            "tunnelx-ngrok-test-{}-{}.yml",
            std::process::id(),
            crate::domain::now_ms()
        ));
        std::fs::write(
            &path,
            "version: '2'\nauthtoken: token\nweb_addr: 127.0.0.1:49152\ntunnels: {}\n",
        )
        .unwrap();

        assert_eq!(
            ngrok_inspect_addr_from_config(&path).unwrap(),
            "127.0.0.1:49152"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn ngrok_config_path_reads_watchdog_snapshot_args() {
        let args = vec![
            "start".into(),
            "--all".into(),
            "--config".into(),
            "/tmp/tunnelx/ngrok/configs/edge.yml".into(),
        ];

        assert_eq!(
            ngrok_config_path_from_args(&args),
            Some(PathBuf::from("/tmp/tunnelx/ngrok/configs/edge.yml"))
        );
    }
}
