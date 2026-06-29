//! 由 Profile 生成 frpc 配置，并经平台级看门狗 sidecar 启停/守护多个 frpc，
//! 把看门狗上报的日志/状态事件推给前端。
use crate::domain::gen_id;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::providers::contract::{
    watchdog_exit_clean, TunnelRuntimeInfo, TunnelRuntimeState, TunnelRuntimeStatusEvent,
    FRP_PROVIDER_ID,
};
use crate::providers::frp::domain::{build_frpc_json, Profile, ProfileProxy, ProxyConfig};
use crate::providers::frp::paths as frp_paths;
use crate::providers::frp::runtime_state::FrpcStatus;
use crate::providers::frp::services::frpc_lifecycle::{
    cleanup_all_frpc_in_runtime_dir, cleanup_frpc_for_config, cleanup_orphan_frpc_in_runtime_dir,
    FrpcProcessProtection,
};
use crate::providers::frp::state::frp_state;
use crate::providers::runtime_public_url::{public_url_details, RuntimePublicUrl};
use crate::services::provider_log;
use crate::services::{process_watchdog, watchdog_relay};
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use reqwest::header::AUTHORIZATION;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};
use tunnelx_watchdog_protocol::{
    WatchdogEvent, WatchdogHttpHeader, WatchdogHttpRequest, WatchdogRelayProtocol, WatchdogRequest,
    WatchdogStartProcessRequest, WatchdogStopProcessRequest, WatchdogStopStrategy,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const ADMIN_HTTP_TIMEOUT: Duration = Duration::from_millis(800);
const ADMIN_PORT_RETRY_LIMIT: usize = 5;
const LOCAL_CHECK_TIMEOUT: Duration = Duration::from_millis(250);
const START_STATUS_PROBE_LIMIT: usize = 20;
const START_STATUS_PROBE_INTERVAL: Duration = Duration::from_millis(300);
const RUNTIME_DETAILS_REFRESH_LIMIT: usize = 8;
const RUNTIME_DETAILS_REFRESH_INTERVAL: Duration = Duration::from_millis(300);
const DEFAULT_WORK_CONN_POOL_COUNT: i32 = 4;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    profile_id: String,
    status: FrpcStatus,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigDirtyEvent {
    profile_id: String,
    needs_restart: bool,
}

#[derive(Clone)]
struct LocalTarget {
    proxy_name: String,
    host: String,
    port: u16,
    address: String,
}

#[derive(Clone)]
struct LocalAlternative {
    host: String,
    address: String,
}

/// 单条隧道的运行态详情，来自 frpc admin API `/api/status`。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatusInfo {
    name: String,
    status: String,
    kind: String,
    public_addr: String,
    public_addrs: Vec<String>,
    remote_port: Option<u16>,
    remote_addr: String,
    local_addr: String,
    err: String,
}

fn emit_runtime_status_event<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
    status: FrpcStatus,
    write_log: bool,
) {
    let info = frp_runtime_info(state, profile_id, status);
    let _ = app.emit(
        "frpc-status-changed",
        StatusEvent {
            profile_id: profile_id.to_string(),
            status,
        },
    );
    let _ = app.emit(
        "provider-tunnel-status-changed",
        TunnelRuntimeStatusEvent { info: info.clone() },
    );
    if write_log {
        emit_system_log_line(app, state, profile_id, info.message);
    }
    crate::refresh_connection_icon(app);
}

fn emit_status<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
    status: FrpcStatus,
) {
    emit_runtime_status_event(app, state, profile_id, status, true);
    if status.is_running() {
        spawn_runtime_details_refresh(app.clone(), state.clone(), profile_id.to_string());
    }
}

fn spawn_runtime_details_refresh<R: Runtime>(
    app: AppHandle<R>,
    state: AppState,
    profile_id: String,
) {
    std::thread::spawn(move || {
        for _ in 0..RUNTIME_DETAILS_REFRESH_LIMIT {
            std::thread::sleep(RUNTIME_DETAILS_REFRESH_INTERVAL);
            let status = frp_state(&state).runtime.status(&profile_id);
            if !status.is_running() {
                return;
            }
            if runtime_details_have_public_urls(&runtime_details(&state, &profile_id, status)) {
                emit_runtime_status_event(&app, &state, &profile_id, status, false);
                return;
            }
        }
    });
}

fn runtime_details_have_public_urls(details: &Value) -> bool {
    details
        .get("publicUrls")
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
}

fn emit_config_dirty<R: Runtime>(app: &AppHandle<R>, profile_id: &str, needs_restart: bool) {
    let _ = app.emit(
        "frpc-config-dirty",
        ConfigDirtyEvent {
            profile_id: profile_id.to_string(),
            needs_restart,
        },
    );
}

fn emit_system_log_line<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
    line: String,
) {
    provider_log::emit_system(app, state, FRP_PROVIDER_ID, profile_id, line);
}

fn emit_native_log_line<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
    line: String,
) {
    provider_log::emit_native(app, state, FRP_PROVIDER_ID, profile_id, line);
}

fn frp_runtime_info(state: &AppState, profile_id: &str, status: FrpcStatus) -> TunnelRuntimeInfo {
    let (pid, needs_restart) = {
        let frp = frp_state(state);
        let rt = frp.runtime.lock();
        rt.instances
            .get(profile_id)
            .map(|instance| (instance.pid, instance.needs_restart))
            .unwrap_or((None, false))
    };
    TunnelRuntimeInfo {
        provider_id: FRP_PROVIDER_ID.into(),
        tunnel_id: profile_id.to_string(),
        status: frp_runtime_state(status),
        pid,
        message: frp_runtime_message(status, needs_restart),
        details: runtime_details(state, profile_id, status),
    }
}

fn frp_runtime_state(status: FrpcStatus) -> TunnelRuntimeState {
    match status {
        FrpcStatus::Stopped => TunnelRuntimeState::Stopped,
        FrpcStatus::Starting => TunnelRuntimeState::Starting,
        FrpcStatus::Running => TunnelRuntimeState::Running,
        FrpcStatus::Warning => TunnelRuntimeState::Warning,
        FrpcStatus::Stopping => TunnelRuntimeState::Stopping,
        FrpcStatus::Errored => TunnelRuntimeState::Errored,
    }
}

fn frp_runtime_message(status: FrpcStatus, needs_restart: bool) -> String {
    if needs_restart {
        return "Configuration changed; restart required".into();
    }
    match status {
        FrpcStatus::Stopped => "frpc stopped",
        FrpcStatus::Starting => "frpc starting",
        FrpcStatus::Running => "frpc running",
        FrpcStatus::Warning => "frpc running with proxy warnings",
        FrpcStatus::Stopping => "frpc stopping",
        FrpcStatus::Errored => "frpc exited with error",
    }
    .into()
}

pub fn mark_process_restart_required<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
) {
    if frp_state(state)
        .runtime
        .mark_process_restart_required(profile_id)
    {
        emit_config_dirty(app, profile_id, true);
    }
}

fn is_attention_log(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains(" start error:")
        || lower.contains(" start error ")
        || lower.contains(" already exists")
        || lower.contains("[e]")
        || lower.contains(" error:")
}

fn is_internal_admin_api_log(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    ["/api/status", "/api/reload", "/api/stop"]
        .iter()
        .any(|path| lower.contains(path))
}

fn proxy_added_names(line: &str) -> Option<Vec<String>> {
    let marker = "proxy added:";
    let start = line.to_ascii_lowercase().find(marker)? + marker.len();
    let tail = line[start..].trim();
    let open = tail.find('[')?;
    let close = tail[open + 1..].find(']')? + open + 1;
    Some(
        tail[open + 1..close]
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    )
}

fn bracket_proxy_name(line: &str, suffix: &str) -> Option<String> {
    let suffix_index = line.to_ascii_lowercase().find(suffix)?;
    let before = &line[..suffix_index];
    let open = before.rfind('[')?;
    let close = before[open + 1..].find(']')? + open + 1;
    Some(before[open + 1..close].to_string())
}

fn already_exists_name(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let start = lower.find("proxy [")? + "proxy [".len();
    let close = line[start..].find(']')? + start;
    if lower[close..].contains("already exists") {
        Some(line[start..close].to_string())
    } else {
        None
    }
}

fn enabled_proxy_names(profile: &Profile) -> Vec<String> {
    profile
        .proxies
        .iter()
        .filter(|proxy| proxy.enabled)
        .map(|proxy| proxy.config.base().name.clone())
        .filter(|name| !name.is_empty())
        .collect()
}

/// 选出该 Profile 实际要用的 frpc 可执行文件（profile 指定优先，否则全局激活版本）。
fn resolve_exe<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile: &Profile,
) -> AppResult<PathBuf> {
    let version = profile
        .frpc_version
        .clone()
        .or_else(|| state.config.frp_settings().active_frpc_version)
        .ok_or(AppError::FrpcNotReady)?;
    let exe = frp_paths::frpc_exe(app, &version)?;
    if !exe.exists() {
        return Err(AppError::FrpcNotReady);
    }
    Ok(exe)
}

/// 连接前用 `frpc verify -c <cfg>` 预检配置：配置有误时尽早拦截并给出明确报错，
/// 而不必等进程起来后再从日志里翻。旧版本 frpc 没有 verify 子命令时跳过，避免误伤。
fn verify_config(exe: &Path, config: &Path) -> AppResult<()> {
    let mut cmd = Command::new(exe);
    cmd.arg("verify").arg("-c").arg(config);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    // frpc 起不来（极少见）：不在这里拦，交给后续真正启动去暴露真实错误。
    let Ok(output) = cmd.output() else {
        return Ok(());
    };
    if output.status.success() {
        return Ok(());
    }
    let mut detail = String::from_utf8_lossy(&output.stderr).into_owned();
    detail.push_str(&String::from_utf8_lossy(&output.stdout));
    // 旧版本 frpc 不认 verify 子命令（cobra 输出 "unknown command"）：这不是配置错误，放行。
    let lower = detail.to_ascii_lowercase();
    if lower.contains("unknown command") || lower.contains("unknown shorthand") {
        return Ok(());
    }
    let detail = detail.trim();
    Err(AppError::ConfigFormatError(if detail.is_empty() {
        "frpc verify failed".to_string()
    } else {
        detail.to_string()
    }))
}

