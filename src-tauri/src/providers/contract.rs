use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

pub const FRP_PROVIDER_ID: &str = "frp";
pub const CLOUDFLARE_PROVIDER_ID: &str = "cloudflare";
pub const NGROK_PROVIDER_ID: &str = "ngrok";
pub const CPOLAR_PROVIDER_ID: &str = "cpolar";
pub const PINGGY_PROVIDER_ID: &str = "pinggy";
pub const TUNNEL_NAME_MAX_CHARS: usize = 64;
pub const PROVIDER_STOP_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
pub const PROVIDER_STOP_WAIT_POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub capabilities: ProviderCapabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    #[serde(default)]
    pub account_login: bool,
    #[serde(default)]
    pub named_tunnels: bool,
    #[serde(default)]
    pub credentials: bool,
    #[serde(default)]
    pub dns_routes: bool,
    #[serde(default)]
    pub ingress: bool,
    #[serde(default)]
    pub local_runtime: bool,
    #[serde(default)]
    pub runtime_metrics: bool,
    #[serde(default)]
    pub memory_stats: bool,
    #[serde(default)]
    pub traffic_stats: bool,
    #[serde(default)]
    pub version_management: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub provider_id: String,
    pub available: bool,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub details: Value,
}

pub const PROVIDER_RUNTIME_INSTALL_PROGRESS_EVENT: &str = "provider-runtime-install-progress";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeInstallProgress {
    pub provider_id: String,
    pub runtime: String,
    pub received_bytes: u64,
    #[serde(default)]
    pub total_bytes: Option<u64>,
    #[serde(default)]
    pub percent: Option<f64>,
}

pub fn emit_runtime_install_progress(
    app: &AppHandle,
    provider_id: &str,
    runtime: &str,
    received_bytes: u64,
    total_bytes: Option<u64>,
) {
    let percent = total_bytes
        .filter(|total| *total > 0)
        .map(|total| (received_bytes as f64 / total as f64 * 100.0).clamp(0.0, 100.0));
    let _ = app.emit(
        PROVIDER_RUNTIME_INSTALL_PROGRESS_EVENT,
        ProviderRuntimeInstallProgress {
            provider_id: provider_id.into(),
            runtime: runtime.into(),
            received_bytes,
            total_bytes,
            percent,
        },
    );
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeUpdateStatus {
    pub provider_id: String,
    pub runtime: String,
    #[serde(default)]
    pub current_version: Option<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
    pub update_available: bool,
    #[serde(default)]
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTunnelInput {
    pub name: String,
}

pub fn normalize_tunnel_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Msg("Connection name is required".into()));
    }
    if name.chars().count() > TUNNEL_NAME_MAX_CHARS {
        return Err(AppError::Msg(format!(
            "Connection name must be {TUNNEL_NAME_MAX_CHARS} characters or fewer"
        )));
    }
    if !name.chars().all(valid_tunnel_name_char) {
        return Err(AppError::Msg(
            "Connection name can only contain letters, numbers, Chinese characters, dots, hyphens, and underscores. Spaces are not allowed".into(),
        ));
    }
    Ok(name.to_string())
}

