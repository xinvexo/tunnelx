use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    empty_details, CreateTunnelInput, ProviderCapabilities, ProviderContext, ProviderDescriptor,
    ProviderMetrics, ProviderStatus, TunnelMetrics, TunnelProvider, TunnelResource,
    TunnelRuntimeInfo, TunnelRuntimeState, FRP_PROVIDER_ID,
};
use crate::providers::frp::domain::{Profile, ProfileProxy, ProxyConfig};
use crate::providers::frp::paths;
use crate::providers::frp::runtime_state::FrpcStatus;
use crate::providers::frp::services::frpc_service;
use crate::providers::frp::services::profile_service;
use crate::providers::frp::state::frp_state;
use crate::services::{process_metrics, watchdog_relay::RelayTrafficStats};
use serde_json::json;

pub struct FrpProvider;

impl FrpProvider {
    pub(crate) fn handle_watchdog_event<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        state: &crate::state::AppState,
        event: tunnelx_watchdog_protocol::WatchdogEvent,
    ) {
        frpc_service::handle_watchdog_event(app, state, event);
    }

    pub(crate) fn handle_watchdog_eof<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        state: &crate::state::AppState,
    ) {
        frpc_service::handle_watchdog_eof(app, state);
    }
}

impl TunnelProvider for FrpProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: FRP_PROVIDER_ID.into(),
            name: "Frp".into(),
            summary: "Connect to frps through frpc and manage TCP/UDP/HTTP/HTTPS proxy tunnels"
                .into(),
            capabilities: ProviderCapabilities {
                named_tunnels: true,
                local_runtime: true,
                runtime_metrics: true,
                memory_stats: true,
                traffic_stats: true,
                version_management: true,
                ..Default::default()
            },
        }
    }

    fn status(&self, ctx: ProviderContext<'_>) -> AppResult<ProviderStatus> {
        let settings = ctx.state.config.frp_settings();
        let version = settings.active_frpc_version.clone();
        let available = version
            .as_deref()
            .and_then(|item| paths::frpc_exe(ctx.app, item).ok())
            .map(|path| path.exists())
            .unwrap_or(false);
        Ok(ProviderStatus {
            provider_id: FRP_PROVIDER_ID.into(),
            available,
            version,
            message: if available {
                "frpc is installed".into()
            } else {
                "No usable frpc version was detected".into()
            },
            details: empty_details(),
        })
    }

    fn cleanup_on_start(&self, ctx: ProviderContext<'_>) -> AppResult<()> {
        frpc_service::cleanup_orphan_frpc_processes(ctx.app, ctx.state).map(|_| ())
    }

    fn process_ids(&self, state: &crate::state::AppState) -> Vec<u32> {
        frp_process_ids(state)
    }

    fn settings_changed(
        &self,
        ctx: ProviderContext<'_>,
        previous: &crate::domain::AppSettings,
        next: &crate::domain::AppSettings,
    ) {
        if previous.traffic_stats_enabled == next.traffic_stats_enabled {
            return;
        }
        let profile_ids = {
            let frp = frp_state(ctx.state);
            let rt = frp.runtime.lock();
            rt.instances
                .iter()
                .filter(|(_, inst)| inst.status.is_running())
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
        };
        for profile_id in profile_ids {
            frpc_service::hot_reload(ctx.app, ctx.state, &profile_id);
        }
    }

    fn list_tunnels(&self, ctx: ProviderContext<'_>) -> AppResult<Vec<TunnelResource>> {
        Ok(ctx
            .state
            .config
            .frp_data()
            .profiles
            .into_iter()
            .map(frp_profile_resource)
            .collect())
    }

    fn create_tunnel(
        &self,
        ctx: ProviderContext<'_>,
        input: CreateTunnelInput,
    ) -> AppResult<TunnelResource> {
        profile_service::create_profile(ctx.app, ctx.state, input.name).map(frp_profile_resource)
    }

    fn update_tunnel(
        &self,
        ctx: ProviderContext<'_>,
        tunnel: TunnelResource,
    ) -> AppResult<TunnelResource> {
        let name = tunnel.name.trim();
        if name.is_empty() {
            return Err(AppError::Msg("Connection name is required".into()));
        }
        let mut profile = ctx.state.config.get_profile(&tunnel.id)?;
        profile.name = name.to_string();
        profile_service::update_profile(ctx.app, ctx.state, profile).map(frp_profile_resource)
    }

    fn delete_tunnel(&self, ctx: ProviderContext<'_>, id: &str, _remote: bool) -> AppResult<()> {
        profile_service::delete_profile(ctx.app, ctx.state, id)
    }

    fn metrics(&self, ctx: ProviderContext<'_>) -> AppResult<ProviderMetrics> {
        if !ctx.state.config.settings().traffic_stats_enabled {
            return Ok(ProviderMetrics::from_tunnels(
                FRP_PROVIDER_ID,
                frp_memory_usage(ctx.state),
                Vec::new(),
            ));
        }

        let profile_names = ctx
            .state
            .config
            .frp_data()
            .profiles
            .into_iter()
            .map(|profile| (profile.id, profile.name))
            .collect::<std::collections::HashMap<_, _>>();
        let tunnels = ctx
            .state
            .watchdog_relay
            .all_stats()
            .into_iter()
            .filter(|(provider_id, _, _)| provider_id == FRP_PROVIDER_ID)
            .flat_map(|(_, profile_id, stats)| {
                let profile_name = profile_names.get(&profile_id).cloned().unwrap_or_default();
                stats
                    .into_iter()
                    .map(move |stat| frp_tunnel_metrics(&profile_id, &profile_name, stat))
            })
            .collect::<Vec<_>>();

        Ok(ProviderMetrics::from_tunnels(
            FRP_PROVIDER_ID,
            frp_memory_usage(ctx.state),
            tunnels,
        ))
    }

    fn start_tunnel(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        let status = frpc_service::start(ctx.app, ctx.state, id)?;
        Ok(frp_runtime_info(ctx.state, id, status))
    }

    fn stop_tunnel(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        let status = frpc_service::stop(ctx.app, ctx.state, id)?;
        Ok(frp_runtime_info(ctx.state, id, status))
    }

    fn tunnel_status(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        Ok(frp_runtime_info(
            ctx.state,
            id,
            frp_state(ctx.state).runtime.status(id),
        ))
    }
}