#[derive(Debug, Clone, Copy)]
struct AdminPort {
    port: u16,
    auto_assigned: bool,
}

enum AdminPortCheck {
    Available,
    Retry,
}

/// 给运行期配置注入本地 admin webServer（仅 127.0.0.1），返回其端口用于停止时优雅断开。
/// 用户已自配 webServer 时沿用其端口；选不到空闲端口时返回 None（退化为信号停止）。
fn ensure_admin_port(profile: &mut Profile) -> Option<AdminPort> {
    if profile.server.web_server.port != 0 {
        return Some(AdminPort {
            port: profile.server.web_server.port,
            auto_assigned: false,
        });
    }
    let port = free_local_port()?;
    profile.server.web_server.addr = "127.0.0.1".into();
    profile.server.web_server.port = port;
    // 自动注入的本地 admin 口默认无鉴权 → 同机任何进程都能经 127.0.0.1:port 调
    // /api/config（读取 token 与各隧道密钥）、/api/stop（本地 DoS）。注入一对随机凭据把它锁上：
    // 仅写运行期配置、不回写用户 Profile，对用户透明（管理 API 只供本程序内部停止/重载/查状态）。
    profile.server.web_server.user = "tunnelx".into();
    profile.server.web_server.password = gen_id();
    Some(AdminPort {
        port,
        auto_assigned: true,
    })
}

/// 生成 frpc admin API 的 Basic Authorization 头。用户未配置 webServer 凭据时返回 None。
fn admin_auth_header(profile: &Profile) -> Option<String> {
    let user = &profile.server.web_server.user;
    let password = &profile.server.web_server.password;
    if user.is_empty() && password.is_empty() {
        return None;
    }
    Some(format!(
        "Basic {}",
        STANDARD.encode(format!("{user}:{password}"))
    ))
}

/// 向 OS 借一个空闲的本地端口（bind 到 127.0.0.1:0 再释放）。
fn free_local_port() -> Option<u16> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).ok()?;
    listener.local_addr().ok().map(|addr| addr.port())
}

fn local_tcp_port_available(port: u16) -> bool {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok()
}

fn check_admin_port_available(
    profile: &mut Profile,
    admin: Option<AdminPort>,
    attempt: usize,
) -> AppResult<AdminPortCheck> {
    let Some(endpoint) = admin else {
        return Ok(AdminPortCheck::Available);
    };
    if local_tcp_port_available(endpoint.port) {
        return Ok(AdminPortCheck::Available);
    }
    if endpoint.auto_assigned && attempt + 1 < ADMIN_PORT_RETRY_LIMIT {
        profile.server.web_server.port = 0;
        return Ok(AdminPortCheck::Retry);
    }
    Err(AppError::PortInUse(endpoint.port))
}

fn write_verified_runtime_config(
    exe: &Path,
    cfg_path: &Path,
    profile: &mut Profile,
) -> AppResult<(Option<u16>, Option<String>)> {
    for attempt in 0..ADMIN_PORT_RETRY_LIMIT {
        let admin = ensure_admin_port(profile);
        let admin_port = admin.map(|endpoint| endpoint.port);
        let admin_auth = admin_auth_header(profile);

        if matches!(
            check_admin_port_available(profile, admin, attempt)?,
            AdminPortCheck::Retry
        ) {
            continue;
        }

        let json = build_frpc_json(profile)?;
        paths::write_secret_file(cfg_path, json.as_bytes())?;
        verify_config(exe, cfg_path)?;

        if matches!(
            check_admin_port_available(profile, admin, attempt)?,
            AdminPortCheck::Retry
        ) {
            continue;
        }

        return Ok((admin_port, admin_auth));
    }

    Err(AppError::Msg(
        "failed to allocate a local admin port; please retry".into(),
    ))
}

/// 经 frpc admin API `POST /api/stop` 触发优雅关闭：frpc 主动断开与 frps 的控制连接，
/// frps 随即注销隧道（不必等心跳超时）。返回是否已被接受(2xx)。
///
/// 放在主程序里做（而不只在看门狗 sidecar 里）：主程序可以在停止流程里立即执行，
/// 保证退出时先走优雅断开，再由 sidecar 做兜底。
fn graceful_stop_port(port: u16, auth: Option<&str>) -> bool {
    let Some(client) = admin_http_client() else {
        return false;
    };
    let mut request = client
        .post(format!("http://127.0.0.1:{port}/api/stop"))
        .timeout(Duration::from_millis(600))
        .body(Vec::<u8>::new());
    if let Some(value) = auth {
        request = request.header(AUTHORIZATION, value);
    }
    matches!(request.send(), Ok(response) if response.status().is_success())
}

/// 查询 frpc admin API（仅 127.0.0.1）。2xx 时返回响应体，否则 None。
fn admin_get(port: u16, auth: Option<&str>, path: &str) -> Option<String> {
    let client = admin_http_client()?;
    let mut request = client.get(format!("http://127.0.0.1:{port}{path}"));
    if let Some(value) = auth {
        request = request.header(AUTHORIZATION, value);
    }
    let response = request.send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.text().ok()
}

/// 进程内共享的 admin API 客户端：只连本地端口，显式禁用系统代理，
/// 避免 HTTP(S)_PROXY 把本地管理请求劫持到外部。用阻塞客户端以保留同步调用点
/// （stop / stop_all / 热重载）的签名，不引入 runtime 上下文依赖。
fn admin_http_client() -> Option<&'static reqwest::blocking::Client> {
    static CLIENT: OnceLock<Option<reqwest::blocking::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .no_proxy()
                .connect_timeout(ADMIN_HTTP_TIMEOUT)
                .timeout(ADMIN_HTTP_TIMEOUT)
                .build()
                .ok()
        })
        .as_ref()
}

fn admin_reload_port(port: u16, auth: Option<&str>) -> bool {
    admin_get(port, auth, "/api/reload").is_some()
}

fn first_remote_addr(raw: &str) -> &str {
    raw.split([',', ';'])
        .map(str::trim)
        .find(|part| !part.is_empty())
        .unwrap_or_default()
}

fn remote_addr_port(raw: &str) -> Option<u16> {
    let first = first_remote_addr(raw);
    if first.is_empty() {
        return None;
    }

    let rest = if let Some(rest) = first.strip_prefix("http://") {
        rest
    } else if let Some(rest) = first.strip_prefix("https://") {
        rest
    } else {
        first
    };

    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .trim();

    if authority.is_empty() {
        return None;
    }

    if let Some(rest) = authority.strip_prefix('[') {
        if let Some((_host, tail)) = rest.split_once(']') {
            return tail
                .strip_prefix(':')
                .and_then(|value| value.parse::<u16>().ok());
        }
    }

    if authority.matches(':').count() > 1 {
        return None;
    }
    authority
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
}

fn configured_or_status_port(configured: Option<u16>, status: &ProxyStatusInfo) -> Option<u16> {
    configured
        .or(status.remote_port)
        .or_else(|| remote_addr_port(&status.remote_addr))
}

fn tcp_udp_public_addr(
    server_addr: &str,
    configured_port: Option<u16>,
    status: &ProxyStatusInfo,
) -> String {
    let Some(port) = configured_or_status_port(configured_port, status) else {
        return String::new();
    };
    format!("{}:{port}", format_host_for_addr(server_addr))
}

fn format_host_for_addr(host: &str) -> String {
    if host.parse::<IpAddr>().is_ok() && host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn scheme_for_public_kind(kind: &str) -> Option<&'static str> {
    match kind.to_ascii_lowercase().as_str() {
        "http" => Some("http"),
        "https" => Some("https"),
        _ => None,
    }
}

fn normalize_public_addr(raw: &str, scheme: Option<&str>) -> String {
    let value = raw.trim();
    if value.is_empty() {
        return String::new();
    }
    if value.contains("://") {
        value.to_string()
    } else if let Some(scheme) = scheme {
        format!("{scheme}://{value}")
    } else {
        value.to_string()
    }
}

fn push_public_addr(out: &mut Vec<String>, raw: &str, scheme: Option<&str>) {
    let value = normalize_public_addr(raw, scheme);
    if value.is_empty() || out.iter().any(|item| item == &value) {
        return;
    }
    out.push(value);
}

fn push_public_addr_value(out: &mut Vec<String>, value: &Value, scheme: Option<&str>) {
    match value {
        Value::String(raw) => {
            for part in raw.split([',', ';']) {
                push_public_addr(out, part, scheme);
            }
        }
        Value::Array(items) => {
            for item in items {
                push_public_addr_value(out, item, scheme);
            }
        }
        _ => {}
    }
}

fn push_public_addr_fields(
    out: &mut Vec<String>,
    value: &Value,
    names: &[&str],
    scheme: Option<&str>,
) {
    for name in names {
        if let Some(field) = value.get(*name) {
            push_public_addr_value(out, field, scheme);
        }
    }
}

fn status_public_addrs(kind: &str, value: &Value) -> Vec<String> {
    let scheme = scheme_for_public_kind(kind);
    let mut out = Vec::new();
    push_public_addr_fields(
        &mut out,
        value,
        &[
            "publicAddrs",
            "public_addrs",
            "publicUrls",
            "public_urls",
            "publicAddr",
            "public_addr",
            "publicUrl",
            "public_url",
        ],
        scheme,
    );
    push_public_addr_fields(
        &mut out,
        value,
        &[
            "remoteAddr",
            "remote_addr",
            "remoteAddress",
            "remote_address",
            "remote",
        ],
        scheme,
    );
    push_public_addr_fields(
        &mut out,
        value,
        &[
            "customDomains",
            "custom_domains",
            "customDomain",
            "custom_domain",
        ],
        scheme,
    );
    out
}

