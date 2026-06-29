use super::data;
use super::domain::{CpolarEndpoint, CpolarTunnel};
use super::environment;
use super::paths;
use crate::error::{AppError, AppResult};
use crate::providers::cli;
use crate::providers::contract::{
    watchdog_exit_clean, ProviderStatus, TunnelRuntimeInfo, TunnelRuntimeState,
    TunnelRuntimeStatusEvent, CPOLAR_PROVIDER_ID,
};
use crate::providers::runtime_public_url::{
    agent_runtime_details, allocate_local_agent_addr, inspect_url_from_details,
    public_urls_from_details, RuntimePublicUrl,
};
use crate::services::{process_watchdog, provider_lifecycle, provider_log, watchdog_relay};
use crate::state::AppState;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};
use tunnelx_watchdog_protocol::{
    WatchdogCleanupAction, WatchdogCleanupPlan, WatchdogEvent, WatchdogRelayProtocol,
    WatchdogRequest, WatchdogStartProcessRequest, WatchdogStopProcessRequest,
};

const CPOLAR_STOP_GRACE: Duration = Duration::from_secs(2);
const CPOLAR_KILL_GRACE: Duration = Duration::from_secs(1);
// cpolar 没有 ngrok 那种 /api/tunnels JSON 接口（实测返回 200+空 body）。
// 成功判定改为解析 cpolar stdout 里的 "Tunnel established at <公网地址>"——
// 这行对 http/tcp/udp 任意隧道类型通用，一行既是成功信号又带公网地址。
const CPOLAR_ESTABLISHED_MARKER: &str = "tunnel established at ";
// 进程起来后多久仍未出现 established 日志，就判定确认超时（Warning）。
const CPOLAR_CONFIRM_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize)]
struct CpolarConfig {
    authtoken: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    region: String,
    tunnels: BTreeMap<String, CpolarEndpointConfig>,
}

#[derive(Serialize)]
struct CpolarEndpointConfig {
    proto: String,
    addr: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    hostname: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    remote_addr: String,
}

pub fn status(app: &AppHandle, state: &AppState) -> ProviderStatus {
    environment::status(app, state)
}

pub fn cleanup_on_start(app: &AppHandle, _state: &AppState) -> AppResult<()> {
    let targets = cleanup_managed_cpolar_processes(app)?;
    for target in targets {
        remove_config_file(app, &target.config_path);
    }
    Ok(())
}

