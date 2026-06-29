use super::domain::{CloudflareIngressRule, CloudflareTunnel};
use super::services;
use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    empty_details, CreateTunnelInput, IngressRule, ProviderCapabilities, ProviderCommandOutput,
    ProviderContext, ProviderDescriptor, ProviderMetrics, ProviderStatus, TunnelMetrics,
    TunnelProvider, TunnelResource, TunnelRuntimeInfo, CLOUDFLARE_PROVIDER_ID,
};
use crate::services::watchdog_relay;

pub struct CloudflareProvider;

impl CloudflareProvider {
    pub(crate) fn handle_watchdog_event<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        state: &crate::state::AppState,
        event: tunnelx_watchdog_protocol::WatchdogEvent,
    ) {
        super::tunnel::runtime::handle_watchdog_event(app, state, event);
    }

    pub(crate) fn handle_watchdog_eof<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        state: &crate::state::AppState,
    ) {
        super::tunnel::runtime::handle_watchdog_eof(app, state);
    }
}

impl TunnelProvider for CloudflareProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            id: CLOUDFLARE_PROVIDER_ID.into(),
            name: "Cloudflare Tunnel".into(),
            summary: "Manage named tunnels, DNS routes, and ingress through cloudflared".into(),
            capabilities: ProviderCapabilities {
                account_login: true,
                named_tunnels: true,
                credentials: false,
                dns_routes: true,
                ingress: true,
                local_runtime: true,
                runtime_metrics: true,
                memory_stats: true,
                traffic_stats: true,
                version_management: true,
            },
        }
    }

    fn status(&self, ctx: ProviderContext<'_>) -> AppResult<ProviderStatus> {
        let status = services::status(ctx.app, ctx.state)?;
        Ok(ProviderStatus {
            provider_id: CLOUDFLARE_PROVIDER_ID.into(),
            available: status.available,
            version: status.version.clone(),
            message: if status.available {
                "cloudflared is available".into()
            } else {
                "cloudflared is unavailable".into()
            },
            details: serde_json::to_value(status).unwrap_or_else(|_| empty_details()),
        })
    }

    fn cleanup_on_start(&self, ctx: ProviderContext<'_>) -> AppResult<()> {
        services::cleanup_on_start(ctx.app, ctx.state)
    }

    fn login(&self, _ctx: ProviderContext<'_>) -> AppResult<ProviderCommandOutput> {
        Err(AppError::Msg(
            "Cloudflare authorization requires a selected connection".into(),
        ))
    }

    fn list_tunnels(&self, ctx: ProviderContext<'_>) -> AppResult<Vec<TunnelResource>> {
        Ok(services::data(ctx.state)
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
        services::create_tunnel(ctx.app, ctx.state, input.name).map(Into::into)
    }

    fn update_tunnel(
        &self,
        ctx: ProviderContext<'_>,
        tunnel: TunnelResource,
    ) -> AppResult<TunnelResource> {
        services::save_tunnel(ctx.app, ctx.state, tunnel.into()).map(Into::into)
    }

    fn delete_tunnel(&self, ctx: ProviderContext<'_>, id: &str, remote: bool) -> AppResult<()> {
        services::delete_tunnel(ctx.app, ctx.state, id, remote, false).map(|_| ())
    }

    fn metrics(&self, ctx: ProviderContext<'_>) -> AppResult<ProviderMetrics> {
        Ok(watchdog_relay::provider_metrics(
            ctx.state,
            CLOUDFLARE_PROVIDER_ID,
            services::data(ctx.state).tunnels,
            cloudflare_tunnel_metrics,
        ))
    }

    fn start_tunnel(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        services::start_tunnel(ctx.app, ctx.state, id)
    }

    fn stop_tunnel(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        services::stop_tunnel(ctx.app, ctx.state, id)
    }

    fn tunnel_status(&self, ctx: ProviderContext<'_>, id: &str) -> AppResult<TunnelRuntimeInfo> {
        Ok(services::tunnel_status(ctx.app, ctx.state, id))
    }
}