fn profile_public_addrs_from_status(
    profile: &Profile,
    proxy: &ProfileProxy,
    status: &ProxyStatusInfo,
) -> Vec<String> {
    if status.status != "running" {
        return Vec::new();
    }

    if !status.public_addrs.is_empty() {
        return status.public_addrs.clone();
    }

    let remote_addr = first_remote_addr(&status.remote_addr);
    if !remote_addr.is_empty() {
        return vec![remote_addr.to_string()];
    }

    let fallback = match &proxy.config {
        ProxyConfig::Tcp(tcp) => {
            tcp_udp_public_addr(&profile.server.server_addr, tcp.remote_port, status)
        }
        ProxyConfig::Udp(udp) => {
            tcp_udp_public_addr(&profile.server.server_addr, udp.remote_port, status)
        }
        ProxyConfig::Http(_)
        | ProxyConfig::Https(_)
        | ProxyConfig::Tcpmux(_)
        | ProxyConfig::Stcp(_)
        | ProxyConfig::Sudp(_)
        | ProxyConfig::Xtcp(_) => String::new(),
    };
    if fallback.is_empty() {
        Vec::new()
    } else {
        vec![fallback]
    }
}

fn decorate_public_addrs(profile: &Profile, statuses: &mut [ProxyStatusInfo]) {
    for status in statuses {
        if status.status != "running" {
            status.public_addr.clear();
            status.public_addrs.clear();
            continue;
        }
        let Some(proxy) = profile
            .proxies
            .iter()
            .find(|proxy| proxy.config.base().name == status.name)
        else {
            status.public_addr = first_remote_addr(&status.remote_addr).to_string();
            status.public_addrs = if status.public_addr.is_empty() {
                Vec::new()
            } else {
                vec![status.public_addr.clone()]
            };
            continue;
        };
        status.public_addrs = profile_public_addrs_from_status(profile, proxy, status);
        status.public_addr = status.public_addrs.first().cloned().unwrap_or_default();
    }
}

fn number_field_u16(value: &Value, names: &[&str]) -> Option<u16> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
    })
}

fn string_field(value: &Value, names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .unwrap_or_default()
        .to_string()
}

fn status_item(kind: &str, value: &Value) -> Option<ProxyStatusInfo> {
    let name = string_field(value, &["name", "proxyName", "proxy_name"]);
    if name.is_empty() {
        return None;
    }
    let item_kind = string_field(value, &["type", "kind", "proxyType", "proxy_type"]);
    let effective_kind = if item_kind.is_empty() {
        kind
    } else {
        &item_kind
    };
    let status = string_field(value, &["status", "statusText", "status_text"]).to_ascii_lowercase();
    let public_addrs = if status == "running" {
        status_public_addrs(effective_kind, value)
    } else {
        Vec::new()
    };
    Some(ProxyStatusInfo {
        name,
        status,
        kind: if item_kind.is_empty() {
            kind.to_string()
        } else {
            item_kind
        },
        public_addr: public_addrs.first().cloned().unwrap_or_default(),
        public_addrs,
        remote_port: number_field_u16(value, &["remotePort", "remote_port"]),
        remote_addr: string_field(
            value,
            &[
                "remoteAddr",
                "remote_addr",
                "remoteAddress",
                "remote_address",
                "remote",
            ],
        ),
        local_addr: string_field(
            value,
            &[
                "localAddr",
                "local_addr",
                "localAddress",
                "local_address",
                "local",
            ],
        ),
        err: string_field(
            value,
            &["err", "error", "errMsg", "err_msg", "errorMsg", "error_msg"],
        ),
    })
}

fn collect_status_items(kind: &str, value: &Value, out: &mut Vec<ProxyStatusInfo>) {
    match value {
        Value::Array(items) => {
            out.extend(items.iter().filter_map(|item| status_item(kind, item)));
        }
        Value::Object(obj) => {
            if let Some(items) = obj.get("proxies").and_then(Value::as_array) {
                out.extend(items.iter().filter_map(|item| status_item(kind, item)));
            } else if let Some(item) = status_item(kind, value) {
                out.push(item);
            }
        }
        _ => {}
    }
}

fn parse_proxy_status(body: &str) -> Option<Vec<ProxyStatusInfo>> {
    let value: Value = serde_json::from_str(body).ok()?;
    let mut out = Vec::new();
    let mut recognized = false;
    match &value {
        Value::Array(_) => {
            recognized = true;
            collect_status_items("", &value, &mut out);
        }
        Value::Object(obj) => {
            recognized = true;
            if let Some(items) = obj.get("proxies") {
                collect_status_items("", items, &mut out);
            }
            for kind in [
                "tcp", "udp", "http", "https", "tcpmux", "stcp", "sudp", "xtcp",
            ] {
                if let Some(items) = obj.get(kind) {
                    collect_status_items(kind, items, &mut out);
                }
            }
        }
        _ => {}
    }

    if !recognized {
        return None;
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    out.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);
    Some(out)
}

fn proxy_status_snapshot(state: &AppState, profile_id: &str) -> Option<Vec<ProxyStatusInfo>> {
    let admin = {
        let frp = frp_state(state);
        let rt = frp.runtime.lock();
        let inst = rt.instances.get(profile_id)?;
        if !matches!(
            inst.status,
            FrpcStatus::Starting | FrpcStatus::Running | FrpcStatus::Warning
        ) {
            return None;
        }
        inst.admin_endpoint()
    };
    let (port, auth) = admin?;
    let body = admin_get(port, auth.as_deref(), "/api/status")?;
    let mut statuses = parse_proxy_status(&body)?;
    if let Ok(profile) = state.config.get_profile(profile_id) {
        decorate_public_addrs(&profile, &mut statuses);
    } else {
        for status in &mut statuses {
            if !status.public_addrs.is_empty() {
                status.public_addr = status.public_addrs.first().cloned().unwrap_or_default();
                continue;
            }
            let public_addr = first_remote_addr(&status.remote_addr).to_string();
            status.public_addr = public_addr.clone();
            status.public_addrs = if public_addr.is_empty() {
                Vec::new()
            } else {
                vec![public_addr]
            };
        }
    }
    Some(statuses)
}

fn runtime_public_urls(statuses: &[ProxyStatusInfo]) -> Vec<RuntimePublicUrl> {
    statuses
        .iter()
        .filter(|status| status.status == "running")
        .flat_map(|status| {
            let addrs = if status.public_addrs.is_empty() && !status.public_addr.is_empty() {
                vec![status.public_addr.clone()]
            } else {
                status.public_addrs.clone()
            };
            addrs
                .into_iter()
                .filter_map(|addr| RuntimePublicUrl::new(&status.name, &status.kind, addr))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdminStatusProbe {
    Pending,
    Running,
    Warning,
}

fn proxy_status_is_warning(status: &ProxyStatusInfo) -> bool {
    !status.err.trim().is_empty()
        || status
            .status
            .split_whitespace()
            .any(|part| matches!(part, "error" | "failed" | "fail" | "closed"))
        || status.status.contains("error")
        || status.status.contains("fail")
}

fn admin_status_probe(profile: &Profile, statuses: &[ProxyStatusInfo]) -> AdminStatusProbe {
    let enabled = enabled_proxy_names(profile);
    if enabled.is_empty() {
        return AdminStatusProbe::Running;
    }

    let mut all_running = true;
    for name in enabled {
        let Some(status) = statuses.iter().find(|item| item.name == name) else {
            all_running = false;
            continue;
        };
        if status.status == "running" {
            continue;
        }
        if proxy_status_is_warning(status) {
            return AdminStatusProbe::Warning;
        }
        all_running = false;
    }

    if all_running {
        AdminStatusProbe::Running
    } else {
        AdminStatusProbe::Pending
    }
}

fn apply_admin_status_probe<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
) -> bool {
    let current = frp_state(state).runtime.status(profile_id);
    if !matches!(current, FrpcStatus::Starting | FrpcStatus::Warning) {
        return true;
    }
    let Ok(profile) = state.config.get_profile(profile_id) else {
        return true;
    };
    let Some(statuses) = proxy_status_snapshot(state, profile_id) else {
        return false;
    };
    let next = match admin_status_probe(&profile, &statuses) {
        AdminStatusProbe::Pending => return false,
        AdminStatusProbe::Running => FrpcStatus::Running,
        AdminStatusProbe::Warning => FrpcStatus::Warning,
    };
    {
        let frp = frp_state(state);
        let mut rt = frp.runtime.lock();
        let Some(inst) = rt.instances.get_mut(profile_id) else {
            return true;
        };
        if !matches!(inst.status, FrpcStatus::Starting | FrpcStatus::Warning) {
            return true;
        }
        inst.status = next;
    }
    emit_status(app, state, profile_id, next);
    true
}

fn spawn_start_status_probe<R: Runtime>(app: AppHandle<R>, state: AppState, profile_id: String) {
    std::thread::spawn(move || {
        for _ in 0..START_STATUS_PROBE_LIMIT {
            if apply_admin_status_probe(&app, &state, &profile_id) {
                return;
            }
            std::thread::sleep(START_STATUS_PROBE_INTERVAL);
        }
    });
}

pub(crate) fn runtime_details(state: &AppState, profile_id: &str, status: FrpcStatus) -> Value {
    if !status.is_running() {
        return public_url_details(Vec::new());
    }
    let public_urls = proxy_status_snapshot(state, profile_id)
        .map(|statuses| runtime_public_urls(&statuses))
        .unwrap_or_default();
    public_url_details(public_urls)
}

pub async fn proxy_status(
    state: &AppState,
    profile_id: &str,
) -> AppResult<Option<Vec<ProxyStatusInfo>>> {
    // proxy_status_snapshot makes a synchronous HTTP call to the frpc admin API; run it on the
    // blocking pool so polling it doesn't stall an async runtime worker.
    let state = state.clone();
    let profile_id = profile_id.to_string();
    tauri::async_runtime::spawn_blocking(move || proxy_status_snapshot(&state, &profile_id))
        .await
        .map_err(|error| AppError::Msg(format!("proxy status task failed: {error}")))
}

pub fn hot_reload<R: Runtime>(app: &AppHandle<R>, state: &AppState, profile_id: &str) -> bool {
    let frp = frp_state(state);
    let original_profile = match state.config.get_profile(profile_id) {
        Ok(profile) => profile,
        Err(_) => {
            if frp.runtime.mark_hot_reload_failed(profile_id) {
                emit_config_dirty(app, profile_id, true);
            }
            return false;
        }
    };
    let mut profile = original_profile.clone();
    if validate_enabled_proxy_names(&profile).is_err() {
        if frp.runtime.mark_hot_reload_failed(profile_id) {
            emit_config_dirty(app, profile_id, true);
        }
        return false;
    }
    apply_runtime_transport_defaults(&mut profile);
    if state.config.settings().traffic_stats_enabled {
        if frp.runtime.mark_hot_reload_failed(profile_id) {
            emit_config_dirty(app, profile_id, true);
        }
        return false;
    }
    let cfg_path = match frp_paths::runtime_config(app, profile_id) {
        Ok(path) => path,
        Err(_) => {
            if frp.runtime.mark_hot_reload_failed(profile_id) {
                emit_config_dirty(app, profile_id, true);
            }
            return false;
        }
    };
    let write_result = (|| -> AppResult<()> {
        if let Some(parent) = cfg_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = build_frpc_json(&profile)?;
        paths::write_secret_file(&cfg_path, json.as_bytes())
    })();
    if write_result.is_err() {
        if frp.runtime.mark_hot_reload_failed(profile_id) {
            emit_config_dirty(app, profile_id, true);
        }
        return false;
    }

    let admin = {
        let rt = frp.runtime.lock();
        let Some(inst) = rt.instances.get(profile_id) else {
            return false;
        };
        if !inst.status.is_running() {
            return true;
        }
        inst.admin_endpoint()
    };
    let Some((port, auth)) = admin else {
        if frp.runtime.mark_hot_reload_failed(profile_id) {
            emit_config_dirty(app, profile_id, true);
        }
        return false;
    };

    if admin_reload_port(port, auth.as_deref()) {
        if let Some(needs_restart) = frp.runtime.mark_hot_reload_succeeded(profile_id) {
            emit_config_dirty(app, profile_id, needs_restart);
        }
        spawn_local_checks(
            app.clone(),
            state.clone(),
            profile_id.to_string(),
            original_profile.proxies,
        );
        true
    } else {
        if frp.runtime.mark_hot_reload_failed(profile_id) {
            emit_config_dirty(app, profile_id, true);
        }
        false
    }
}

fn normalize_local_target_host(value: &str) -> String {
    let trimmed = value.trim().trim_matches(['[', ']']);
    if trimmed.is_empty() || matches!(trimmed, "0.0.0.0" | "::") {
        Ipv4Addr::LOCALHOST.to_string()
    } else {
        trimmed.to_string()
    }
}

fn local_address(host: &str, port: u16) -> String {
    format!("{}:{port}", format_host_for_addr(host))
}

fn local_target(proxy: &ProfileProxy) -> Option<LocalTarget> {
    if !proxy.enabled || proxy.config.base().plugin.is_some() {
        return None;
    }
    if matches!(proxy.config, ProxyConfig::Udp(_) | ProxyConfig::Sudp(_)) {
        return None;
    }
    let base = proxy.config.base();
    let port = base.local_port?;
    let host = normalize_local_target_host(&base.local_ip);
    Some(LocalTarget {
        proxy_name: base.name.clone(),
        address: local_address(&host, port),
        host,
        port,
    })
}

fn local_tcp_available(host: &str, port: u16) -> bool {
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, LOCAL_CHECK_TIMEOUT).is_ok())
}

