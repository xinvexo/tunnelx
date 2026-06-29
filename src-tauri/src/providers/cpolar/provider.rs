use super::data;
use super::domain::{CpolarEndpoint, CpolarTunnel};
use super::runtime;
use crate::error::AppResult;
use crate::providers::contract::{
    CreateTunnelInput, IngressRule, ProviderCapabilities, ProviderContext, ProviderDescriptor,
    ProviderMetrics, ProviderStatus, TunnelMetrics, TunnelProvider, TunnelResource,
    TunnelRuntimeInfo, CPOLAR_PROVIDER_ID,
};
use crate::services::watchdog_relay;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{AppHandle, Runtime};
use tunnelx_watchdog_protocol::WatchdogEvent;

pub struct CpolarProvider;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CpolarMetadata {
    #[serde(default)]
    authtoken: String,
    #[serde(default)]
    region: String,
    #[serde(default)]
    endpoints: Vec<CpolarEndpoint>,
}

impl CpolarProvider {
    pub(crate) fn handle_watchdog_event<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        state: &crate::state::AppState,
        event: WatchdogEvent,
    ) {
        runtime::handle_watchdog_event(app, state, event);
    }

    pub(crate) fn handle_watchdog_eof<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        state: &crate::state::AppState,
    ) {
        runtime::handle_watchdog_eof(app, state);
    }
}

impl TunnelProvider for CpolarProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: CPOLAR_PROVIDER_ID.into(),
            name: "Cpolar".into(),
            summary: "Expose local HTTP/TCP services through the cpolar agent".into(),
            capabilities: ProviderCapabilities {
                named_tunnels: true,
                credentials: true,
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
        Ok(runtime::status(ctx.app, ctx.state))
    }

    fn cleanup_on_start(&self, ctx: ProviderContext<'_>) -> AppResult<()> {
        runtime::cleanup_on_start(ctx.app, ctx.state)
    }

    fn list_tunnels(&self, ctx: ProviderContext<'_>) -> AppResult<Vec<TunnelResource>> {
        Ok(data::data(ctx.state)
            .tunnels
            .into_iter()
            .map(Into::into)
            .collect())
    }

    fn create_tunnel(
        &self,
        ctx: ProviderContext<'_>,
        input: CreateTunnelInput,
    ) -> AppResult<TunnelResource> {
        data::create_tunnel(ctx.app, ctx.state, input.name).map(Into::into)
    }

    fn update_tunnel(
        &self,
        ctx: ProviderContext<'_>,
        tunnel: TunnelResource,
    ) -> AppResult<TunnelResource> {
        data::update_tunnel(ctx.app, ctx.state, tunnel.into()).map(Into::into)
    }

    fn delete_tunnel(&self, ctx: ProviderContext<'_>, id: &str, _remote: bool) -> AppResult<()> {
        data::delete_tunnel(ctx.app, ctx.state, id)
    }

    fn metrics(&self, ctx: ProviderContext<'_>) -> AppResult<ProviderMetrics> {
        Ok(watchdog_relay::provider_metrics(
            ctx.state,
            CPOLAR_PROVIDER_ID,
            data::data(ctx.state).tunnels,
            cpolar_tunnel_metrics,
        ))
    }

    fn start_tunnel(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        runtime::start_tunnel(ctx.app, ctx.state, id)
    }

    fn stop_tunnel(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        runtime::stop_tunnel(ctx.app, ctx.state, id)
    }

    fn tunnel_status(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        Ok(runtime::tunnel_status(ctx.app, ctx.state, id))
    }
}

impl From<CpolarTunnel> for TunnelResource {
    fn from(tunnel: CpolarTunnel) -> Self {
        let metadata = CpolarMetadata {
            authtoken: tunnel.authtoken.clone(),
            region: tunnel.region.clone(),
            endpoints: tunnel.endpoints.clone(),
        };
        Self {
            id: tunnel.id,
            provider_id: CPOLAR_PROVIDER_ID.into(),
            name: tunnel.name,
            provider_tunnel_id: String::new(),
            credentials_ref: String::new(),
            config_file: tunnel.config_file,
            ingress: tunnel
                .endpoints
                .iter()
                .map(|endpoint| IngressRule {
                    id: endpoint.id.clone(),
                    name: endpoint.name.clone(),
                    hostname: endpoint.hostname.clone(),
                    service: endpoint.addr.clone(),
                    enabled: endpoint.enabled,
                    dns_routed: false,
                })
                .collect(),
            created_at: tunnel.created_at,
            updated_at: tunnel.updated_at,
            metadata: serde_json::to_value(metadata).unwrap_or_else(|_| json!({})),
        }
    }
}

impl From<TunnelResource> for CpolarTunnel {
    fn from(resource: TunnelResource) -> Self {
        let metadata =
            serde_json::from_value::<CpolarMetadata>(resource.metadata).unwrap_or_default();
        let endpoints = if metadata.endpoints.is_empty() {
            resource
                .ingress
                .into_iter()
                .map(|rule| CpolarEndpoint {
                    id: rule.id,
                    name: rule.name,
                    proto: "http".into(),
                    addr: rule.service,
                    hostname: rule.hostname,
                    remote_addr: String::new(),
                    enabled: rule.enabled,
                })
                .collect()
        } else {
            metadata.endpoints
        };
        Self {
            id: resource.id,
            name: resource.name,
            authtoken: metadata.authtoken,
            region: metadata.region,
            config_file: resource.config_file,
            endpoints,
            created_at: resource.created_at,
            updated_at: resource.updated_at,
        }
    }
}

fn cpolar_tunnel_metrics(
    state: &crate::state::AppState,
    tunnel: CpolarTunnel,
) -> Option<TunnelMetrics> {
    let endpoint_count = tunnel
        .endpoints
        .iter()
        .filter(|endpoint| endpoint.enabled)
        .count();
    watchdog_relay::tunnel_metrics(
        state,
        CPOLAR_PROVIDER_ID,
        tunnel.id,
        tunnel.name,
        "cpolar",
        endpoint_count,
        json!({}),
    )
}
