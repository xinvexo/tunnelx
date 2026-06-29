use super::cloudflare::CloudflareProvider;
use super::contract::{
    empty_details, normalize_tunnel_name, CreateTunnelInput, MetricSample, ProviderCommandOutput,
    ProviderContext, ProviderDescriptor, ProviderMetrics, ProviderStatus, RuntimeMetrics,
    RuntimeTrafficSummary, TunnelProvider, TunnelResource, TunnelRuntimeInfo, TunnelRuntimeState,
    CLOUDFLARE_PROVIDER_ID, CPOLAR_PROVIDER_ID, FRP_PROVIDER_ID, NGROK_PROVIDER_ID,
    PINGGY_PROVIDER_ID, TUNNEL_NAME_MAX_CHARS,
};
use super::cpolar::CpolarProvider;
use super::frp::services::frpc_service;
use super::frp::state::frp_state;
use super::frp::FrpProvider;
use super::ngrok::NgrokProvider;
use super::pinggy::PinggyProvider;
use crate::error::{AppError, AppResult};
use crate::services::process_metrics;
use crate::services::process_watchdog;
use crate::services::provider_log;
use crate::services::redaction;
use crate::services::watchdog_log;
use crate::services::watchdog_relay;
use crate::state::AppState;
use std::collections::HashSet;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Runtime};
use tunnelx_watchdog_protocol::WatchdogEvent;

const TRAFFIC_SETTING_RESTART_TIMEOUT: Duration = Duration::from_secs(90);
const TRAFFIC_SETTING_RESTART_POLL: Duration = Duration::from_millis(250);
const EXIT_STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const EXIT_STOP_WAIT_MARGIN: Duration = Duration::from_secs(2);
const EXIT_STOP_WAIT_POLL: Duration = Duration::from_millis(100);

static CLOUDFLARE: CloudflareProvider = CloudflareProvider;
static CPOLAR: CpolarProvider = CpolarProvider;
static FRP: FrpProvider = FrpProvider;
static NGROK: NgrokProvider = NgrokProvider;
static PINGGY: PinggyProvider = PinggyProvider;

struct ProviderRegistration {
    id: &'static str,
    provider: &'static dyn TunnelProvider,
    watchdog: WatchdogHooks,
}