fn cloudflare_tunnel_metrics(
    state: &crate::state::AppState,
    tunnel: CloudflareTunnel,
) -> Option<TunnelMetrics> {
    let endpoint_count = tunnel.ingress.iter().filter(|rule| rule.enabled).count();
    watchdog_relay::tunnel_metrics(
        state,
        CLOUDFLARE_PROVIDER_ID,
        tunnel.id,
        tunnel.name,
        "cloudflared",
        endpoint_count,
        serde_json::json!({
            "providerTunnelId": tunnel.tunnel_id,
        }),
    )
}

impl From<CloudflareIngressRule> for IngressRule {
    fn from(rule: CloudflareIngressRule) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            hostname: rule.hostname,
            service: rule.service,
            enabled: rule.enabled,
            dns_routed: rule.dns_routed,
        }
    }
}

impl From<IngressRule> for CloudflareIngressRule {
    fn from(rule: IngressRule) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            hostname: rule.hostname,
            service: rule.service,
            runtime_http_host_header: String::new(),
            enabled: rule.enabled,
            dns_routed: rule.dns_routed,
        }
    }
}

impl From<CloudflareTunnel> for TunnelResource {
    fn from(tunnel: CloudflareTunnel) -> Self {
        Self {
            id: tunnel.id,
            provider_id: CLOUDFLARE_PROVIDER_ID.into(),
            name: tunnel.name,
            provider_tunnel_id: tunnel.tunnel_id,
            credentials_ref: tunnel.credentials_file,
            config_file: tunnel.config_file,
            ingress: tunnel.ingress.into_iter().map(Into::into).collect(),
            created_at: tunnel.created_at,
            updated_at: tunnel.updated_at,
            metadata: serde_json::json!({
                "account": tunnel.account,
                "certFile": tunnel.cert_file,
            }),
        }
    }
}

impl From<TunnelResource> for CloudflareTunnel {
    fn from(resource: TunnelResource) -> Self {
        let account = resource
            .metadata
            .get("account")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default();
        let cert_file = resource
            .metadata
            .get("certFile")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        Self {
            id: resource.id,
            name: resource.name,
            account,
            cert_file,
            tunnel_id: resource.provider_tunnel_id,
            credentials_file: resource.credentials_ref,
            config_file: resource.config_file,
            ingress: resource.ingress.into_iter().map(Into::into).collect(),
            pending_remote_cleanup: Vec::new(),
            created_at: resource.created_at,
            updated_at: resource.updated_at,
        }
    }
}

impl From<super::domain::CloudflareCommandOutput> for ProviderCommandOutput {
    fn from(output: super::domain::CloudflareCommandOutput) -> Self {
        Self {
            success: output.success,
            stdout: crate::services::redaction::text(output.stdout),
            stderr: crate::services::redaction::text(output.stderr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::state::AppState;

    #[test]
    fn cloudflare_metrics_include_running_connection_memory() {
        let state = AppState::default();
        let tunnel = CloudflareTunnel::new("api");
        let tunnel_id = tunnel.id.clone();
        state.provider_runtime.mark_running(
            CLOUDFLARE_PROVIDER_ID,
            &tunnel_id,
            std::process::id(),
            "running",
        );

        let metrics = cloudflare_tunnel_metrics(&state, tunnel).unwrap();

        assert_eq!(metrics.tunnel_id, tunnel_id);
        assert_eq!(metrics.tunnel_type, "cloudflared");
        assert!(metrics.memory_bytes.unwrap_or_default() > 0);
    }

    #[test]
    fn cloudflare_metrics_skip_stopped_connections() {
        let state = AppState::default();
        let tunnel = CloudflareTunnel::new("api");

        assert!(cloudflare_tunnel_metrics(&state, tunnel).is_none());
    }

    #[test]
    fn cloudflare_command_output_redacts_sensitive_values() {
        let output: ProviderCommandOutput =
            crate::providers::cloudflare::domain::CloudflareCommandOutput {
                success: false,
                stdout: "failed --token secret".into(),
                stderr: "Authorization=Bearer abc".into(),
            }
            .into();

        assert!(output.stdout.contains("--token ***"));
        assert_eq!(output.stderr, "Authorization=***");
        assert!(!output.stdout.contains("secret"));
        assert!(!output.stderr.contains("Bearer abc"));
    }
}