fn loopback_alternative(target: &LocalTarget) -> Option<LocalAlternative> {
    let host = target.host.trim_matches(['[', ']']);
    let alternative_host = match host {
        "127.0.0.1" => Ipv6Addr::LOCALHOST.to_string(),
        "::1" => Ipv4Addr::LOCALHOST.to_string(),
        _ => return None,
    };
    if !local_tcp_available(&alternative_host, target.port) {
        return None;
    }
    Some(LocalAlternative {
        address: local_address(&alternative_host, target.port),
        host: alternative_host,
    })
}

fn format_local_check_log(target: &LocalTarget, alternative: Option<&LocalAlternative>) -> String {
    if let Some(alternative) = alternative {
        format!(
            "self-check {} TCP target {} is not listening; {} is reachable, set Local IP to {} or make the service listen on {}",
            target.proxy_name, target.address, alternative.address, alternative.host, target.address
        )
    } else {
        format!(
            "self-check {} TCP target {} is not listening; requests may fail, make sure the local service is running",
            target.proxy_name, target.address
        )
    }
}

fn spawn_local_checks<R: Runtime>(
    app: AppHandle<R>,
    state: AppState,
    profile_id: String,
    proxies: Vec<ProfileProxy>,
) {
    let targets: Vec<LocalTarget> = proxies.iter().filter_map(local_target).collect();
    if targets.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for target in targets {
            if local_tcp_available(&target.host, target.port) {
                continue;
            }
            let alternative = loopback_alternative(&target);
            let line = format_local_check_log(&target, alternative.as_ref());
            emit_system_log_line(&app, &state, &profile_id, line);
        }
    });
}

fn mark_start_failed<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
    error: &AppError,
) {
    let frp = frp_state(state);
    watchdog_relay::release(state, FRP_PROVIDER_ID, profile_id);
    emit_system_log_line(app, state, profile_id, format!("start failed: {error}"));
    {
        let mut rt = frp.runtime.lock();
        let inst = rt.instance_mut(profile_id);
        inst.status = FrpcStatus::Errored;
        inst.config_path = None;
        inst.pid = None;
        inst.admin_port = None;
        inst.admin_auth = None;
        inst.needs_restart = false;
        inst.process_restart_required = false;
        inst.proxy_running.clear();
        inst.proxy_warning.clear();
    }
    emit_status(app, state, profile_id, FrpcStatus::Errored);
}

fn prepare_traffic_profile<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
    profile: &mut Profile,
) -> AppResult<()> {
    if !state.config.settings().traffic_stats_enabled {
        watchdog_relay::release(state, FRP_PROVIDER_ID, profile_id);
        return Ok(());
    }
    let endpoints = profile
        .proxies
        .iter()
        .filter(|proxy| proxy.enabled)
        .filter_map(frp_relay_endpoint)
        .collect::<Vec<_>>();
    let bindings = watchdog_relay::prepare(app, state, FRP_PROVIDER_ID, profile_id, endpoints)?;
    for binding in bindings {
        if let Some(proxy) = profile
            .proxies
            .iter_mut()
            .find(|proxy| proxy.config.base().name == binding.endpoint_id)
        {
            let base = proxy.config.base_mut();
            base.local_ip = binding.relay_host;
            base.local_port = Some(binding.relay_port);
        }
    }
    Ok(())
}

fn frp_relay_endpoint(proxy: &ProfileProxy) -> Option<watchdog_relay::RelayEndpointPlan> {
    let protocol = frp_relay_protocol(&proxy.config)?;
    let base = proxy.config.base();
    if base.plugin.is_some() {
        return None;
    }
    let port = base.local_port.filter(|port| *port > 0)?;
    let host = normalize_local_target_host(&base.local_ip);
    Some(watchdog_relay::RelayEndpointPlan {
        endpoint_id: base.name.clone(),
        endpoint_name: base.name.clone(),
        endpoint_type: proxy_config_type(&proxy.config).into(),
        protocol,
        target: format!("{host}:{port}"),
    })
}

fn frp_relay_protocol(config: &ProxyConfig) -> Option<WatchdogRelayProtocol> {
    if config.base().plugin.is_some() {
        return None;
    }
    match config {
        ProxyConfig::Tcp(_)
        | ProxyConfig::Http(_)
        | ProxyConfig::Https(_)
        | ProxyConfig::Tcpmux(_)
        | ProxyConfig::Stcp(_)
        | ProxyConfig::Xtcp(_) => Some(WatchdogRelayProtocol::Tcp),
        ProxyConfig::Udp(_) | ProxyConfig::Sudp(_) => Some(WatchdogRelayProtocol::Udp),
    }
}

fn proxy_config_type(config: &ProxyConfig) -> &'static str {
    match config {
        ProxyConfig::Tcp(_) => "tcp",
        ProxyConfig::Udp(_) => "udp",
        ProxyConfig::Http(_) => "http",
        ProxyConfig::Https(_) => "https",
        ProxyConfig::Tcpmux(_) => "tcpmux",
        ProxyConfig::Stcp(_) => "stcp",
        ProxyConfig::Sudp(_) => "sudp",
        ProxyConfig::Xtcp(_) => "xtcp",
    }
}

fn validate_enabled_proxy_names(profile: &Profile) -> AppResult<()> {
    let mut names = HashSet::new();
    for proxy in profile.proxies.iter().filter(|proxy| proxy.enabled) {
        let name = proxy.config.base().name.trim();
        if name.is_empty() {
            return Err(AppError::ConfigFormatError(
                "enabled tunnel name cannot be empty".into(),
            ));
        }
        if !names.insert(name.to_string()) {
            return Err(AppError::ConfigFormatError(format!(
                "duplicate tunnel name: {name}"
            )));
        }
    }
    Ok(())
}

fn uses_work_connections(proxy: &ProfileProxy) -> bool {
    proxy.enabled
        && matches!(
            proxy.config,
            ProxyConfig::Tcp(_)
                | ProxyConfig::Http(_)
                | ProxyConfig::Https(_)
                | ProxyConfig::Tcpmux(_)
        )
}

fn tcp_transport_uses_work_pool(profile: &Profile) -> bool {
    let protocol = profile.server.transport.protocol.trim();
    protocol.is_empty() || protocol.eq_ignore_ascii_case("tcp")
}

fn should_apply_default_pool_count(profile: &Profile) -> bool {
    profile.server.transport.pool_count.is_none()
        && tcp_transport_uses_work_pool(profile)
        && profile.proxies.iter().any(uses_work_connections)
}