fn valid_tunnel_name_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '.' | '-' | '_')
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngressRule {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub service: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub dns_routed: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelResource {
    pub id: String,
    pub provider_id: String,
    pub name: String,
    #[serde(default)]
    pub provider_tunnel_id: String,
    #[serde(default)]
    pub credentials_ref: String,
    #[serde(default)]
    pub config_file: String,
    #[serde(default)]
    pub ingress: Vec<IngressRule>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderTunnelsUpdatedEvent {
    pub provider_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TunnelRuntimeState {
    #[default]
    Stopped,
    Starting,
    Running,
    Warning,
    Stopping,
    Errored,
}

impl TunnelRuntimeState {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            TunnelRuntimeState::Starting
                | TunnelRuntimeState::Running
                | TunnelRuntimeState::Warning
                | TunnelRuntimeState::Stopping
        )
    }
}

pub fn watchdog_exit_clean(success: bool, cleanup_success: Option<bool>) -> bool {
    success && cleanup_success.unwrap_or(true)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelRuntimeInfo {
    pub provider_id: String,
    pub tunnel_id: String,
    pub status: TunnelRuntimeState,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub message: String,
    #[serde(default = "empty_details")]
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelRuntimeStatusEvent {
    pub info: TunnelRuntimeInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelRuntimeLogEvent {
    pub provider_id: String,
    pub tunnel_id: String,
    pub line: String,
    #[serde(default)]
    pub reset: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetricSample {
    pub timestamp: i64,
    pub tunnel_id: String,
    pub tunnel_name: String,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TunnelMetrics {
    pub tunnel_id: String,
    pub tunnel_name: String,
    #[serde(default)]
    pub tunnel_type: String,
    #[serde(default)]
    pub memory_bytes: Option<u64>,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub total_bytes: u64,
    pub last_updated_at: i64,
    #[serde(default)]
    pub history: Vec<MetricSample>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderMetrics {
    pub provider_id: String,
    pub collected_at: i64,
    #[serde(default)]
    pub memory_bytes: Option<u64>,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub total_bytes: u64,
    #[serde(default)]
    pub tunnels: Vec<TunnelMetrics>,
    #[serde(default)]
    pub metadata: Value,
}

impl ProviderMetrics {
    pub fn empty(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            collected_at: crate::domain::now_ms(),
            memory_bytes: None,
            download_bytes: 0,
            upload_bytes: 0,
            download_speed: 0.0,
            upload_speed: 0.0,
            total_bytes: 0,
            tunnels: Vec::new(),
            metadata: empty_details(),
        }
    }

    pub fn from_tunnels(
        provider_id: impl Into<String>,
        memory_bytes: Option<u64>,
        tunnels: Vec<TunnelMetrics>,
    ) -> Self {
        let mut metrics = Self::empty(provider_id);
        metrics.memory_bytes = memory_bytes;
        metrics.download_bytes = tunnels.iter().map(|item| item.download_bytes).sum();
        metrics.upload_bytes = tunnels.iter().map(|item| item.upload_bytes).sum();
        metrics.download_speed = tunnels.iter().map(|item| item.download_speed).sum();
        metrics.upload_speed = tunnels.iter().map(|item| item.upload_speed).sum();
        metrics.total_bytes = metrics.download_bytes + metrics.upload_bytes;
        metrics.tunnels = tunnels;
        metrics
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetrics {
    pub collected_at: i64,
    #[serde(default)]
    pub app_memory_bytes: Option<u64>,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub total_bytes: u64,
    #[serde(default)]
    pub providers: Vec<ProviderMetrics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTrafficSummary {
    pub collected_at: i64,
    #[serde(default)]
    pub app_memory_bytes: Option<u64>,
    pub has_active_tunnels: bool,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub total_bytes: u64,
    #[serde(default)]
    pub history: Vec<MetricSample>,
}

pub struct ProviderContext<'a> {
    pub app: &'a AppHandle,
    pub state: &'a AppState,
}

impl<'a> ProviderContext<'a> {
    pub fn new(app: &'a AppHandle, state: &'a AppState) -> Self {
        Self { app, state }
    }
}

pub trait TunnelProvider: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;

    fn status(&self, ctx: ProviderContext<'_>) -> AppResult<ProviderStatus>;

    fn cleanup_on_start(&self, _ctx: ProviderContext<'_>) -> AppResult<()> {
        Ok(())
    }

    fn auto_connect(&self, ctx: ProviderContext<'_>) -> AppResult<()> {
        let descriptor = self.descriptor();
        if !descriptor.capabilities.local_runtime || !descriptor.capabilities.named_tunnels {
            return Ok(());
        }

        let mut errors = Vec::new();
        for tunnel in self.list_tunnels(ProviderContext::new(ctx.app, ctx.state))? {
            let status = self
                .tunnel_status(ProviderContext::new(ctx.app, ctx.state), &tunnel.id)
                .map(|info| info.status)
                .unwrap_or(TunnelRuntimeState::Stopped);
            if status.is_active() {
                continue;
            }
            match self.start_tunnel(ProviderContext::new(ctx.app, ctx.state), &tunnel.id) {
                Ok(_) | Err(AppError::AlreadyRunning) => {}
                Err(error) => errors.push(format!("{}({}): {error}", tunnel.name, tunnel.id)),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(AppError::Msg(errors.join("; ")))
        }
    }

    fn process_ids(&self, state: &AppState) -> Vec<u32> {
        state.provider_runtime.pids(Some(&self.descriptor().id))
    }

    fn settings_changed(
        &self,
        _ctx: ProviderContext<'_>,
        _previous: &crate::domain::AppSettings,
        _next: &crate::domain::AppSettings,
    ) {
    }

    fn login(&self, _ctx: ProviderContext<'_>) -> AppResult<ProviderCommandOutput> {
        Err(unsupported(self.descriptor().id, "login"))
    }

    fn list_tunnels(&self, _ctx: ProviderContext<'_>) -> AppResult<Vec<TunnelResource>> {
        Err(unsupported(self.descriptor().id, "list_tunnels"))
    }

    fn create_tunnel(
        &self,
        _ctx: ProviderContext<'_>,
        _input: CreateTunnelInput,
    ) -> AppResult<TunnelResource> {
        Err(unsupported(self.descriptor().id, "create_tunnel"))
    }

    fn update_tunnel(
        &self,
        _ctx: ProviderContext<'_>,
        _tunnel: TunnelResource,
    ) -> AppResult<TunnelResource> {
        Err(unsupported(self.descriptor().id, "update_tunnel"))
    }

    fn delete_tunnel(&self, _ctx: ProviderContext<'_>, _id: &str, _remote: bool) -> AppResult<()> {
        Err(unsupported(self.descriptor().id, "delete_tunnel"))
    }

    fn metrics(&self, _ctx: ProviderContext<'_>) -> AppResult<ProviderMetrics> {
        Ok(ProviderMetrics::empty(self.descriptor().id))
    }

    fn start_tunnel(&self, _ctx: ProviderContext<'_>, _id: &str) -> AppResult<TunnelRuntimeInfo> {
        Err(unsupported(self.descriptor().id, "start_tunnel"))
    }

    fn stop_tunnel(&self, _ctx: ProviderContext<'_>, _id: &str) -> AppResult<TunnelRuntimeInfo> {
        Err(unsupported(self.descriptor().id, "stop_tunnel"))
    }

    fn tunnel_status(&self, _ctx: ProviderContext<'_>, _id: &str) -> AppResult<TunnelRuntimeInfo> {
        Err(unsupported(self.descriptor().id, "tunnel_status"))
    }
}

fn unsupported(provider_id: String, operation: &str) -> AppError {
    AppError::Msg(format!(
        "Provider {provider_id} does not support {operation}"
    ))
}

pub fn empty_details() -> Value {
    json!({})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_exit_requires_process_and_cleanup_success() {
        assert!(watchdog_exit_clean(true, Some(true)));
        assert!(watchdog_exit_clean(true, None));
        assert!(!watchdog_exit_clean(true, Some(false)));
        assert!(!watchdog_exit_clean(false, Some(true)));
    }
}