pub fn start_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let mut tunnel = data::current_tunnel(state, id)?;
    let info = state
        .provider_runtime
        .begin_start(CPOLAR_PROVIDER_ID, id, "cpolar starting")?;
    emit_status(app, info.clone());
    if let Err(error) = validate_tunnel(&tunnel) {
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_system_log(app, state, id, "resolving cpolar executable".into());
    let program = match environment::resolve_command(app) {
        Ok(program) => program,
        Err(error) => {
            mark_start_error(app, state, id, &error);
            return Err(error);
        }
    };
    emit_system_log(app, state, id, "cpolar executable ready".into());
    emit_system_log(app, state, id, "resolving cpolar local api".into());
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
        format!("cpolar local api reserved: http://{inspect_addr}"),
    );
    if let Err(error) = prepare_traffic_relay(app, state, id, &mut tunnel) {
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    let with_details = state.provider_runtime.set_details(
        CPOLAR_PROVIDER_ID,
        id,
        agent_runtime_details(&inspect_addr, Vec::new()),
    );
    emit_status(app, with_details.clone());
    emit_system_log(app, state, id, "writing cpolar config".into());
    let config_file = match write_config(app, &tunnel) {
        Ok(path) => path,
        Err(error) => {
            mark_start_error(app, state, id, &error);
            return Err(error);
        }
    };
    if let Err(error) = data::save_config_path(app, state, id, config_file.clone()) {
        remove_config_file(app, Path::new(&config_file));
        let _ = data::clear_config_path(app, state, id);
        mark_start_error(app, state, id, &error);
        return Err(error);
    }
    emit_system_log(app, state, id, "cpolar config ready".into());
    emit_system_log(app, state, id, "sending cpolar start command".into());
    let command = WatchdogRequest::StartProcess(WatchdogStartProcessRequest {
        provider_id: CPOLAR_PROVIDER_ID.into(),
        tunnel_id: id.into(),
        program,
        args: vec![
            "start-all".into(),
            format!("-config={config_file}"),
            format!("-inspect-addr={inspect_addr}"),
            "-log=stdout".into(),
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
    emit_system_log(app, state, id, "cpolar start command accepted".into());
    Ok(state.provider_runtime.reconcile(CPOLAR_PROVIDER_ID, id))
}

fn mark_start_error(app: &AppHandle, state: &AppState, id: &str, error: &AppError) {
    watchdog_relay::release(state, CPOLAR_PROVIDER_ID, id);
    provider_lifecycle::mark_errored_with_log(
        app,
        state,
        CPOLAR_PROVIDER_ID,
        id,
        error.to_string(),
    );
}

pub fn stop_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let current = state.provider_runtime.reconcile(CPOLAR_PROVIDER_ID, id);
    if !current.status.is_active() {
        watchdog_relay::release(state, CPOLAR_PROVIDER_ID, id);
        emit_system_log(app, state, id, "cpolar is not running".into());
        return Ok(current);
    }
    emit_system_log(
        app,
        state,
        id,
        "stopping cpolar agent; public URLs will be released".into(),
    );
    let info = state.provider_runtime.mark_status(
        CPOLAR_PROVIDER_ID,
        id,
        TunnelRuntimeState::Stopping,
        "cpolar stopping",
    );
    emit_status(app, info.clone());
    let command = WatchdogRequest::StopProcess(WatchdogStopProcessRequest {
        provider_id: CPOLAR_PROVIDER_ID.into(),
        tunnel_id: id.into(),
    });
    match process_watchdog::send_if_alive(state, command) {
        Ok(true) => Ok(info),
        Ok(false) => {
            cleanup_cpolar_for_tunnel(app, state, id)?;
            watchdog_relay::release(state, CPOLAR_PROVIDER_ID, id);
            emit_system_log(app, state, id, "cpolar agent stopped".into());
            emit_system_log(app, state, id, "public URLs released".into());
            let stopped = state.provider_runtime.mark_status(
                CPOLAR_PROVIDER_ID,
                id,
                TunnelRuntimeState::Stopped,
                "cpolar stopped",
            );
            emit_status(app, stopped.clone());
            Ok(stopped)
        }
        Err(error) => {
            watchdog_relay::release(state, CPOLAR_PROVIDER_ID, id);
            provider_lifecycle::mark_errored_with_log(
                app,
                state,
                CPOLAR_PROVIDER_ID,
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
    if let Err(error) = cleanup_managed_cpolar_processes(app) {
        errors.push(format!("managed cpolar process cleanup failed: {error}"));
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
        CPOLAR_PROVIDER_ID,
        id,
        stopped,
    )?;
    cleanup_cpolar_for_tunnel(app, state, id)?;
    watchdog_relay::release(state, CPOLAR_PROVIDER_ID, id);
    let info = state.provider_runtime.mark_status(
        CPOLAR_PROVIDER_ID,
        id,
        TunnelRuntimeState::Stopped,
        "cpolar stopped",
    );
    emit_status(app, info);
    Ok(())
}

pub fn tunnel_status(_app: &AppHandle, state: &AppState, id: &str) -> TunnelRuntimeInfo {
    state.provider_runtime.reconcile(CPOLAR_PROVIDER_ID, id)
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
                CPOLAR_PROVIDER_ID,
                &tunnel_id,
                pid,
                "cpolar process started",
            );
            emit_status(app, info);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                "cpolar process started; waiting for tunnel to be established".into(),
            );
            // 成功以 ProcessLog 里的 "Tunnel established at <addr>" 为准；超时仍未
            // 出现则降级 Warning。不再仅凭进程存活就判成功（那会掩盖真连不上的情况）。
            spawn_confirmation_timeout(app, state, tunnel_id);
        }
        WatchdogEvent::ProcessSnapshot {
            tunnel_id,
            pid,
            args,
            ..
        } => {
            state.provider_runtime.mark_spawned(
                CPOLAR_PROVIDER_ID,
                &tunnel_id,
                pid,
                provider_log::watchdog_recovered_line("cpolar"),
            );
            restore_runtime_details_from_snapshot(state, &tunnel_id, &args);
            let info = state.provider_runtime.mark_status(
                CPOLAR_PROVIDER_ID,
                &tunnel_id,
                TunnelRuntimeState::Running,
                provider_log::watchdog_recovered_line("cpolar"),
            );
            emit_status(app, info);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_recovered_line("cpolar"),
            );
        }
        WatchdogEvent::ProcessLog {
            tunnel_id, line, ..
        } => {
            capture_cpolar_public_url(app, state, &tunnel_id, &line);
            emit_native_log(app, state, &tunnel_id, line);
        }
        WatchdogEvent::ProcessExit {
            tunnel_id,
            success,
            cleanup_success,
            cleanup_error,
            ..
        } => {
            watchdog_relay::clear_local(state, CPOLAR_PROVIDER_ID, &tunnel_id);
            let ok = watchdog_exit_clean(success, cleanup_success);
            let message = provider_log::watchdog_exit_state_message(
                "cpolar",
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
                    .mark_exit(CPOLAR_PROVIDER_ID, &tunnel_id, ok, message);
            emit_status(app, info);
        }
        WatchdogEvent::ProcessError {
            tunnel_id, message, ..
        } => {
            watchdog_relay::clear_local(state, CPOLAR_PROVIDER_ID, &tunnel_id);
            emit_system_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_error_line(&message),
            );
            let info = state.provider_runtime.mark_status(
                CPOLAR_PROVIDER_ID,
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
        if provider_id != CPOLAR_PROVIDER_ID {
            continue;
        }
        let info = state.provider_runtime.mark_status(
            CPOLAR_PROVIDER_ID,
            &tunnel_id,
            TunnelRuntimeState::Errored,
            provider_log::WATCHDOG_STREAM_CLOSED_MESSAGE,
        );
        watchdog_relay::clear_local(state, CPOLAR_PROVIDER_ID, &tunnel_id);
        provider_log::emit_watchdog_stream_closed(app, state, CPOLAR_PROVIDER_ID, &tunnel_id);
        emit_status(app, info);
    }
}

fn cleanup_cpolar_for_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    let tunnel = data::current_tunnel(state, id)?;
    let config_file = if tunnel.config_file.trim().is_empty() {
        paths::config_file(app, id)?
    } else {
        PathBuf::from(tunnel.config_file.trim())
    };
    let targets = cleanup_cpolar_for_config(&config_file)?;
    if !targets.is_empty() {
        remove_config_file(app, &config_file);
    }
    Ok(())
}

fn cpolar_process_spec() -> cli::CliProcessSpec {
    cli::CliProcessSpec {
        label: "cpolar",
        exe_name: paths::exe_name(),
        binary_stem: "cpolar",
        stop_grace: CPOLAR_STOP_GRACE,
        kill_grace: CPOLAR_KILL_GRACE,
    }
}

fn cleanup_managed_cpolar_processes(app: &AppHandle) -> AppResult<Vec<cli::CliProcess>> {
    cpolar_process_spec().cleanup_under_dir(&paths::configs_dir(app)?)
}

fn cleanup_cpolar_for_config(config_path: &Path) -> AppResult<Vec<cli::CliProcess>> {
    cpolar_process_spec().cleanup_for_config(config_path)
}

fn remove_config_file(app: &AppHandle, path: &Path) {
    if let Ok(root) = paths::root(app) {
        crate::paths::remove_file_if_under(path, &root);
    }
}

fn write_config(app: &AppHandle, tunnel: &CpolarTunnel) -> AppResult<String> {
    validate_tunnel(tunnel)?;
    let path = paths::config_file(app, &tunnel.id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_yaml_ng::to_string(&config(tunnel))
        .map_err(|error| AppError::Msg(format!("cpolar config serialization failed: {error}")))?;
    crate::paths::write_secret_file(&path, text.as_bytes())?;
    Ok(path.to_string_lossy().to_string())
}

fn validate_tunnel(tunnel: &CpolarTunnel) -> AppResult<()> {
    if tunnel.authtoken.trim().is_empty() {
        return Err(AppError::Msg("cpolar authtoken is required".into()));
    }
    let mut names = HashSet::new();
    let mut active = 0usize;
    for endpoint in enabled_endpoints(tunnel) {
        active += 1;
        if endpoint.name.trim().is_empty() {
            return Err(AppError::Msg("cpolar tunnel name is required".into()));
        }
        if endpoint.addr.trim().is_empty() {
            return Err(AppError::Msg("cpolar tunnel address is required".into()));
        }
        let proto = endpoint.proto.trim();
        if !matches!(proto, "http" | "tcp") {
            return Err(AppError::Msg("cpolar proto must be http or tcp".into()));
        }
        if !names.insert(endpoint.name.trim().to_ascii_lowercase()) {
            return Err(AppError::Msg("cpolar tunnel names must be unique".into()));
        }
    }
    if active == 0 {
        return Err(AppError::Msg(
            "at least one enabled cpolar tunnel is required".into(),
        ));
    }
    Ok(())
}

fn config(tunnel: &CpolarTunnel) -> CpolarConfig {
    let tunnels = enabled_endpoints(tunnel)
        .map(|endpoint| {
            (
                endpoint.name.trim().to_string(),
                CpolarEndpointConfig {
                    proto: endpoint.proto.trim().to_ascii_lowercase(),
                    addr: endpoint.addr.trim().to_string(),
                    hostname: endpoint.hostname.trim().to_string(),
                    remote_addr: endpoint.remote_addr.trim().to_string(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    CpolarConfig {
        authtoken: tunnel.authtoken.trim().to_string(),
        region: tunnel.region.trim().to_string(),
        tunnels,
    }
}

fn restore_runtime_details_from_snapshot(state: &AppState, tunnel_id: &str, args: &[String]) {
    let Some(inspect_addr) = cpolar_inspect_addr_from_args(args) else {
        return;
    };
    state.provider_runtime.set_details(
        CPOLAR_PROVIDER_ID,
        tunnel_id,
        agent_runtime_details(&inspect_addr, Vec::new()),
    );
}

fn cpolar_inspect_addr_from_args(args: &[String]) -> Option<String> {
    args.iter().enumerate().find_map(|(index, item)| {
        let text = item.trim();
        text.strip_prefix("-inspect-addr=")
            .or_else(|| text.strip_prefix("--inspect-addr="))
            .map(str::to_string)
            .or_else(|| {
                matches!(text, "-inspect-addr" | "--inspect-addr").then(|| {
                    args.get(index + 1)
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                })?
            })
    })
}

// 进程起来后等一段时间；若仍停在 Starting（没等到 established 日志）→ Warning。
// 日志若已确认 Running，mark_start_confirmation_timed_out 会因状态非 Starting 而 no-op。
fn spawn_confirmation_timeout<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: String,
) {
    let app = app.clone();
    let state = state.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CPOLAR_CONFIRM_TIMEOUT).await;
        provider_lifecycle::mark_start_confirmation_timed_out(
            &app,
            &state,
            CPOLAR_PROVIDER_ID,
            &tunnel_id,
            "no \"tunnel established\" log within timeout",
        );
    });
}

// 解析 cpolar 日志里的 "Tunnel established at <公网地址>"：命中即把公网地址写入
// details，并把仍在 Starting 的隧道确认为 Running。类型无关（http/tcp/udp 通用）。
fn capture_cpolar_public_url<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: &str,
) {
    let lower = line.to_ascii_lowercase();
    let Some(pos) = lower.find(CPOLAR_ESTABLISHED_MARKER) else {
        return;
    };
    // 公网地址不含空白，取 "at " 之后的第一个 token，避免误吞行尾附加文本。
    let addr = line[pos + CPOLAR_ESTABLISHED_MARKER.len()..]
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches(['.', ',', ';', ')', '"', '\''])
        .to_string();
    if addr.is_empty() {
        return;
    }

    let details = state
        .provider_runtime
        .info(CPOLAR_PROVIDER_ID, tunnel_id)
        .details;
    let inspect_url = inspect_url_from_details(&details).unwrap_or_default();
    let mut urls = public_urls_from_details(&details);
    if !urls.iter().any(|item| item.public_url == addr) {
        let proto = addr
            .split_once("://")
            .map(|(scheme, _)| scheme)
            .unwrap_or("");
        let name = nth_enabled_endpoint_name(state, tunnel_id, urls.len());
        if let Some(item) = RuntimePublicUrl::new(name, proto, &addr) {
            urls.push(item);
        }
        let info = state.provider_runtime.set_details(
            CPOLAR_PROVIDER_ID,
            tunnel_id,
            agent_runtime_details(&inspect_url, urls),
        );
        emit_status(app, info);
    }

    if state
        .provider_runtime
        .info(CPOLAR_PROVIDER_ID, tunnel_id)
        .status
        == TunnelRuntimeState::Starting
    {
        let info = state.provider_runtime.mark_status(
            CPOLAR_PROVIDER_ID,
            tunnel_id,
            TunnelRuntimeState::Running,
            "cpolar running",
        );
        emit_status(app, info);
    }
}

// 按出现顺序把公网地址映射到第 index 个已启用 endpoint 的名字（cpolar 日志不带名字）。
fn nth_enabled_endpoint_name(state: &AppState, tunnel_id: &str, index: usize) -> String {
    data::current_tunnel(state, tunnel_id)
        .ok()
        .and_then(|tunnel| {
            let endpoints: Vec<_> = enabled_endpoints(&tunnel).collect();
            if endpoints.len() == 1 {
                endpoints.first().map(|endpoint| endpoint.name.clone())
            } else {
                endpoints.get(index).map(|endpoint| endpoint.name.clone())
            }
        })
        .unwrap_or_default()
}

fn enabled_endpoints(tunnel: &CpolarTunnel) -> impl Iterator<Item = &CpolarEndpoint> {
    tunnel.endpoints.iter().filter(|endpoint| endpoint.enabled)
}

fn prepare_traffic_relay(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    tunnel: &mut CpolarTunnel,
) -> AppResult<()> {
    if !state.config.settings().traffic_stats_enabled {
        watchdog_relay::release(state, CPOLAR_PROVIDER_ID, id);
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
    let bindings = watchdog_relay::prepare(app, state, CPOLAR_PROVIDER_ID, id, endpoints)?;
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
    provider_log::emit_system(app, state, CPOLAR_PROVIDER_ID, tunnel_id, line);
}

fn emit_native_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: String,
) {
    provider_log::emit_native(app, state, CPOLAR_PROVIDER_ID, tunnel_id, line);
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
    use crate::providers::cpolar::domain::CpolarData;

    // cpolar 没有可用 API，成功靠日志里的 "Tunnel established at <addr>"。
    // 命中该行应把 Starting 确认为 Running 并记录公网地址；普通日志不改状态。
    #[test]
    fn established_log_confirms_running_and_captures_url() {
        let state = AppState::default();
        state
            .provider_runtime
            .begin_start(CPOLAR_PROVIDER_ID, "t1", "starting")
            .unwrap();
        let app = tauri::test::mock_builder()
            .manage(state.clone())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app_handle = app.handle().clone();

        // 普通日志：保持 Starting。
        capture_cpolar_public_url(&app_handle, &state, "t1", "INFO Reading configuration file");
        assert_eq!(
            state.provider_runtime.info(CPOLAR_PROVIDER_ID, "t1").status,
            TunnelRuntimeState::Starting
        );

        // established 行：确认 Running + 记录公网地址（类型无关）。
        capture_cpolar_public_url(
            &app_handle,
            &state,
            "t1",
            "INFO Tunnel established at https://abc.r3.cpolar.cn",
        );
        let info = state.provider_runtime.info(CPOLAR_PROVIDER_ID, "t1");
        assert_eq!(info.status, TunnelRuntimeState::Running);
        assert!(public_urls_from_details(&info.details)
            .iter()
            .any(|url| url.public_url == "https://abc.r3.cpolar.cn"));
    }

    #[test]
    fn established_logs_keep_multiple_urls_on_single_endpoint() {
        let state = AppState::default();
        state
            .config
            .update_provider_data::<CpolarData, _>(CPOLAR_PROVIDER_ID, |data| {
                data.tunnels.push(CpolarTunnel {
                    id: "t1".into(),
                    name: "edge".into(),
                    authtoken: "token".into(),
                    region: String::new(),
                    config_file: String::new(),
                    endpoints: vec![CpolarEndpoint {
                        id: "e1".into(),
                        name: "web".into(),
                        proto: "http".into(),
                        addr: "127.0.0.1:1420".into(),
                        hostname: String::new(),
                        remote_addr: String::new(),
                        enabled: true,
                    }],
                    created_at: 0,
                    updated_at: 0,
                });
                Ok(())
            })
            .unwrap();
        state
            .provider_runtime
            .begin_start(CPOLAR_PROVIDER_ID, "t1", "starting")
            .unwrap();
        let app = tauri::test::mock_builder()
            .manage(state.clone())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app_handle = app.handle().clone();

        capture_cpolar_public_url(
            &app_handle,
            &state,
            "t1",
            "INFO Tunnel established at http://abc.r3.cpolar.cn",
        );
        capture_cpolar_public_url(
            &app_handle,
            &state,
            "t1",
            "INFO Tunnel established at https://abc.r3.cpolar.cn",
        );

        let urls = public_urls_from_details(
            &state
                .provider_runtime
                .info(CPOLAR_PROVIDER_ID, "t1")
                .details,
        );
        assert_eq!(urls.len(), 2);
        assert!(urls.iter().all(|url| url.name == "web"));
    }

    #[test]
    fn cpolar_config_contains_enabled_tunnels_only() {
        let mut tunnel = CpolarTunnel::new("edge");
        tunnel.authtoken = "token".into();
        tunnel.endpoints = vec![
            CpolarEndpoint {
                name: "web".into(),
                proto: "http".into(),
                addr: "localhost:8080".into(),
                hostname: "app.example.com".into(),
                enabled: true,
                ..Default::default()
            },
            CpolarEndpoint {
                name: "off".into(),
                enabled: false,
                ..Default::default()
            },
        ];

        let text = serde_yaml_ng::to_string(&config(&tunnel)).unwrap();

        assert!(text.contains("authtoken: token"));
        assert!(text.contains("web:"));
        assert!(text.contains("hostname: app.example.com"));
        assert!(!text.contains("off:"));
    }

    #[test]
    fn cpolar_config_rejects_unknown_proto() {
        let mut tunnel = CpolarTunnel::new("edge");
        tunnel.authtoken = "token".into();
        tunnel.endpoints.push(CpolarEndpoint::default());
        tunnel.endpoints[0].proto = "udp".into();

        assert!(validate_tunnel(&tunnel).is_err());
    }

    #[test]
    fn cpolar_inspect_addr_reads_watchdog_snapshot_args() {
        let args = vec![
            "start-all".into(),
            "-config=/tmp/tunnelx/cpolar/configs/edge.yml".into(),
            "-inspect-addr=127.0.0.1:49152".into(),
        ];

        assert_eq!(
            cpolar_inspect_addr_from_args(&args).as_deref(),
            Some("127.0.0.1:49152")
        );

        let args = vec![
            "start-all".into(),
            "--inspect-addr".into(),
            "127.0.0.1:49153".into(),
        ];

        assert_eq!(
            cpolar_inspect_addr_from_args(&args).as_deref(),
            Some("127.0.0.1:49153")
        );
    }
}