fn apply_runtime_transport_defaults(profile: &mut Profile) {
    if should_apply_default_pool_count(profile) {
        profile.server.transport.pool_count = Some(DEFAULT_WORK_CONN_POOL_COUNT);
    }
}

fn frpc_process_protection(state: &AppState) -> FrpcProcessProtection {
    let mut protection = FrpcProcessProtection::default();
    protection.protect_pid(std::process::id());
    if let Some(pid) = state.process_watchdog.pid() {
        protection.protect_pid(pid);
    }
    let frp = frp_state(state);
    let rt = frp.runtime.lock();
    for inst in rt.instances.values() {
        if let Some(pid) = inst.pid {
            protection.protect_pid(pid);
        }
        if inst.status.is_active() {
            if let Some(config_path) = inst.config_path.as_ref() {
                protection.protect_config_path(config_path);
            }
        }
    }
    protection
}

pub fn cleanup_orphan_frpc_processes<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
) -> AppResult<usize> {
    let runtime_dir = frp_paths::runtime_dir(app)?;
    let killed = cleanup_orphan_frpc_in_runtime_dir(&runtime_dir, &frpc_process_protection(state))?;
    Ok(killed.len())
}

fn cleanup_all_runtime_frpc<R: Runtime>(app: &AppHandle<R>) -> AppResult<usize> {
    let killed = frp_paths::runtime_dir(app)
        .and_then(|runtime_dir| cleanup_all_frpc_in_runtime_dir(&runtime_dir))?;
    if !killed.is_empty() {
        crate::diag::info(
            &crate::diag::provider_scope(FRP_PROVIDER_ID),
            format!(
                "cleaned {} orphan frpc process(es) from runtime directory",
                killed.len()
            ),
        );
    }
    Ok(killed.len())
}

pub fn start<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
) -> AppResult<FrpcStatus> {
    let frp = frp_state(state);
    let mut profile = state.config.get_profile(profile_id)?;
    let exe = resolve_exe(app, state, &profile)?;
    validate_enabled_proxy_names(&profile)?;
    apply_runtime_transport_defaults(&mut profile);

    {
        let mut rt = frp.runtime.lock();
        if let Some(inst) = rt.instances.get(profile_id) {
            if inst.status.is_active() {
                return Err(AppError::AlreadyRunning);
            }
        }
        let inst = rt.instance_mut(profile_id);
        inst.status = FrpcStatus::Starting;
        inst.config_path = None;
        inst.needs_restart = false;
        inst.process_restart_required = false;
        inst.pid = None;
        inst.admin_port = None;
        inst.admin_auth = None;
        inst.proxy_running.clear();
        inst.proxy_warning.clear();
    }
    emit_status(app, state, profile_id, FrpcStatus::Starting);
    emit_config_dirty(app, profile_id, false);

    let result = (|| -> AppResult<()> {
        prepare_traffic_profile(app, state, profile_id, &mut profile)?;

        // 写入运行时配置文件。这里也会为运行期注入一个仅监听 127.0.0.1 的 frpc admin
        // 端口，并在写配置/verify 后再次确认端口可绑定；自动端口若被抢占则重选重写。
        let cfg_path = frp_paths::runtime_config(app, profile_id)?;
        if let Some(parent) = cfg_path.parent() {
            fs::create_dir_all(parent)?;
        }
        cleanup_orphan_frpc_processes(app, state)?;
        let (admin_port, admin_auth) =
            write_verified_runtime_config(&exe, &cfg_path, &mut profile)?;

        // 确保平台级看门狗在运行：已存在则复用，不存在才创建。
        process_watchdog::ensure(app, state)?;

        {
            let mut rt = frp.runtime.lock();
            let inst = rt.instance_mut(profile_id);
            inst.config_path = Some(cfg_path.clone());
            inst.admin_port = admin_port;
            inst.admin_auth = admin_auth.clone();
        }

        process_watchdog::send(
            app,
            state,
            WatchdogRequest::StartProcess(WatchdogStartProcessRequest {
                provider_id: FRP_PROVIDER_ID.into(),
                tunnel_id: profile_id.to_string(),
                program: exe.to_string_lossy().into_owned(),
                args: vec!["-c".into(), cfg_path.to_string_lossy().into_owned()],
                env: Vec::new(),
                stop_strategy: admin_port
                    .map(|port| Box::new(watchdog_admin_stop_strategy(port, admin_auth))),
                cleanup: None,
            }),
        )?;

        Ok(())
    })();

    if let Err(error) = result {
        mark_start_failed(app, state, profile_id, &error);
        return Err(error);
    }

    // 真正的 Running 由看门狗的 `started` 事件驱动（见 spawn_event_reader）。
    Ok(FrpcStatus::Starting)
}

fn watchdog_admin_stop_strategy(admin_port: u16, auth: Option<String>) -> WatchdogStopStrategy {
    let headers = auth
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            vec![WatchdogHttpHeader {
                name: "Authorization".into(),
                value,
            }]
        })
        .unwrap_or_default();

    WatchdogStopStrategy::Http {
        request: WatchdogHttpRequest {
            method: "POST".into(),
            url: format!("http://127.0.0.1:{admin_port}/api/stop"),
            headers,
            body: None,
            accepted_statuses: Vec::new(),
            json_expectation: None,
            timeout_ms: Some(800),
        },
        grace_ms: Some(2000),
    }
}

fn mark_watchdog_spawned(state: &AppState, id: &str, pid: u32) -> bool {
    let frp = frp_state(state);
    let mut rt = frp.runtime.lock();
    if !rt.instances.contains_key(id) && state.config.get_profile(id).is_err() {
        return false;
    }
    let inst = rt.instance_mut(id);
    if !matches!(inst.status, FrpcStatus::Running | FrpcStatus::Warning) {
        inst.status = FrpcStatus::Starting;
    }
    inst.pid = Some(pid);
    true
}

fn mark_watchdog_snapshot(state: &AppState, id: &str, pid: u32) -> bool {
    let frp = frp_state(state);
    let mut rt = frp.runtime.lock();
    if !rt.instances.contains_key(id) && state.config.get_profile(id).is_err() {
        return false;
    }
    let inst = rt.instance_mut(id);
    inst.status = FrpcStatus::Running;
    inst.pid = Some(pid);
    true
}

fn mark_watchdog_exited(state: &AppState, id: &str, clean_exit: bool) -> FrpcStatus {
    let frp = frp_state(state);
    watchdog_relay::clear_local(state, FRP_PROVIDER_ID, id);
    let status = if clean_exit {
        FrpcStatus::Stopped
    } else {
        FrpcStatus::Errored
    };
    {
        let mut rt = frp.runtime.lock();
        if let Some(inst) = rt.instances.get_mut(id) {
            inst.status = status;
            inst.needs_restart = false;
            inst.process_restart_required = false;
            inst.pid = None;
        }
    }
    status
}

fn mark_watchdog_error(state: &AppState, id: &str, message: &str) -> String {
    let frp = frp_state(state);
    watchdog_relay::clear_local(state, FRP_PROVIDER_ID, id);
    let line = provider_log::watchdog_error_line(message);
    {
        let mut rt = frp.runtime.lock();
        if let Some(inst) = rt.instances.get_mut(id) {
            inst.status = FrpcStatus::Errored;
            inst.pid = None;
        }
    }
    line
}

fn reconcile_watchdog_eof(state: &AppState) -> Vec<String> {
    let frp = frp_state(state);
    let reconciled: Vec<String> = {
        let mut rt = frp.runtime.lock();
        rt.instances
            .iter_mut()
            .filter(|(_, inst)| inst.status.is_active())
            .map(|(id, inst)| {
                inst.status = FrpcStatus::Errored;
                inst.pid = None;
                id.clone()
            })
            .collect()
    };
    for id in &reconciled {
        watchdog_relay::clear_local(state, FRP_PROVIDER_ID, id);
    }
    reconciled
}