fn frp_memory_usage(state: &crate::state::AppState) -> Option<u64> {
    process_metrics::memory_usage_for_process_ids(frp_process_ids(state))
}

fn frp_process_ids(state: &crate::state::AppState) -> Vec<u32> {
    let frp = frp_state(state);
    let rt = frp.runtime.lock();
    rt.instances
        .values()
        .filter_map(|instance| instance.pid)
        .collect()
}

fn frp_profile_resource(profile: Profile) -> TunnelResource {
    let proxy_count = profile.proxies.len();
    let enabled_proxy_count = profile.proxies.iter().filter(|proxy| proxy.enabled).count();
    let proxies = profile
        .proxies
        .iter()
        .map(frp_proxy_summary)
        .collect::<Vec<_>>();
    TunnelResource {
        id: profile.id.clone(),
        provider_id: FRP_PROVIDER_ID.into(),
        name: profile.name.clone(),
        provider_tunnel_id: profile.id.clone(),
        credentials_ref: String::new(),
        config_file: String::new(),
        ingress: Vec::new(),
        created_at: profile.created_at,
        updated_at: profile.updated_at,
        metadata: json!({
            "profileId": profile.id,
            "profileName": profile.name,
            "serverAddr": profile.server.server_addr,
            "serverPort": profile.server.server_port,
            "proxyCount": proxy_count,
            "enabledProxyCount": enabled_proxy_count,
            "frpcVersion": profile.frpc_version,
            "proxies": proxies,
        }),
    }
}

fn frp_proxy_summary(proxy: &ProfileProxy) -> serde_json::Value {
    let base = proxy.config.base();
    json!({
        "id": proxy.id,
        "name": base.name.clone(),
        "proxyType": proxy_config_type(&proxy.config),
        "enabled": proxy.enabled,
        "localIP": base.local_ip.clone(),
        "localPort": base.local_port,
    })
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

fn frp_runtime_info(
    state: &crate::state::AppState,
    id: &str,
    status: FrpcStatus,
) -> TunnelRuntimeInfo {
    let (pid, needs_restart) = {
        let frp = frp_state(state);
        let rt = frp.runtime.lock();
        rt.instances
            .get(id)
            .map(|instance| (instance.pid, instance.needs_restart))
            .unwrap_or((None, false))
    };
    TunnelRuntimeInfo {
        provider_id: FRP_PROVIDER_ID.into(),
        tunnel_id: id.to_string(),
        status: frp_runtime_state(status),
        pid,
        message: frp_runtime_message(status, needs_restart),
        details: frpc_service::runtime_details(state, id, status),
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

fn frp_tunnel_metrics(
    profile_id: &str,
    profile_name: &str,
    stat: RelayTrafficStats,
) -> TunnelMetrics {
    let tunnel_id = format!("{profile_id}:{}", stat.endpoint_name);
    TunnelMetrics {
        tunnel_id: tunnel_id.clone(),
        tunnel_name: stat.endpoint_name.clone(),
        tunnel_type: stat.endpoint_type.clone(),
        memory_bytes: None,
        download_bytes: stat.download_bytes,
        upload_bytes: stat.upload_bytes,
        download_speed: stat.download_speed,
        upload_speed: stat.upload_speed,
        total_bytes: stat.total_bytes,
        last_updated_at: stat.last_updated_at,
        history: stat.history.into_iter().collect(),
        metadata: json!({
            "profileId": profile_id,
            "profileName": profile_name,
            "target": stat.target,
            "relayHost": stat.relay_host,
            "relayPort": stat.relay_port,
        }),
    }
}