enum WatchdogHooks {
    Frp(&'static FrpProvider),
    Cloudflare(&'static CloudflareProvider),
    Ngrok(&'static NgrokProvider),
    Cpolar(&'static CpolarProvider),
    Pinggy(&'static PinggyProvider),
}

impl WatchdogHooks {
    fn event<R: Runtime>(&self, app: &AppHandle<R>, state: &AppState, event: WatchdogEvent) {
        match self {
            WatchdogHooks::Frp(provider) => provider.handle_watchdog_event(app, state, event),
            WatchdogHooks::Cloudflare(provider) => {
                provider.handle_watchdog_event(app, state, event)
            }
            WatchdogHooks::Ngrok(provider) => provider.handle_watchdog_event(app, state, event),
            WatchdogHooks::Cpolar(provider) => provider.handle_watchdog_event(app, state, event),
            WatchdogHooks::Pinggy(provider) => provider.handle_watchdog_event(app, state, event),
        }
    }

    fn eof<R: Runtime>(&self, app: &AppHandle<R>, state: &AppState) {
        match self {
            WatchdogHooks::Frp(provider) => provider.handle_watchdog_eof(app, state),
            WatchdogHooks::Cloudflare(provider) => provider.handle_watchdog_eof(app, state),
            WatchdogHooks::Ngrok(provider) => provider.handle_watchdog_eof(app, state),
            WatchdogHooks::Cpolar(provider) => provider.handle_watchdog_eof(app, state),
            WatchdogHooks::Pinggy(provider) => provider.handle_watchdog_eof(app, state),
        }
    }
}

static PROVIDERS: [ProviderRegistration; 5] = [
    ProviderRegistration {
        id: FRP_PROVIDER_ID,
        provider: &FRP,
        watchdog: WatchdogHooks::Frp(&FRP),
    },
    ProviderRegistration {
        id: CLOUDFLARE_PROVIDER_ID,
        provider: &CLOUDFLARE,
        watchdog: WatchdogHooks::Cloudflare(&CLOUDFLARE),
    },
    ProviderRegistration {
        id: NGROK_PROVIDER_ID,
        provider: &NGROK,
        watchdog: WatchdogHooks::Ngrok(&NGROK),
    },
    ProviderRegistration {
        id: CPOLAR_PROVIDER_ID,
        provider: &CPOLAR,
        watchdog: WatchdogHooks::Cpolar(&CPOLAR),
    },
    ProviderRegistration {
        id: PINGGY_PROVIDER_ID,
        provider: &PINGGY,
        watchdog: WatchdogHooks::Pinggy(&PINGGY),
    },
];

fn all() -> impl Iterator<Item = &'static dyn TunnelProvider> {
    PROVIDERS.iter().map(|entry| entry.provider)
}

pub fn descriptors() -> Vec<ProviderDescriptor> {
    all().map(|provider| provider.descriptor()).collect()
}

pub fn get(provider_id: &str) -> AppResult<&'static dyn TunnelProvider> {
    get_registration(provider_id).map(|entry| entry.provider)
}

fn get_registration(provider_id: &str) -> AppResult<&'static ProviderRegistration> {
    PROVIDERS
        .iter()
        .find(|entry| entry.id == provider_id)
        .ok_or_else(|| AppError::Msg(format!("Unknown provider: {provider_id}")))
}

pub fn status(app: &AppHandle, state: &AppState, provider_id: &str) -> AppResult<ProviderStatus> {
    get(provider_id)?
        .status(ProviderContext::new(app, state))
        .map(redact_provider_status)
}

pub fn login(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
) -> AppResult<ProviderCommandOutput> {
    get(provider_id)?
        .login(ProviderContext::new(app, state))
        .map(redact_provider_command_output)
}

fn redact_provider_status(mut status: ProviderStatus) -> ProviderStatus {
    status.message = redaction::text(status.message);
    status.details = redaction::json_value(status.details);
    status
}

fn redact_provider_command_output(mut output: ProviderCommandOutput) -> ProviderCommandOutput {
    output.stdout = redaction::text(output.stdout);
    output.stderr = redaction::text(output.stderr);
    output
}

fn redact_tunnel_resources(resources: Vec<TunnelResource>) -> Vec<TunnelResource> {
    resources.into_iter().map(redact_tunnel_resource).collect()
}

pub(crate) fn redact_tunnel_resource(mut resource: TunnelResource) -> TunnelResource {
    resource.metadata = redaction::json_value(resource.metadata);
    resource
}

pub fn list_tunnels(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
) -> AppResult<Vec<TunnelResource>> {
    get(provider_id)?
        .list_tunnels(ProviderContext::new(app, state))
        .map(redact_tunnel_resources)
}

fn tunnel_order_key(resource: &TunnelResource) -> String {
    format!("{}:{}", resource.provider_id, resource.id)
}

pub fn reorder_tunnels(app: &AppHandle, state: &AppState, keys: Vec<String>) -> AppResult<()> {
    let current = all()
        .map(|provider| provider.list_tunnels(ProviderContext::new(app, state)))
        .collect::<AppResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let valid = current.iter().map(tunnel_order_key).collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for key in keys {
        if valid.contains(&key) && seen.insert(key.clone()) {
            order.push(key);
        }
    }
    for resource in &current {
        let key = tunnel_order_key(resource);
        if seen.insert(key.clone()) {
            order.push(key);
        }
    }
    let before = state.config.snapshot()?;
    state.config.set_connection_order(order)?;
    if let Err(error) = crate::store::save_app_data(app, state) {
        state.config.replace(before);
        return Err(error);
    }
    Ok(())
}

pub fn create_tunnel(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    mut input: CreateTunnelInput,
) -> AppResult<TunnelResource> {
    input.name = normalize_tunnel_name(&input.name)?;
    get(provider_id)?
        .create_tunnel(ProviderContext::new(app, state), input)
        .map(redact_tunnel_resource)
}

pub fn update_tunnel(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    mut tunnel: TunnelResource,
) -> AppResult<TunnelResource> {
    tunnel.name = normalize_tunnel_name(&tunnel.name)?;
    get(provider_id)?
        .update_tunnel(ProviderContext::new(app, state), tunnel)
        .map(redact_tunnel_resource)
}

pub fn duplicate_tunnel(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    id: &str,
) -> AppResult<TunnelResource> {
    let provider = get(provider_id)?;
    let tunnels = provider.list_tunnels(ProviderContext::new(app, state))?;
    let source = tunnels
        .iter()
        .find(|tunnel| tunnel.id == id)
        .ok_or_else(|| AppError::TunnelNotFound(id.into()))?;
    let name = duplicate_name(&tunnels, &source.name);
    provider
        .create_tunnel(ProviderContext::new(app, state), CreateTunnelInput { name })
        .map(redact_tunnel_resource)
}

fn duplicate_name(tunnels: &[TunnelResource], source_name: &str) -> String {
    for index in 0..1000 {
        let suffix = if index == 0 {
            "-copy".to_string()
        } else {
            format!("-copy{}", index + 1)
        };
        let max_base_chars = TUNNEL_NAME_MAX_CHARS.saturating_sub(suffix.chars().count());
        let base = source_name.chars().take(max_base_chars).collect::<String>();
        let candidate = format!("{base}{suffix}");
        if normalize_tunnel_name(&candidate).is_ok()
            && !tunnels
                .iter()
                .any(|item| item.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
    }
    "connection-copy".into()
}

pub fn delete_tunnel(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    id: &str,
    remote: bool,
) -> AppResult<()> {
    let before = state.config.snapshot()?;
    remove_connection_order_key(state, provider_id, id)?;
    if let Err(error) =
        get(provider_id)?.delete_tunnel(ProviderContext::new(app, state), id, remote)
    {
        state.config.replace(before);
        return Err(error);
    }
    watchdog_relay::release(state, provider_id, id);
    state.provider_runtime.remove(provider_id, id);
    Ok(())
}

fn remove_connection_order_key(state: &AppState, provider_id: &str, id: &str) -> AppResult<bool> {
    let key = format!("{provider_id}:{id}");
    let mut order = state.config.connection_order();
    let before = order.len();
    order.retain(|item| item != &key);
    let removed = order.len() != before;
    if removed {
        state.config.set_connection_order(order)?;
    }
    Ok(removed)
}

pub fn start_tunnel(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    id: &str,
) -> AppResult<TunnelRuntimeInfo> {
    let provider = get(provider_id)?;
    let current = provider.tunnel_status(ProviderContext::new(app, state), id)?;
    if !current.status.is_active() {
        crate::services::provider_log::clear(app, state, provider_id, id);
    }
    provider.start_tunnel(ProviderContext::new(app, state), id)
}

pub fn stop_tunnel(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    id: &str,
) -> AppResult<TunnelRuntimeInfo> {
    get(provider_id)?.stop_tunnel(ProviderContext::new(app, state), id)
}

pub fn tunnel_status(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    id: &str,
) -> AppResult<TunnelRuntimeInfo> {
    get(provider_id)?.tunnel_status(ProviderContext::new(app, state), id)
}

pub fn tunnel_logs(
    _app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    id: &str,
) -> AppResult<Vec<String>> {
    Ok(crate::services::provider_log::logs(state, provider_id, id))
}

pub fn metrics(app: &AppHandle, state: &AppState, provider_id: &str) -> AppResult<ProviderMetrics> {
    get(provider_id)?.metrics(ProviderContext::new(app, state))
}

pub fn cleanup_on_start(app: &AppHandle, state: &AppState) {
    for provider in all() {
        if let Err(error) = provider.cleanup_on_start(ProviderContext::new(app, state)) {
            let provider_id = provider.descriptor().id;
            let message = format!("provider {provider_id} startup cleanup failed: {error}");
            crate::diag::warn(
                &crate::diag::provider_scope(&provider_id),
                format!("startup cleanup failed: {error}"),
            );
            watchdog_log::danger(app, state, Some("provider_startup_cleanup_failed"), message);
        }
    }
}

pub fn auto_connect(app: &AppHandle, state: &AppState) {
    if !state.config.settings().auto_connect {
        return;
    }
    for provider in all() {
        if let Err(error) = provider.auto_connect(ProviderContext::new(app, state)) {
            let provider_id = provider.descriptor().id;
            let message = format!("provider {provider_id} auto-connect failed: {error}");
            crate::diag::warn(
                &crate::diag::provider_scope(&provider_id),
                format!("auto-connect failed: {error}"),
            );
            watchdog_log::warning(app, state, Some("provider_auto_connect_failed"), message);
        }
    }
}

pub fn active_tunnel_count(state: &AppState) -> usize {
    state.provider_runtime.active_count() + active_frp_tunnel_count(state)
}

fn active_frp_tunnel_count(state: &AppState) -> usize {
    let frp = frp_state(state);
    let count = frp
        .runtime
        .lock()
        .instances
        .values()
        .filter(|instance| instance.status.is_active())
        .count();
    count
}

pub fn settings_changed(
    app: &AppHandle,
    state: &AppState,
    previous: &crate::domain::AppSettings,
    next: &crate::domain::AppSettings,
) {
    for provider in all() {
        provider.settings_changed(ProviderContext::new(app, state), previous, next);
    }
    if previous.traffic_stats_enabled != next.traffic_stats_enabled {
        restart_active_tunnels_after_traffic_setting_change(app, state);
    }
}

fn restart_active_tunnels_after_traffic_setting_change(app: &AppHandle, state: &AppState) {
    let targets = traffic_setting_restart_targets(state);
    for (provider_id, tunnel_id) in targets {
        let app = app.clone();
        let state = state.clone();
        thread::spawn(move || {
            restart_tunnel_after_traffic_setting_change(&app, &state, &provider_id, &tunnel_id);
        });
    }
}

fn traffic_setting_restart_targets(state: &AppState) -> Vec<(String, String)> {
    let mut seen = HashSet::new();
    let mut targets = Vec::new();

    for (provider_id, tunnel_id) in state.provider_runtime.active_keys() {
        if should_restart_for_traffic_setting(state, &provider_id, &tunnel_id)
            && seen.insert((provider_id.clone(), tunnel_id.clone()))
        {
            targets.push((provider_id, tunnel_id));
        }
    }

    for tunnel_id in running_frp_tunnel_ids(state) {
        let provider_id = FRP_PROVIDER_ID.to_string();
        if should_restart_for_traffic_setting(state, &provider_id, &tunnel_id)
            && seen.insert((provider_id.clone(), tunnel_id.clone()))
        {
            targets.push((provider_id, tunnel_id));
        }
    }

    targets
}

fn running_frp_tunnel_ids(state: &AppState) -> Vec<String> {
    let frp = frp_state(state);
    let mut ids = frp
        .runtime
        .lock()
        .instances
        .iter()
        .filter(|(_, instance)| instance.status.is_running())
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn should_restart_for_traffic_setting(
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
) -> bool {
    if provider_id == FRP_PROVIDER_ID {
        return frp_state(state).runtime.status(tunnel_id).is_running();
    }
    let Ok(provider) = get(provider_id) else {
        return false;
    };
    let capabilities = provider.descriptor().capabilities;
    if !capabilities.local_runtime || !capabilities.named_tunnels || !capabilities.traffic_stats {
        return false;
    }
    matches!(
        state.provider_runtime.info(provider_id, tunnel_id).status,
        TunnelRuntimeState::Running | TunnelRuntimeState::Warning
    )
}

fn restart_tunnel_after_traffic_setting_change(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
) {
    provider_log::emit_system(
        app,
        state,
        provider_id,
        tunnel_id,
        "traffic statistics setting changed; restarting connection",
    );
    if let Err(error) = stop_tunnel(app, state, provider_id, tunnel_id) {
        provider_log::emit_system(
            app,
            state,
            provider_id,
            tunnel_id,
            format!("traffic statistics restart failed while stopping: {error}"),
        );
        return;
    }
    let info = wait_for_inactive_target(
        app,
        state,
        provider_id,
        tunnel_id,
        TRAFFIC_SETTING_RESTART_TIMEOUT,
        TRAFFIC_SETTING_RESTART_POLL,
    );
    match info.status {
        TunnelRuntimeState::Stopped => {}
        TunnelRuntimeState::Errored => {
            provider_log::emit_system(
                app,
                state,
                provider_id,
                tunnel_id,
                format!("traffic statistics restart aborted: {}", info.message),
            );
            return;
        }
        _ => {
            provider_log::emit_system(
                app,
                state,
                provider_id,
                tunnel_id,
                "traffic statistics restart timed out while waiting for stop",
            );
            return;
        }
    }
    if let Err(error) = start_tunnel(app, state, provider_id, tunnel_id) {
        provider_log::emit_system(
            app,
            state,
            provider_id,
            tunnel_id,
            format!("traffic statistics restart failed while starting: {error}"),
        );
    }
}

fn wait_for_inactive_target(
    app: &AppHandle,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    timeout: Duration,
    poll: Duration,
) -> TunnelRuntimeInfo {
    let deadline = Instant::now() + timeout;
    loop {
        let info = tunnel_status(app, state, provider_id, tunnel_id).unwrap_or_else(|error| {
            TunnelRuntimeInfo {
                provider_id: provider_id.to_string(),
                tunnel_id: tunnel_id.to_string(),
                status: TunnelRuntimeState::Errored,
                pid: None,
                message: error.to_string(),
                details: empty_details(),
            }
        });
        if !info.status.is_active() || Instant::now() >= deadline {
            return info;
        }
        thread::sleep(poll);
    }
}

pub fn stop_active_tunnels(app: &AppHandle, state: &AppState) {
    let targets = active_stop_targets(state);
    let stop_results = spawn_stop_requests(app, state, &targets);
    if let Err(error) = frpc_service::stop_all(app, state) {
        watchdog_log::danger(
            app,
            state,
            Some("connection_stop_failed"),
            format!("provider {FRP_PROVIDER_ID} stop all failed: {error}"),
        );
    }
    let remaining =
        wait_for_inactive_targets(state, &targets, exit_stop_wait_timeout(state, &targets));
    for (provider_id, tunnel_id, result) in stop_results.try_iter() {
        if let Err(error) = result {
            watchdog_log::danger(
                app,
                state,
                Some("connection_stop_failed"),
                format!("provider {provider_id} connection {tunnel_id} stop failed: {error}"),
            );
        }
    }
    if !remaining.is_empty() {
        for (provider_id, tunnel_id) in &remaining {
            provider_log::emit_system(
                app,
                state,
                provider_id,
                tunnel_id,
                "exit stop timed out while waiting for connection cleanup",
            );
        }
        watchdog_log::warning(
            app,
            state,
            Some("connection_stop_timeout"),
            format!(
                "timed out while waiting for connections to stop before exit: {}",
                remaining
                    .into_iter()
                    .map(|(provider_id, tunnel_id)| format!("{provider_id}:{tunnel_id}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
    process_watchdog::shutdown(app, state);
}

type StopResult = (String, String, AppResult<()>);

fn spawn_stop_requests(
    app: &AppHandle,
    state: &AppState,
    targets: &[(String, String)],
) -> mpsc::Receiver<StopResult> {
    let (tx, rx) = mpsc::channel();
    for (provider_id, tunnel_id) in targets.iter().cloned() {
        let app = app.clone();
        let state = state.clone();
        let tx = tx.clone();
        thread::spawn(move || {
            let result = stop_tunnel(&app, &state, &provider_id, &tunnel_id).map(|_| ());
            let _ = tx.send((provider_id, tunnel_id, result));
        });
    }
    drop(tx);
    rx
}

fn exit_stop_wait_timeout(state: &AppState, targets: &[(String, String)]) -> Duration {
    targets
        .iter()
        .filter_map(|(provider_id, tunnel_id)| {
            state.provider_runtime.stop_timeout(provider_id, tunnel_id)
        })
        .map(|timeout| timeout.saturating_add(EXIT_STOP_WAIT_MARGIN))
        .max()
        .unwrap_or(EXIT_STOP_WAIT_TIMEOUT)
        .max(EXIT_STOP_WAIT_TIMEOUT)
}

fn wait_for_inactive_targets(
    state: &AppState,
    targets: &[(String, String)],
    timeout: Duration,
) -> Vec<(String, String)> {
    let deadline = Instant::now() + timeout;
    loop {
        let active = active_targets_subset(state, targets);
        if active.is_empty() || Instant::now() >= deadline {
            return active;
        }
        thread::sleep(EXIT_STOP_WAIT_POLL);
    }
}

fn active_targets_subset(state: &AppState, targets: &[(String, String)]) -> Vec<(String, String)> {
    targets
        .iter()
        .filter(|(provider_id, tunnel_id)| {
            state
                .provider_runtime
                .info(provider_id, tunnel_id)
                .status
                .is_active()
        })
        .cloned()
        .collect()
}

fn active_stop_targets(state: &AppState) -> Vec<(String, String)> {
    let active = state.provider_runtime.active_keys();
    let mut seen = HashSet::new();
    let mut targets = Vec::new();
    for provider in &PROVIDERS {
        let mut ids = active
            .iter()
            .filter(|(provider_id, tunnel_id)| {
                provider_id == provider.id && seen.insert((provider_id.clone(), tunnel_id.clone()))
            })
            .map(|(_, tunnel_id)| tunnel_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        targets.extend(
            ids.into_iter()
                .map(|tunnel_id| (provider.id.to_string(), tunnel_id)),
        );
    }
    targets
}

pub fn runtime_metrics(app: &AppHandle, state: &AppState) -> RuntimeMetrics {
    let providers = all()
        .filter_map(|provider| provider.metrics(ProviderContext::new(app, state)).ok())
        .collect::<Vec<_>>();
    RuntimeMetrics {
        collected_at: crate::domain::now_ms(),
        app_memory_bytes: process_metrics::memory_usage_for_process_ids(app_process_ids(state)),
        download_bytes: providers.iter().map(|item| item.download_bytes).sum(),
        upload_bytes: providers.iter().map(|item| item.upload_bytes).sum(),
        download_speed: providers.iter().map(|item| item.download_speed).sum(),
        upload_speed: providers.iter().map(|item| item.upload_speed).sum(),
        total_bytes: providers.iter().map(|item| item.total_bytes).sum(),
        providers,
    }
}

pub fn runtime_traffic_summary(_app: &AppHandle, state: &AppState) -> RuntimeTrafficSummary {
    let mut summary = RuntimeTrafficSummary {
        collected_at: crate::domain::now_ms(),
        app_memory_bytes: process_metrics::memory_usage_for_process_ids(app_process_ids(state)),
        has_active_tunnels: active_tunnel_count(state) > 0,
        ..Default::default()
    };
    if !state.config.settings().traffic_stats_enabled {
        return summary;
    }

    append_watchdog_relay_traffic_summary(state, &mut summary);
    summary.total_bytes = summary.download_bytes + summary.upload_bytes;
    summary
}

fn append_watchdog_relay_traffic_summary(state: &AppState, summary: &mut RuntimeTrafficSummary) {
    for (provider_id, tunnel_id, stats) in state.watchdog_relay.all_stats() {
        for stat in stats {
            summary.has_active_tunnels = true;
            let metric_tunnel_id = format!("{provider_id}:{tunnel_id}:{}", stat.endpoint_id);
            push_metric_samples(
                &mut summary.history,
                &metric_tunnel_id,
                &stat.endpoint_name,
                stat.history,
            );
            summary.download_bytes += stat.download_bytes;
            summary.upload_bytes += stat.upload_bytes;
            summary.download_speed += stat.download_speed;
            summary.upload_speed += stat.upload_speed;
        }
    }
}

fn push_metric_samples(
    target: &mut Vec<MetricSample>,
    tunnel_id: &str,
    tunnel_name: &str,
    samples: impl IntoIterator<Item = MetricSample>,
) {
    target.extend(samples.into_iter().map(|mut sample| {
        sample.tunnel_id = tunnel_id.to_string();
        sample.tunnel_name = tunnel_name.to_string();
        sample
    }));
}

fn app_process_ids(state: &AppState) -> Vec<u32> {
    let mut seen = HashSet::new();
    let mut pids = Vec::new();
    push_pid(&mut seen, &mut pids, std::process::id());
    if let Some(pid) = state.process_watchdog.pid() {
        push_pid(&mut seen, &mut pids, pid);
    }
    for provider in all() {
        for pid in provider.process_ids(state) {
            push_pid(&mut seen, &mut pids, pid);
        }
    }
    pids
}

fn push_pid(seen: &mut HashSet<u32>, pids: &mut Vec<u32>, pid: u32) {
    if seen.insert(pid) {
        pids.push(pid);
    }
}

pub fn watchdog_event<R: Runtime>(app: &AppHandle<R>, state: &AppState, event: WatchdogEvent) {
    if let WatchdogEvent::RelayError {
        provider_id,
        tunnel_id,
        endpoint_name,
        endpoint_type,
        target,
        message,
        ..
    } = event
    {
        let endpoint = if endpoint_name.trim().is_empty() {
            endpoint_type.clone()
        } else {
            format!("{endpoint_name} ({endpoint_type})")
        };
        let message = format!("relay endpoint {endpoint} cannot reach target {target}: {message}");
        provider_log::emit_system(app, state, &provider_id, &tunnel_id, message.clone());
        watchdog_log::warning(
            app,
            state,
            Some("relay_endpoint_unreachable"),
            format!("provider {provider_id} connection {tunnel_id}: {message}"),
        );
        return;
    }
    if let WatchdogEvent::RelayStats {
        provider_id,
        tunnel_id,
        endpoints,
    } = event
    {
        watchdog_relay::update_stats_if_active(app, state, provider_id, tunnel_id, endpoints);
        return;
    }
    let Some(provider_id) = event.provider_id().map(str::to_string) else {
        handle_platform_watchdog_event(app, state, event);
        return;
    };
    match get_registration(&provider_id) {
        Ok(provider) => {
            if let Some((level, code, message)) = provider_watchdog_alert(&event) {
                match level {
                    "danger" => watchdog_log::danger(app, state, Some(code), message),
                    _ => watchdog_log::warning(app, state, Some(code), message),
                }
            }
            provider.watchdog.event(app, state, event);
        }
        Err(_) => {
            let message = format!("watchdog ignored unknown provider event: {provider_id}");
            crate::diag::warn(
                "watchdog",
                format!("ignored unknown provider event: {provider_id}"),
            );
            watchdog_log::warning(app, state, Some("unknown_provider_event"), message);
        }
    }
}

fn provider_watchdog_alert(event: &WatchdogEvent) -> Option<(&'static str, &'static str, String)> {
    match event {
        WatchdogEvent::ProcessError {
            provider_id,
            tunnel_id,
            message,
        } => Some((
            "danger",
            "provider_process_error",
            format!("provider {provider_id} connection {tunnel_id} watchdog error: {message}"),
        )),
        WatchdogEvent::ProcessExit {
            provider_id,
            tunnel_id,
            success,
            forced: _,
            code,
            cleanup_success: _,
            cleanup_error,
        } if !success => Some((
            "danger",
            "provider_process_exited",
            format!(
                "provider {provider_id} connection {tunnel_id} exited unexpectedly{}{}",
                code.map(|code| format!(" with code {code}"))
                    .unwrap_or_default(),
                cleanup_error
                    .as_ref()
                    .filter(|error| !error.trim().is_empty())
                    .map(|error| format!("; cleanup: {error}"))
                    .unwrap_or_default()
            ),
        )),
        WatchdogEvent::ProcessExit {
            provider_id,
            tunnel_id,
            cleanup_success: Some(false),
            cleanup_error,
            ..
        } => Some((
            "danger",
            "provider_cleanup_failed",
            format!(
                "provider {provider_id} connection {tunnel_id} cleanup was not confirmed{}",
                cleanup_error
                    .as_ref()
                    .filter(|error| !error.trim().is_empty())
                    .map(|error| format!(": {error}"))
                    .unwrap_or_default()
            ),
        )),
        _ => None,
    }
}

fn handle_platform_watchdog_event<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    event: WatchdogEvent,
) {
    match event {
        WatchdogEvent::OwnerAttached { pid, active } => watchdog_log::info(
            app,
            state,
            format!("watchdog owner reattached; watchdog pid={pid}; active processes={active}"),
        ),
        WatchdogEvent::OwnerLost { timeout_ms } => watchdog_log::warning(
            app,
            state,
            Some("owner_lost"),
            format!(
                "watchdog owner lost; waiting {} seconds for heartbeat before cleanup",
                timeout_ms / 1000
            ),
        ),
        WatchdogEvent::OwnerHeartbeatExpired => watchdog_log::danger(
            app,
            state,
            Some("owner_heartbeat_expired"),
            "watchdog heartbeat timed out; cleanup started",
        ),
        WatchdogEvent::ProcessStarted { .. }
        | WatchdogEvent::ProcessSnapshot { .. }
        | WatchdogEvent::ProcessLog { .. }
        | WatchdogEvent::ProcessExit { .. }
        | WatchdogEvent::ProcessError { .. }
        | WatchdogEvent::RelayError { .. }
        | WatchdogEvent::RelayStats { .. } => {}
    }
}

pub fn watchdog_eof<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    if active_tunnel_count(state) > 0 {
        watchdog_log::danger(
            app,
            state,
            Some("watchdog_eof"),
            "watchdog event stream closed; reconciling active connections",
        );
    } else {
        watchdog_log::info(
            app,
            state,
            "watchdog event stream closed; no active managed connections",
        );
    }
    for provider in &PROVIDERS {
        provider.watchdog.eof(app, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::AppData;

    use crate::providers::frp::runtime_state::FrpcStatus;

    #[test]
    fn traffic_setting_restart_targets_running_frp_runtime() {
        let state = AppState::default();
        {
            let frp = frp_state(&state);
            let mut rt = frp.runtime.lock();
            rt.instance_mut("frp-1").status = FrpcStatus::Running;
            rt.instance_mut("frp-stopped").status = FrpcStatus::Stopped;
        }

        assert!(should_restart_for_traffic_setting(
            &state,
            FRP_PROVIDER_ID,
            "frp-1"
        ));
        assert!(!should_restart_for_traffic_setting(
            &state,
            FRP_PROVIDER_ID,
            "frp-stopped"
        ));
        assert_eq!(
            traffic_setting_restart_targets(&state),
            vec![(FRP_PROVIDER_ID.to_string(), "frp-1".to_string())]
        );
    }

    #[test]
    fn traffic_setting_restart_targets_running_provider_runtime_connections() {
        let state = AppState::default();
        state.provider_runtime.mark_running(
            CLOUDFLARE_PROVIDER_ID,
            "cf-1",
            std::process::id(),
            "running",
        );

        assert!(should_restart_for_traffic_setting(
            &state,
            CLOUDFLARE_PROVIDER_ID,
            "cf-1"
        ));
        assert!(!should_restart_for_traffic_setting(
            &state,
            CLOUDFLARE_PROVIDER_ID,
            "missing"
        ));
        assert_eq!(
            traffic_setting_restart_targets(&state),
            vec![(CLOUDFLARE_PROVIDER_ID.to_string(), "cf-1".to_string())]
        );
    }

    #[test]
    fn active_stop_targets_follow_provider_order_and_tunnel_id_order() {
        let state = AppState::default();
        state.provider_runtime.mark_status(
            PINGGY_PROVIDER_ID,
            "z",
            TunnelRuntimeState::Running,
            "running",
        );
        state.provider_runtime.mark_status(
            CLOUDFLARE_PROVIDER_ID,
            "b",
            TunnelRuntimeState::Running,
            "running",
        );
        state.provider_runtime.mark_status(
            CLOUDFLARE_PROVIDER_ID,
            "a",
            TunnelRuntimeState::Running,
            "running",
        );
        state.provider_runtime.mark_status(
            NGROK_PROVIDER_ID,
            "n",
            TunnelRuntimeState::Stopped,
            "stopped",
        );

        assert_eq!(
            active_stop_targets(&state),
            vec![
                (CLOUDFLARE_PROVIDER_ID.to_string(), "a".to_string()),
                (CLOUDFLARE_PROVIDER_ID.to_string(), "b".to_string()),
                (PINGGY_PROVIDER_ID.to_string(), "z".to_string()),
            ]
        );
    }

    #[test]
    fn active_targets_subset_only_returns_requested_active_connections() {
        let state = AppState::default();
        state.provider_runtime.mark_status(
            CLOUDFLARE_PROVIDER_ID,
            "active",
            TunnelRuntimeState::Running,
            "running",
        );
        state.provider_runtime.mark_status(
            CLOUDFLARE_PROVIDER_ID,
            "stopped",
            TunnelRuntimeState::Stopped,
            "stopped",
        );
        state.provider_runtime.mark_status(
            NGROK_PROVIDER_ID,
            "errored",
            TunnelRuntimeState::Errored,
            "failed",
        );
        state.provider_runtime.mark_status(
            PINGGY_PROVIDER_ID,
            "not-requested",
            TunnelRuntimeState::Running,
            "running",
        );

        let targets = vec![
            (CLOUDFLARE_PROVIDER_ID.to_string(), "active".to_string()),
            (CLOUDFLARE_PROVIDER_ID.to_string(), "stopped".to_string()),
            (NGROK_PROVIDER_ID.to_string(), "errored".to_string()),
        ];

        assert_eq!(
            active_targets_subset(&state, &targets),
            vec![(CLOUDFLARE_PROVIDER_ID.to_string(), "active".to_string())]
        );
    }

    #[test]
    fn active_tunnel_count_includes_frp_runtime() {
        let state = AppState::default();
        state.provider_runtime.mark_status(
            PINGGY_PROVIDER_ID,
            "pinggy-1",
            TunnelRuntimeState::Running,
            "running",
        );
        {
            let frp = frp_state(&state);
            let mut rt = frp.runtime.lock();
            rt.instance_mut("frp-1").status = FrpcStatus::Running;
        }

        assert_eq!(active_tunnel_count(&state), 2);
    }

    #[test]
    fn remove_connection_order_key_only_removes_deleted_connection() {
        let state = AppState::default();
        state.config.replace(AppData {
            connection_order: vec![
                format!("{CLOUDFLARE_PROVIDER_ID}:keep"),
                format!("{NGROK_PROVIDER_ID}:delete-me"),
                format!("{PINGGY_PROVIDER_ID}:delete-me"),
            ],
            ..Default::default()
        });

        assert!(remove_connection_order_key(&state, NGROK_PROVIDER_ID, "delete-me").unwrap());

        assert_eq!(
            state.config.connection_order(),
            vec![
                format!("{CLOUDFLARE_PROVIDER_ID}:keep"),
                format!("{PINGGY_PROVIDER_ID}:delete-me"),
            ]
        );
        assert!(!remove_connection_order_key(&state, NGROK_PROVIDER_ID, "missing").unwrap());
    }

    #[test]
    fn provider_status_redacts_sensitive_details() {
        let status = redact_provider_status(ProviderStatus {
            provider_id: "provider-a".into(),
            available: false,
            version: None,
            message: "failed --token secret".into(),
            details: serde_json::json!({
                "Authorization": "Bearer abc",
                "url": "https://api.example.test?token=query-token&ok=1"
            }),
        });

        assert!(status.message.contains("--token ***"));
        assert_eq!(status.details["Authorization"], "***");
        assert_eq!(
            status.details["url"],
            "https://api.example.test?token=***&ok=1"
        );
        assert!(!status.message.contains("secret"));
        assert!(!status.details.to_string().contains("Bearer abc"));
        assert!(!status.details.to_string().contains("query-token"));
    }

    #[test]
    fn provider_command_output_redacts_sensitive_text() {
        let output = redact_provider_command_output(ProviderCommandOutput {
            success: false,
            stdout: "ok --authtoken secret".into(),
            stderr: "Authorization=Bearer abc".into(),
        });

        assert!(output.stdout.contains("--authtoken ***"));
        assert_eq!(output.stderr, "Authorization=***");
        assert!(!output.stdout.contains("secret"));
        assert!(!output.stderr.contains("Bearer abc"));
    }

    #[test]
    fn tunnel_resource_redacts_sensitive_metadata() {
        let resource = redact_tunnel_resource(TunnelResource {
            id: "conn-1".into(),
            provider_id: "provider-a".into(),
            name: "conn".into(),
            provider_tunnel_id: String::new(),
            credentials_ref: String::new(),
            config_file: String::new(),
            ingress: Vec::new(),
            created_at: 0,
            updated_at: 0,
            metadata: serde_json::json!({
                "authtoken": "secret-token",
                "nested": {
                    "token": "nested-token"
                }
            }),
        });

        assert_eq!(resource.metadata["authtoken"], "***");
        assert_eq!(resource.metadata["nested"]["token"], "***");
        assert!(!resource.metadata.to_string().contains("secret-token"));
        assert!(!resource.metadata.to_string().contains("nested-token"));
    }

    #[test]
    fn provider_watchdog_alert_reports_abnormal_exit_and_cleanup_failure() {
        let exit = provider_watchdog_alert(&WatchdogEvent::ProcessExit {
            provider_id: "provider-a".into(),
            tunnel_id: "conn-1".into(),
            success: false,
            forced: false,
            code: Some(101),
            cleanup_success: Some(true),
            cleanup_error: None,
        })
        .unwrap();
        assert_eq!(exit.1, "provider_process_exited");
        assert!(exit.2.contains("provider-a"));
        assert!(exit.2.contains("code 101"));

        let cleanup = provider_watchdog_alert(&WatchdogEvent::ProcessExit {
            provider_id: "provider-a".into(),
            tunnel_id: "conn-1".into(),
            success: true,
            forced: false,
            code: Some(0),
            cleanup_success: Some(false),
            cleanup_error: Some("remote cleanup failed".into()),
        })
        .unwrap();
        assert_eq!(cleanup.1, "provider_cleanup_failed");
        assert!(cleanup.2.contains("remote cleanup failed"));
    }

    #[test]
    fn provider_watchdog_alert_ignores_clean_exit() {
        assert!(provider_watchdog_alert(&WatchdogEvent::ProcessExit {
            provider_id: "provider-a".into(),
            tunnel_id: "conn-1".into(),
            success: true,
            forced: false,
            code: Some(0),
            cleanup_success: Some(true),
            cleanup_error: None,
        })
        .is_none());
    }

    #[test]
    fn exit_stop_wait_timeout_uses_registered_cleanup_timeout() {
        let state = AppState::default();
        let targets = vec![
            (CLOUDFLARE_PROVIDER_ID.to_string(), "slow".to_string()),
            (PINGGY_PROVIDER_ID.to_string(), "fast".to_string()),
        ];
        state.provider_runtime.set_stop_timeout(
            CLOUDFLARE_PROVIDER_ID,
            "slow",
            Duration::from_secs(42),
        );

        assert_eq!(
            exit_stop_wait_timeout(&state, &targets),
            Duration::from_secs(44)
        );
    }

    #[test]
    fn exit_stop_wait_timeout_has_minimum_floor() {
        let state = AppState::default();
        let targets = vec![(PINGGY_PROVIDER_ID.to_string(), "fast".to_string())];
        state
            .provider_runtime
            .set_stop_timeout(PINGGY_PROVIDER_ID, "fast", Duration::from_secs(1));

        assert_eq!(
            exit_stop_wait_timeout(&state, &targets),
            EXIT_STOP_WAIT_TIMEOUT
        );
    }
}