/// 处理一行 frpc 日志并据此更新隧道/代理状态。实时日志与看门狗重连时回放的
/// `recent_logs` 共用同一套逻辑，保证重连恢复后的状态与"日志逐行喂进来"完全一致。
fn handle_frpc_log_line<R: Runtime>(app: &AppHandle<R>, state: &AppState, id: &str, line: String) {
    let frp = frp_state(state);
    let line = provider_log::sanitize_line(line);
    if is_internal_admin_api_log(&line) {
        return;
    }
    let attention = is_attention_log(&line);
    let status_changed = {
        let mut rt = frp.runtime.lock();
        if let Some(names) = proxy_added_names(&line) {
            let inst = rt.instance_mut(id);
            for name in names {
                inst.proxy_warning.remove(&name);
                inst.proxy_running.insert(name);
            }
        }
        if let Some(name) = bracket_proxy_name(&line, " start proxy success") {
            let inst = rt.instance_mut(id);
            inst.proxy_warning.remove(&name);
            inst.proxy_running.insert(name);
        }
        if let Some(name) = bracket_proxy_name(&line, " start error:") {
            let inst = rt.instance_mut(id);
            inst.proxy_running.remove(&name);
            inst.proxy_warning.insert(name);
        }
        if let Some(name) = already_exists_name(&line) {
            let inst = rt.instance_mut(id);
            inst.proxy_running.remove(&name);
            inst.proxy_warning.insert(name);
        }
        if attention {
            match rt.instances.get_mut(id) {
                Some(inst) if matches!(inst.status, FrpcStatus::Running | FrpcStatus::Starting) => {
                    inst.status = FrpcStatus::Warning;
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    };
    emit_native_log_line(app, state, id, line);
    if status_changed {
        emit_status(app, state, id, FrpcStatus::Warning);
    } else {
        let recovered = {
            let profile = state.config.get_profile(id).ok();
            let mut rt = frp.runtime.lock();
            match (profile, rt.instances.get_mut(id)) {
                (Some(profile), Some(inst))
                    if matches!(inst.status, FrpcStatus::Starting | FrpcStatus::Warning) =>
                {
                    let enabled = enabled_proxy_names(&profile);
                    let ok = !enabled.is_empty()
                        && inst.proxy_warning.is_empty()
                        && enabled.iter().all(|name| inst.proxy_running.contains(name));
                    if ok {
                        inst.status = FrpcStatus::Running;
                    }
                    ok
                }
                _ => false,
            }
        };
        if recovered {
            emit_status(app, state, id, FrpcStatus::Running);
        }
    }
}

pub(crate) fn handle_watchdog_event<R: Runtime>(
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
        WatchdogEvent::ProcessStarted {
            tunnel_id: id, pid, ..
        } => {
            let changed = mark_watchdog_spawned(state, &id, pid);
            if changed {
                emit_status(app, state, &id, FrpcStatus::Starting);
                spawn_start_status_probe(app.clone(), state.clone(), id.clone());
                if let Ok(profile) = state.config.get_profile(&id) {
                    spawn_local_checks(app.clone(), state.clone(), id, profile.proxies);
                }
            }
        }
        WatchdogEvent::ProcessSnapshot {
            tunnel_id: id,
            pid,
            recent_logs,
            ..
        } => {
            let changed = mark_watchdog_snapshot(state, &id, pid);
            if changed {
                emit_status(app, state, &id, FrpcStatus::Running);
                emit_system_log_line(
                    app,
                    state,
                    &id,
                    provider_log::watchdog_recovered_line("frpc"),
                );
                // 回放看门狗缓存的最近日志，重建各代理健康度：近期日志里若有
                // 代理启动失败 / already exists，会把状态从 Running 校正为 Warning，
                // 而不是仅凭"进程还活着"就无脑判成功。
                for line in recent_logs {
                    handle_frpc_log_line(app, state, &id, line);
                }
                if let Ok(profile) = state.config.get_profile(&id) {
                    spawn_local_checks(app.clone(), state.clone(), id, profile.proxies);
                }
            }
        }
        WatchdogEvent::ProcessLog {
            tunnel_id: id,
            line,
            ..
        } => {
            handle_frpc_log_line(app, state, &id, line);
        }
        WatchdogEvent::ProcessExit {
            tunnel_id: id,
            success,
            cleanup_success,
            cleanup_error,
            ..
        } => {
            let clean_exit = watchdog_exit_clean(success, cleanup_success);
            let status = mark_watchdog_exited(state, &id, clean_exit);
            if !success {
                emit_system_log_line(
                    app,
                    state,
                    &id,
                    provider_log::watchdog_exit_state_message(
                        "frpc",
                        success,
                        cleanup_success,
                        cleanup_error.as_deref(),
                    ),
                );
            }
            if success && !cleanup_success.unwrap_or(true) {
                let message = cleanup_error
                    .as_deref()
                    .filter(|error| !error.trim().is_empty());
                emit_system_log_line(
                    app,
                    state,
                    &id,
                    provider_log::cleanup_unconfirmed_line("frpc", "cleanup", message),
                );
            }
            emit_status(app, state, &id, status);
            emit_config_dirty(app, &id, false);
        }
        WatchdogEvent::ProcessError {
            tunnel_id: id,
            message,
            ..
        } => {
            let line = mark_watchdog_error(state, &id, &message);
            emit_system_log_line(app, state, &id, line);
            emit_status(app, state, &id, FrpcStatus::Errored);
        }
    }
}

pub(crate) fn handle_watchdog_eof<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let reconciled = reconcile_watchdog_eof(state);
    for id in reconciled {
        provider_log::emit_watchdog_stream_closed(app, state, FRP_PROVIDER_ID, &id);
        emit_status(app, state, &id, FrpcStatus::Errored);
    }
}

pub fn stop<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
) -> AppResult<FrpcStatus> {
    let frp = frp_state(state);
    watchdog_relay::release(state, FRP_PROVIDER_ID, profile_id);
    // 仅在该实例处于运行中态时才需要动作。
    let running = {
        let rt = frp.runtime.lock();
        rt.instances
            .get(profile_id)
            .map(|i| i.status.is_active())
            .unwrap_or(false)
    };
    if !running {
        return Ok(frp.runtime.status(profile_id));
    }

    {
        let mut rt = frp.runtime.lock();
        if let Some(inst) = rt.instances.get_mut(profile_id) {
            inst.status = FrpcStatus::Stopping;
            inst.needs_restart = false;
            inst.process_restart_required = false;
        }
    }
    emit_status(app, state, profile_id, FrpcStatus::Stopping);

    // 主程序侧先经 admin API 优雅断开（与看门狗冗余）：即便 sidecar 暂时不可达，也能让 frps 立即释放隧道。
    let admin = {
        let rt = frp.runtime.lock();
        rt.instances
            .get(profile_id)
            .and_then(|inst| inst.admin_endpoint())
    };
    if let Some((port, auth)) = admin {
        graceful_stop_port(port, auth.as_deref());
    }
    let config_path = {
        let rt = frp.runtime.lock();
        rt.instances
            .get(profile_id)
            .and_then(|inst| inst.config_path.clone())
    };

    // 让看门狗停掉这一个 frpc（其它 frpc 不受影响，看门狗本身保持存活）。
    let sent = process_watchdog::send_if_alive(
        state,
        WatchdogRequest::StopProcess(WatchdogStopProcessRequest {
            provider_id: FRP_PROVIDER_ID.into(),
            tunnel_id: profile_id.to_string(),
        }),
    )?;
    if !sent {
        // 看门狗已不在不代表 frpc 必然退出；macOS 上 sidecar 被强杀时没有
        // PDEATHSIG/Job Object 兜底，必须按运行期 config 再扫一次。
        if let Some(config_path) = config_path.as_ref() {
            cleanup_frpc_for_config(config_path)?;
        }
        {
            let mut rt = frp.runtime.lock();
            if let Some(inst) = rt.instances.get_mut(profile_id) {
                inst.status = FrpcStatus::Stopped;
                inst.pid = None;
            }
        }
        emit_status(app, state, profile_id, FrpcStatus::Stopped);
        emit_config_dirty(app, profile_id, false);
        return Ok(FrpcStatus::Stopped);
    }

    // 真正的 Stopped 由看门狗的 `exited` 事件驱动（见 spawn_event_reader）。
    Ok(FrpcStatus::Stopping)
}

/// 删除 Profile 前的清理：先停掉可能在跑的 frpc，再从运行态移除该实例，
/// 最后删掉运行时配置文件（内含明文密钥，不可残留）。
pub fn cleanup_for_delete<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    profile_id: &str,
) -> AppResult<()> {
    stop(app, state, profile_id)?;
    let frp = frp_state(state);
    watchdog_relay::release(state, FRP_PROVIDER_ID, profile_id);
    let config_path = frp
        .runtime
        .lock()
        .instances
        .remove(profile_id)
        .and_then(|instance| instance.config_path);
    if let Ok(runtime_dir) = frp_paths::runtime_dir(app) {
        if let Some(cfg_path) = config_path {
            paths::remove_file_if_under(&cfg_path, &runtime_dir);
        }
        if let Ok(cfg_path) = frp_paths::runtime_config(app, profile_id) {
            paths::remove_file_if_under(&cfg_path, &runtime_dir);
        }
    }
    Ok(())
}

/// 正常退出（如托盘「退出」）时关停看门狗，由它清理掉所有 frpc。
/// 异常退出的兜底由看门狗负责（父进程存活轮询 / stdin EOF / Job Object / PDEATHSIG）。
pub fn stop_all<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> AppResult<()> {
    let frp = frp_state(state);
    let traffic_ids: Vec<String> = {
        let rt = frp.runtime.lock();
        rt.instances.keys().cloned().collect()
    };
    for id in traffic_ids {
        watchdog_relay::release(state, FRP_PROVIDER_ID, &id);
    }

    // 先由主程序经 admin API 优雅断开所有在跑的 frpc：frps 会立即注销隧道，避免重连报“已存在”。
    // 这一步不依赖看门狗 sidecar 是否为最新构建；停顿一下让“干净断开”经反向代理送达 frps，
    // 之后再交给看门狗收尾（强杀仅兜底）。
    let admins: Vec<(u16, Option<String>)> = {
        let rt = frp.runtime.lock();
        rt.instances
            .values()
            .filter(|inst| inst.status.is_active())
            .filter_map(|inst| inst.admin_endpoint())
            .collect()
    };
    let total = admins.len();
    let mut graceful = 0usize;
    for (port, auth) in admins {
        if graceful_stop_port(port, auth.as_deref()) {
            graceful += 1;
        }
    }
    if graceful > 0 {
        crate::diag::info(
            &crate::diag::provider_scope(FRP_PROVIDER_ID),
            format!(
                "shutdown: gracefully stopped {graceful}/{total} frpc process(es); frps should release tunnels immediately"
            ),
        );
        std::thread::sleep(Duration::from_millis(1000));
    } else if total > 0 {
        crate::diag::warn(
            &crate::diag::provider_scope(FRP_PROVIDER_ID),
            format!(
                "shutdown: {total} frpc process(es) were not stopped gracefully; watchdog will force stop them"
            ),
        );
    }

    cleanup_all_runtime_frpc(app)?;

    // 把仍在运行中态的实例统一归位为 Stopped（一致性；此刻通常即将退出进程）。
    let ids: Vec<String> = {
        let mut rt = frp.runtime.lock();
        rt.instances
            .iter_mut()
            .filter(|(_, inst)| inst.status.is_active())
            .map(|(id, inst)| {
                inst.status = FrpcStatus::Stopped;
                inst.pid = None;
                id.clone()
            })
            .collect()
    };
    for id in ids {
        emit_status(app, state, &id, FrpcStatus::Stopped);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::{AppData, AppSettings};

    use crate::providers::frp::data::{FrpData, FrpSettings};

    use crate::providers::frp::domain::{ProxyBase, TcpProxy};

    use crate::providers::frp::runtime_state::FrpcInstance;

    use std::collections::BTreeMap;

    use std::time::Instant;

    fn state_with_instance(id: &str, status: FrpcStatus) -> AppState {
        let state = AppState::default();
        {
            let frp = frp_state(&state);
            let mut rt = frp.runtime.lock();
            let inst = rt.instance_mut(id);
            inst.status = status;
        }
        state
    }

    fn tcp_profile_proxy(id: &str, name: &str) -> ProfileProxy {
        ProfileProxy {
            id: id.to_string(),
            enabled: true,
            config: ProxyConfig::Tcp(TcpProxy {
                base: ProxyBase {
                    name: name.to_string(),
                    local_ip: "127.0.0.1".to_string(),
                    local_port: Some(8080),
                    ..Default::default()
                },
                remote_port: Some(18080),
            }),
        }
    }

    // 看门狗重连回放最近日志时复用 handle_frpc_log_line：近期代理启动失败应把
    // 重连后默认置的 Running 校正为 Warning，而不是无脑判成功。
    #[test]
    fn replayed_log_downgrades_running_to_warning_on_proxy_error() {
        let state = state_with_instance("p1", FrpcStatus::Running);
        let app = tauri::test::mock_builder()
            .manage(state.clone())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app_handle = app.handle().clone();

        handle_frpc_log_line(
            &app_handle,
            &state,
            "p1",
            "[web] start error: port already used".into(),
        );
        assert_eq!(frp_state(&state).runtime.status("p1"), FrpcStatus::Warning);
    }

    #[cfg(unix)]
    fn unique_test_dir(name: &str) -> PathBuf {
        let suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir()
            .join("tunnelx-tests")
            .join(name)
            .join(suffix);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[cfg(unix)]
    fn write_executable_script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    fn wait_for_status(state: &AppState, profile_id: &str, expected: FrpcStatus) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if frp_state(state).runtime.status(profile_id) == expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(frp_state(state).runtime.status(profile_id), expected);
    }

    fn parse_watchdog_frame_line(
        line: &str,
    ) -> Result<tunnelx_watchdog_protocol::WatchdogFrame, serde_json::Error> {
        serde_json::from_str::<tunnelx_watchdog_protocol::WatchdogFrame>(line)
    }

    fn wait_for_watchdog_none(state: &AppState) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if state.process_watchdog.pid().is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(state.process_watchdog.pid().is_none());
    }

    fn loopback_bind_available() -> bool {
        TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).is_ok()
    }

    #[test]
    #[cfg(unix)]
    fn start_and_stop_lifecycle_uses_watchdog_protocol() {
        if !loopback_bind_available() {
            return;
        }

        let _guard = crate::paths::data_dir_test_guard();
        let root = unique_test_dir("start-stop");
        crate::paths::set_test_data_dir(root.join("data"));

        let state = AppState::default();
        let app = tauri::test::mock_builder()
            .manage(state.clone())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app_handle = app.handle().clone();

        let version = "test-version";
        let fake_frpc = frp_paths::frpc_exe(&app_handle, version).unwrap();
        write_executable_script(&fake_frpc, "#!/bin/sh\nexit 0\n");

        let fake_watchdog = root.join("tunnelx-watchdog");
        write_executable_script(
            &fake_watchdog,
            r#"#!/bin/sh
    while IFS= read -r line; do
      id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
      case "$line" in
        *'"name":"heartbeat"'*)
          echo '{"type":"response","version":7,"id":"'"$id"'","seq":0,"name":"accepted","payload":{"request":"heartbeat"}}'
          ;;
        *'"name":"start_process"'*)
          echo '{"type":"response","version":7,"id":"'"$id"'","seq":1,"name":"accepted","payload":{"request":"start_process"}}'
          echo '{"type":"event","version":7,"id":"event-1","seq":2,"name":"process_started","payload":{"providerId":"frp","tunnelId":"p1","pid":4242}}'
          echo '{"type":"event","version":7,"id":"event-2","seq":3,"name":"process_log","payload":{"providerId":"frp","tunnelId":"p1","line":"[web] start proxy success"}}'
          ;;
        *'"name":"stop_process"'*)
          echo '{"type":"response","version":7,"id":"'"$id"'","seq":4,"name":"accepted","payload":{"request":"stop_process"}}'
          echo '{"type":"event","version":7,"id":"event-3","seq":5,"name":"process_exit","payload":{"providerId":"frp","tunnelId":"p1","success":true,"forced":false,"code":0,"cleanupSuccess":true,"cleanupError":null}}'
          exit 0
          ;;
        *'"name":"shutdown"'*)
          echo '{"type":"response","version":7,"id":"'"$id"'","seq":6,"name":"accepted","payload":{"request":"shutdown"}}'
          exit 0
          ;;
      esac
    done
    "#,
        );
        process_watchdog::set_test_watchdog_exe(fake_watchdog);

        let mut profile = Profile::new("Lifecycle");
        profile.id = "p1".into();
        profile.proxies = vec![tcp_profile_proxy("proxy-1", "web")];
        let mut providers = BTreeMap::new();
        providers.insert(
            FRP_PROVIDER_ID.into(),
            serde_json::to_value(FrpData {
                profiles: vec![profile],
                settings: FrpSettings {
                    active_frpc_version: Some(version.into()),
                    ..Default::default()
                },
            })
            .unwrap(),
        );
        state.config.replace(AppData {
            connection_order: Vec::new(),
            providers,
            settings: AppSettings {
                traffic_stats_enabled: false,
                ..Default::default()
            },
        });

        assert_eq!(
            start(&app_handle, &state, "p1").unwrap(),
            FrpcStatus::Starting
        );
        wait_for_status(&state, "p1", FrpcStatus::Running);

        let cfg_path = {
            let frp = frp_state(&state);
            let rt = frp.runtime.lock();
            let inst = rt.instances.get("p1").unwrap();
            assert_eq!(inst.pid, Some(4242));
            assert!(inst.admin_port.is_some());
            inst.config_path.clone().unwrap()
        };
        let cfg = fs::read_to_string(&cfg_path).unwrap();
        assert!(cfg.contains(r#""serverAddr": "127.0.0.1""#));
        assert!(cfg.contains(r#""webServer""#));

        // The fake watchdog does not start a real frpc admin API. Keep stop focused on the
        // watchdog command/event path instead of waiting for the best-effort admin request.
        frp_state(&state)
            .runtime
            .lock()
            .instances
            .get_mut("p1")
            .unwrap()
            .admin_port = None;

        assert!(matches!(
            stop(&app_handle, &state, "p1").unwrap(),
            FrpcStatus::Stopping | FrpcStatus::Stopped
        ));
        wait_for_status(&state, "p1", FrpcStatus::Stopped);
        wait_for_watchdog_none(&state);

        let frp = frp_state(&state);
        let rt = frp.runtime.lock();
        let inst = rt.instances.get("p1").unwrap();
        assert_eq!(inst.pid, None);
        assert!(!inst.needs_restart);
        assert!(!inst.process_restart_required);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn cleanup_for_delete_removes_runtime_instance_and_config_file() {
        let _guard = crate::paths::data_dir_test_guard();
        let root = unique_test_dir("cleanup-delete");
        crate::paths::set_test_data_dir(root.join("data"));

        let state = AppState::default();
        let app = tauri::test::mock_builder()
            .manage(state.clone())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let app_handle = app.handle().clone();
        let cfg_path = frp_paths::runtime_config(&app_handle, "p1").unwrap();
        fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        fs::write(&cfg_path, "{}").unwrap();
        {
            let frp = frp_state(&state);
            let mut rt = frp.runtime.lock();
            rt.instances.insert(
                "p1".into(),
                FrpcInstance {
                    status: FrpcStatus::Stopped,
                    config_path: Some(cfg_path.clone()),
                    ..Default::default()
                },
            );
        }

        cleanup_for_delete(&app_handle, &state, "p1").unwrap();

        let frp = frp_state(&state);
        assert!(!frp.runtime.lock().instances.contains_key("p1"));
        assert!(!cfg_path.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn first_remote_addr_takes_first_nonempty() {
        assert_eq!(first_remote_addr("a.com, b.com"), "a.com");
        assert_eq!(first_remote_addr(" ; x.com ; y.com"), "x.com");
        assert_eq!(first_remote_addr(""), "");
    }

    #[test]
    fn remote_addr_port_reads_common_addr_forms() {
        assert_eq!(remote_addr_port("http://example.com:8080/path"), Some(8080));
        assert_eq!(remote_addr_port("example.com:1234"), Some(1234));
        assert_eq!(remote_addr_port("[::1]:9000"), Some(9000));
        assert_eq!(remote_addr_port("1.2.3.4"), None);
    }

    #[test]
    fn parse_status_public_addrs_from_custom_domains() {
        let statuses = parse_proxy_status(
            r#"{"http":[{"name":"web","status":"running","customDomains":["app.example.com","api.example.com"]}]}"#,
        )
        .unwrap();

        assert_eq!(
            statuses[0].public_addrs,
            vec![
                "http://app.example.com".to_string(),
                "http://api.example.com".to_string()
            ]
        );
        assert_eq!(statuses[0].public_addr, "http://app.example.com");
    }

    #[test]
    fn parse_status_public_addrs_prefers_public_url_fields() {
        let statuses = parse_proxy_status(
            r#"{"https":[{"name":"secure","status":"running","publicUrls":["https://runtime.example.com"],"remoteAddr":"fallback.example.com"}]}"#,
        )
        .unwrap();

        assert_eq!(
            statuses[0].public_addrs,
            vec![
                "https://runtime.example.com".to_string(),
                "https://fallback.example.com".to_string()
            ]
        );
    }

    #[test]
    fn runtime_public_urls_uses_standard_runtime_shape() {
        let statuses = parse_proxy_status(
            r#"{"tcp":[{"name":"ssh","status":"running","remoteAddr":"example.com:6000"}]}"#,
        )
        .unwrap();
        let urls = runtime_public_urls(&statuses);

        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].name, "ssh");
        assert_eq!(urls[0].proto, "tcp");
        assert_eq!(urls[0].public_url, "example.com:6000");
    }

    #[test]
    fn admin_status_probe_marks_empty_enabled_set_running() {
        let profile = Profile::new("No proxy");

        assert_eq!(admin_status_probe(&profile, &[]), AdminStatusProbe::Running);
    }

    #[test]
    fn admin_status_probe_waits_for_enabled_proxy_statuses() {
        let mut profile = Profile::new("With proxy");
        profile.proxies = vec![tcp_profile_proxy("proxy-1", "web")];

        assert_eq!(admin_status_probe(&profile, &[]), AdminStatusProbe::Pending);
    }

    #[test]
    fn admin_status_probe_uses_structured_proxy_status() {
        let mut profile = Profile::new("With proxy");
        profile.proxies = vec![tcp_profile_proxy("proxy-1", "web")];
        let running = parse_proxy_status(r#"{"tcp":[{"name":"web","status":"running"}]}"#).unwrap();
        let warning = parse_proxy_status(
            r#"{"tcp":[{"name":"web","status":"start error","err":"port used"}]}"#,
        )
        .unwrap();

        assert_eq!(
            admin_status_probe(&profile, &running),
            AdminStatusProbe::Running
        );
        assert_eq!(
            admin_status_probe(&profile, &warning),
            AdminStatusProbe::Warning
        );
    }

    #[test]
    fn parse_watchdog_event_line_rejects_malformed_events() {
        assert!(parse_watchdog_frame_line("{not-json").is_err());
        assert!(parse_watchdog_frame_line(r#"{"ev":"started","pid":42}"#).is_err());
        assert!(parse_watchdog_frame_line(r#"{"ev":"renamed","id":"p1"}"#).is_err());
    }

    #[test]
    fn local_tcp_port_available_detects_bound_port() {
        let Ok(listener) = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))) else {
            return;
        };
        let port = listener.local_addr().unwrap().port();

        assert!(!local_tcp_port_available(port));
        drop(listener);
        let released = (0..20).any(|_| {
            if local_tcp_port_available(port) {
                true
            } else {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
        });
        assert!(released);
    }

    #[test]
    fn local_target_formats_ipv6_address() {
        let mut proxy = tcp_profile_proxy("p1", "web");
        if let ProxyConfig::Tcp(tcp) = &mut proxy.config {
            tcp.base.local_ip = "::1".to_string();
            tcp.base.local_port = Some(1420);
        }

        let target = local_target(&proxy).unwrap();

        assert_eq!(target.host, "::1");
        assert_eq!(target.port, 1420);
        assert_eq!(target.address, "[::1]:1420");
    }

    #[test]
    fn local_check_log_is_english_and_points_to_alternative() {
        let target = LocalTarget {
            proxy_name: "web".to_string(),
            host: Ipv4Addr::LOCALHOST.to_string(),
            port: 1420,
            address: "127.0.0.1:1420".to_string(),
        };
        let alternative = LocalAlternative {
            host: "::1".to_string(),
            address: "[::1]:1420".to_string(),
        };

        let line = format_local_check_log(&target, Some(&alternative));

        assert!(line.contains("self-check web TCP target 127.0.0.1:1420 is not listening"));
        assert!(line.contains("[::1]:1420 is reachable"));
        assert!(line.contains("set Local IP to ::1"));
    }

    #[test]
    fn ensure_admin_port_auto_assigns_local_secured_port() {
        if !loopback_bind_available() {
            return;
        }

        let mut profile = Profile::new("test");

        let admin = ensure_admin_port(&mut profile).unwrap();

        assert!(admin.auto_assigned);
        assert_eq!(profile.server.web_server.addr, "127.0.0.1");
        assert_eq!(profile.server.web_server.port, admin.port);
        assert_ne!(admin.port, 0);
        assert_eq!(profile.server.web_server.user, "tunnelx");
        assert!(!profile.server.web_server.password.is_empty());
    }

    #[test]
    fn validate_enabled_proxy_names_rejects_empty_and_duplicate_names() {
        let mut profile = Profile::new("test");
        profile.proxies = vec![
            tcp_profile_proxy("p1", "web"),
            tcp_profile_proxy("p2", " web "),
        ];
        assert!(matches!(
            validate_enabled_proxy_names(&profile),
            Err(AppError::ConfigFormatError(_))
        ));

        profile.proxies = vec![tcp_profile_proxy("p1", "")];
        assert!(matches!(
            validate_enabled_proxy_names(&profile),
            Err(AppError::ConfigFormatError(_))
        ));

        profile.proxies = vec![tcp_profile_proxy("p1", "web")];
        assert!(validate_enabled_proxy_names(&profile).is_ok());
    }

    #[test]
    fn runtime_transport_defaults_fill_missing_pool_count_for_work_connections() {
        let mut profile = Profile::new("test");
        profile.server.transport.protocol = "tcp".to_string();
        profile.server.transport.pool_count = None;
        profile.proxies = vec![tcp_profile_proxy("p1", "web")];

        apply_runtime_transport_defaults(&mut profile);

        assert_eq!(
            profile.server.transport.pool_count,
            Some(DEFAULT_WORK_CONN_POOL_COUNT)
        );
    }

    #[test]
    fn runtime_transport_defaults_preserve_explicit_pool_count() {
        let mut profile = Profile::new("test");
        profile.server.transport.protocol = "tcp".to_string();
        profile.server.transport.pool_count = Some(0);
        profile.proxies = vec![tcp_profile_proxy("p1", "web")];

        apply_runtime_transport_defaults(&mut profile);

        assert_eq!(profile.server.transport.pool_count, Some(0));
    }

    #[test]
    fn mark_watchdog_spawned_updates_existing_instance() {
        let state = state_with_instance("p1", FrpcStatus::Starting);

        assert!(mark_watchdog_spawned(&state, "p1", 42));
        {
            let frp = frp_state(&state);
            let rt = frp.runtime.lock();
            let inst = rt.instances.get("p1").unwrap();
            assert_eq!(inst.status, FrpcStatus::Starting);
            assert_eq!(inst.pid, Some(42));
        }

        assert!(!mark_watchdog_spawned(&state, "missing", 7));
        assert!(!frp_state(&state)
            .runtime
            .lock()
            .instances
            .contains_key("missing"));
    }

    #[test]
    fn mark_watchdog_exited_clears_runtime_flags() {
        let state = state_with_instance("p1", FrpcStatus::Running);
        {
            let frp = frp_state(&state);
            let mut rt = frp.runtime.lock();
            let inst = rt.instances.get_mut("p1").unwrap();
            inst.pid = Some(42);
            inst.needs_restart = true;
            inst.process_restart_required = true;
        }

        let status = mark_watchdog_exited(&state, "p1", false);
        assert_eq!(status, FrpcStatus::Errored);
        let frp = frp_state(&state);
        let rt = frp.runtime.lock();
        let inst = rt.instances.get("p1").unwrap();
        assert_eq!(inst.status, FrpcStatus::Errored);
        assert_eq!(inst.pid, None);
        assert!(!inst.needs_restart);
        assert!(!inst.process_restart_required);
    }

    #[test]
    fn mark_watchdog_exited_treats_cleanup_failure_as_error() {
        let state = state_with_instance("p1", FrpcStatus::Stopping);
        {
            let frp = frp_state(&state);
            frp.runtime.lock().instances.get_mut("p1").unwrap().pid = Some(42);
        }

        let status = mark_watchdog_exited(&state, "p1", false);

        assert_eq!(status, FrpcStatus::Errored);
        let frp = frp_state(&state);
        let rt = frp.runtime.lock();
        let inst = rt.instances.get("p1").unwrap();
        assert_eq!(inst.status, FrpcStatus::Errored);
        assert_eq!(inst.pid, None);
    }

    #[test]
    fn mark_watchdog_error_sets_errored() {
        let state = state_with_instance("p1", FrpcStatus::Starting);
        {
            let frp = frp_state(&state);
            let mut rt = frp.runtime.lock();
            rt.instances.get_mut("p1").unwrap().pid = Some(42);
        }

        let line = mark_watchdog_error(&state, "p1", "spawn failed");
        assert_eq!(line, "[watchdog] spawn failed");
        let frp = frp_state(&state);
        let rt = frp.runtime.lock();
        let inst = rt.instances.get("p1").unwrap();
        assert_eq!(inst.status, FrpcStatus::Errored);
        assert_eq!(inst.pid, None);
    }

    #[test]
    fn reconcile_watchdog_eof_marks_only_live_instances_errored() {
        let state = AppState::default();
        {
            let frp = frp_state(&state);
            let mut rt = frp.runtime.lock();
            rt.instances.insert(
                "running".into(),
                FrpcInstance {
                    status: FrpcStatus::Running,
                    pid: Some(1),
                    ..Default::default()
                },
            );
            rt.instances.insert(
                "warning".into(),
                FrpcInstance {
                    status: FrpcStatus::Warning,
                    pid: Some(2),
                    ..Default::default()
                },
            );
            rt.instances.insert(
                "stopped".into(),
                FrpcInstance {
                    status: FrpcStatus::Stopped,
                    pid: Some(3),
                    ..Default::default()
                },
            );
        }

        let reconciled = reconcile_watchdog_eof(&state)
            .into_iter()
            .collect::<HashSet<_>>();
        assert_eq!(
            reconciled,
            HashSet::from(["running".to_string(), "warning".to_string()])
        );
        let frp = frp_state(&state);
        let rt = frp.runtime.lock();
        assert_eq!(
            rt.instances.get("running").unwrap().status,
            FrpcStatus::Errored
        );
        assert_eq!(rt.instances.get("running").unwrap().pid, None);
        assert_eq!(
            rt.instances.get("warning").unwrap().status,
            FrpcStatus::Errored
        );
        assert_eq!(rt.instances.get("warning").unwrap().pid, None);
        assert_eq!(
            rt.instances.get("stopped").unwrap().status,
            FrpcStatus::Stopped
        );
        assert_eq!(rt.instances.get("stopped").unwrap().pid, Some(3));
    }
}
