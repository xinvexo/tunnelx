use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    MetricSample, ProviderMetrics, TunnelMetrics, TunnelRuntimeState,
};
use crate::services::process_metrics;
use crate::services::process_watchdog;
use crate::services::provider_log;
use crate::state::AppState;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Runtime};
use tunnelx_watchdog_protocol::{
    WatchdogRelayBinding, WatchdogRelayEndpoint, WatchdogRelayProtocol, WatchdogRelayStats,
    WatchdogReleaseRelayRequest, WatchdogRequest, WatchdogResponse,
};

#[derive(Clone, Default)]
pub struct WatchdogRelayState(Arc<Mutex<WatchdogRelayInner>>);

#[derive(Default)]
struct WatchdogRelayInner {
    stats: HashMap<(String, String), Vec<WatchdogRelayStats>>,
    access: HashMap<(String, String, String), RelayAccessState>,
}

#[derive(Default)]
struct RelayAccessState {
    active: bool,
    last_movement_at: i64,
}

struct RelayAccessLog {
    provider_id: String,
    tunnel_id: String,
    message: String,
}

#[derive(Clone, Copy, Default)]
struct RelayAccessTotals {
    download_bytes: u64,
    upload_bytes: u64,
}

const RELAY_ACCESS_IDLE_MS: i64 = 5_000;

#[derive(Debug, Clone)]
pub struct RelayEndpointPlan {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub endpoint_type: String,
    pub protocol: WatchdogRelayProtocol,
    pub target: String,
}

#[derive(Debug, Clone, Default)]
pub struct RelayTrafficStats {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub endpoint_type: String,
    pub target: String,
    pub relay_host: String,
    pub relay_port: u16,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub total_bytes: u64,
    pub last_updated_at: i64,
    pub history: Vec<MetricSample>,
}

impl WatchdogRelayState {
    fn lock(&self) -> MutexGuard<'_, WatchdogRelayInner> {
        self.0.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn update_stats(
        &self,
        provider_id: String,
        tunnel_id: String,
        endpoints: Vec<WatchdogRelayStats>,
    ) -> Vec<RelayAccessLog> {
        let mut inner = self.lock();
        let events = relay_access_logs(&mut inner, &provider_id, &tunnel_id, &endpoints);
        inner.stats.insert((provider_id, tunnel_id), endpoints);
        events
    }

    pub fn clear(&self, provider_id: &str, tunnel_id: &str) {
        let mut inner = self.lock();
        inner
            .stats
            .remove(&(provider_id.to_string(), tunnel_id.to_string()));
        inner
            .access
            .retain(|(stored_provider, stored_tunnel, _), _| {
                stored_provider != provider_id || stored_tunnel != tunnel_id
            });
    }

    pub fn stats(&self, provider_id: &str, tunnel_id: &str) -> Vec<RelayTrafficStats> {
        self.lock()
            .stats
            .get(&(provider_id.to_string(), tunnel_id.to_string()))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|stat| relay_stat_to_runtime(provider_id, tunnel_id, stat))
            .collect()
    }

    pub fn all_stats(&self) -> Vec<(String, String, Vec<RelayTrafficStats>)> {
        self.lock()
            .stats
            .iter()
            .map(|((provider_id, tunnel_id), stats)| {
                (
                    provider_id.clone(),
                    tunnel_id.clone(),
                    stats
                        .iter()
                        .cloned()
                        .map(|stat| relay_stat_to_runtime(provider_id, tunnel_id, stat))
                        .collect(),
                )
            })
            .collect()
    }
}

pub fn prepare<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: &str,
    tunnel_id: &str,
    endpoints: Vec<RelayEndpointPlan>,
) -> AppResult<Vec<WatchdogRelayBinding>> {
    process_watchdog::ensure(app, state)?;
    let response = process_watchdog::request(
        app,
        state,
        WatchdogRequest::PrepareRelay(tunnelx_watchdog_protocol::WatchdogPrepareRelayRequest {
            provider_id: provider_id.into(),
            tunnel_id: tunnel_id.into(),
            endpoints: endpoints
                .into_iter()
                .map(|endpoint| WatchdogRelayEndpoint {
                    endpoint_id: endpoint.endpoint_id,
                    endpoint_name: endpoint.endpoint_name,
                    endpoint_type: endpoint.endpoint_type,
                    protocol: endpoint.protocol,
                    target: endpoint.target,
                })
                .collect(),
        }),
    )?;
    match response {
        WatchdogResponse::RelayPrepared {
            provider_id: response_provider,
            tunnel_id: response_tunnel,
            endpoints,
            ..
        } if response_provider == provider_id && response_tunnel == tunnel_id => Ok(endpoints),
        WatchdogResponse::RelayPrepared { .. } => Err(AppError::Msg(
            "watchdog returned relay bindings for a different connection".into(),
        )),
        _ => Err(AppError::Msg(
            "watchdog did not return relay bindings".into(),
        )),
    }
}

pub fn release(state: &AppState, provider_id: &str, tunnel_id: &str) {
    clear_local(state, provider_id, tunnel_id);
    let _ = process_watchdog::request_if_alive(
        state,
        WatchdogRequest::ReleaseRelay(WatchdogReleaseRelayRequest {
            provider_id: provider_id.into(),
            tunnel_id: tunnel_id.into(),
        }),
    );
}

pub fn clear_local(state: &AppState, provider_id: &str, tunnel_id: &str) {
    state.watchdog_relay.clear(provider_id, tunnel_id);
}

pub fn update_stats_if_active<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    provider_id: String,
    tunnel_id: String,
    endpoints: Vec<WatchdogRelayStats>,
) {
    let logs = update_stats_if_active_local(state, provider_id, tunnel_id, endpoints);
    for log in logs {
        provider_log::emit_system(app, state, &log.provider_id, &log.tunnel_id, log.message);
    }
}

fn update_stats_if_active_local(
    state: &AppState,
    provider_id: String,
    tunnel_id: String,
    endpoints: Vec<WatchdogRelayStats>,
) -> Vec<RelayAccessLog> {
    if state
        .provider_runtime
        .info(&provider_id, &tunnel_id)
        .status
        .is_active()
    {
        state
            .watchdog_relay
            .update_stats(provider_id, tunnel_id, endpoints)
    } else {
        state.watchdog_relay.clear(&provider_id, &tunnel_id);
        Vec::new()
    }
}

pub fn apply_binding(target: &mut String, binding: &WatchdogRelayBinding) {
    *target = binding.relay_target.clone();
}

pub fn tunnel_metrics(
    state: &AppState,
    provider_id: &str,
    tunnel_id: String,
    tunnel_name: String,
    tunnel_type: impl Into<String>,
    endpoint_count: usize,
    extra_metadata: Value,
) -> Option<TunnelMetrics> {
    let info = state.provider_runtime.info(provider_id, &tunnel_id);
    if info.status != TunnelRuntimeState::Running {
        return None;
    }
    let pid = info.pid?;
    let stats = state.watchdog_relay.stats(provider_id, &tunnel_id);
    let download_bytes = stats.iter().map(|stat| stat.download_bytes).sum();
    let upload_bytes = stats.iter().map(|stat| stat.upload_bytes).sum();
    let download_speed = stats.iter().map(|stat| stat.download_speed).sum();
    let upload_speed = stats.iter().map(|stat| stat.upload_speed).sum();
    let last_updated_at = stats
        .iter()
        .map(|stat| stat.last_updated_at)
        .max()
        .unwrap_or_else(crate::domain::now_ms);
    let history = stats
        .iter()
        .flat_map(|stat| stat.history.clone())
        .collect::<Vec<_>>();
    Some(TunnelMetrics {
        tunnel_id,
        tunnel_name,
        tunnel_type: tunnel_type.into(),
        memory_bytes: process_metrics::memory_usage_for_process_ids([pid]),
        download_bytes,
        upload_bytes,
        download_speed,
        upload_speed,
        total_bytes: download_bytes + upload_bytes,
        last_updated_at,
        history,
        metadata: merge_metadata(
            json!({
                "pid": pid,
                "endpointCount": endpoint_count,
                "relayEndpoints": stats.iter().map(relay_endpoint_metadata).collect::<Vec<_>>(),
            }),
            extra_metadata,
        ),
    })
}

pub fn provider_metrics<T>(
    state: &AppState,
    provider_id: &str,
    tunnels: impl IntoIterator<Item = T>,
    mut collect_tunnel: impl FnMut(&AppState, T) -> Option<TunnelMetrics>,
) -> ProviderMetrics {
    let memory_bytes = process_metrics::memory_usage_for_process_ids(
        state.provider_runtime.pids(Some(provider_id)),
    );
    if !state.config.settings().traffic_stats_enabled {
        return ProviderMetrics::from_tunnels(provider_id, memory_bytes, Vec::new());
    }
    let tunnels = tunnels
        .into_iter()
        .filter_map(|tunnel| collect_tunnel(state, tunnel))
        .collect::<Vec<_>>();
    ProviderMetrics::from_tunnels(provider_id, memory_bytes, tunnels)
}

fn relay_endpoint_metadata(stat: &RelayTrafficStats) -> Value {
    json!({
        "id": stat.endpoint_id,
        "name": stat.endpoint_name,
        "type": stat.endpoint_type,
        "target": stat.target,
        "relayHost": stat.relay_host,
        "relayPort": stat.relay_port,
    })
}

fn relay_access_logs(
    inner: &mut WatchdogRelayInner,
    provider_id: &str,
    tunnel_id: &str,
    endpoints: &[WatchdogRelayStats],
) -> Vec<RelayAccessLog> {
    let previous = inner
        .stats
        .get(&(provider_id.to_string(), tunnel_id.to_string()))
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|stat| {
            (
                stat.endpoint_id,
                RelayAccessTotals {
                    download_bytes: stat.download_bytes,
                    upload_bytes: stat.upload_bytes,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut logs = Vec::new();
    for endpoint in endpoints {
        let previous_total = previous
            .get(&endpoint.endpoint_id)
            .copied()
            .unwrap_or_default();
        let upload_delta = endpoint
            .upload_bytes
            .saturating_sub(previous_total.upload_bytes);
        let download_delta = endpoint
            .download_bytes
            .saturating_sub(previous_total.download_bytes);
        let delta = upload_delta.saturating_add(download_delta);
        let key = (
            provider_id.to_string(),
            tunnel_id.to_string(),
            endpoint.endpoint_id.clone(),
        );
        let access = inner.access.entry(key).or_default();
        if delta > 0 {
            if !access.active {
                logs.push(RelayAccessLog {
                    provider_id: provider_id.to_string(),
                    tunnel_id: tunnel_id.to_string(),
                    message: relay_access_message(endpoint, upload_delta, download_delta),
                });
            }
            access.active = true;
            access.last_movement_at = endpoint.last_updated_at;
        } else if access.active
            && endpoint
                .last_updated_at
                .saturating_sub(access.last_movement_at)
                >= RELAY_ACCESS_IDLE_MS
        {
            access.active = false;
        }
    }
    logs
}

fn relay_access_message(
    endpoint: &WatchdogRelayStats,
    upload_delta: u64,
    download_delta: u64,
) -> String {
    let endpoint_label = if endpoint.endpoint_name.trim().is_empty() {
        endpoint.endpoint_type.trim().to_string()
    } else if endpoint.endpoint_type.trim().is_empty() {
        endpoint.endpoint_name.trim().to_string()
    } else {
        format!(
            "{} ({})",
            endpoint.endpoint_name.trim(),
            endpoint.endpoint_type.trim()
        )
    };
    format!(
        "traffic observed: {endpoint_label} +{} up / +{} down",
        format_bytes(upload_delta),
        format_bytes(download_delta)
    )
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.1} GiB", value / GIB)
    } else if value >= MIB {
        format!("{:.1} MiB", value / MIB)
    } else if value >= KIB {
        format!("{:.1} KiB", value / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn merge_metadata(mut base: Value, extra: Value) -> Value {
    if let (Value::Object(base), Value::Object(extra)) = (&mut base, extra) {
        for (key, value) in extra {
            base.insert(key, value);
        }
    }
    base
}

fn relay_stat_to_runtime(
    provider_id: &str,
    tunnel_id: &str,
    stat: WatchdogRelayStats,
) -> RelayTrafficStats {
    let metric_tunnel_id = format!("{provider_id}:{tunnel_id}:{}", stat.endpoint_id);
    RelayTrafficStats {
        endpoint_id: stat.endpoint_id.clone(),
        endpoint_name: stat.endpoint_name.clone(),
        endpoint_type: stat.endpoint_type,
        target: stat.target,
        relay_host: stat.relay_host,
        relay_port: stat.relay_port,
        download_bytes: stat.download_bytes,
        upload_bytes: stat.upload_bytes,
        download_speed: stat.download_speed as f64,
        upload_speed: stat.upload_speed as f64,
        total_bytes: stat.total_bytes,
        last_updated_at: stat.last_updated_at,
        history: stat
            .history
            .into_iter()
            .map(|sample| MetricSample {
                timestamp: sample.timestamp,
                tunnel_id: metric_tunnel_id.clone(),
                tunnel_name: stat.endpoint_name.clone(),
                download_bytes: sample.download_bytes,
                upload_bytes: sample.upload_bytes,
                download_speed: sample.download_speed as f64,
                upload_speed: sample.upload_speed as f64,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tunnelx_watchdog_protocol::WatchdogRelaySample;

    #[test]
    fn provider_metrics_skips_tunnels_when_traffic_stats_are_disabled() {
        let state = AppState::default();
        state
            .config
            .set_options(false, false, false, false, false)
            .unwrap();
        state
            .provider_runtime
            .mark_running("provider-a", "conn-1", std::process::id(), "running");

        let metrics = provider_metrics(&state, "provider-a", ["conn-1"], |state, tunnel_id| {
            tunnel_metrics(
                state,
                "provider-a",
                tunnel_id.to_string(),
                "Connection".into(),
                "runtime",
                1,
                json!({}),
            )
        });

        assert_eq!(metrics.provider_id, "provider-a");
        assert!(metrics.tunnels.is_empty());
        assert_eq!(metrics.download_bytes, 0);
        assert_eq!(metrics.upload_bytes, 0);
    }

    #[test]
    fn provider_metrics_collects_watchdog_relay_tunnels() {
        let state = AppState::default();
        state
            .provider_runtime
            .mark_running("provider-a", "conn-1", std::process::id(), "running");
        state.watchdog_relay.update_stats(
            "provider-a".into(),
            "conn-1".into(),
            vec![WatchdogRelayStats {
                endpoint_id: "endpoint-1".into(),
                endpoint_name: "web".into(),
                endpoint_type: "http".into(),
                protocol: WatchdogRelayProtocol::Tcp,
                target: "127.0.0.1:1420".into(),
                relay_host: "127.0.0.1".into(),
                relay_port: 30000,
                download_bytes: 120,
                upload_bytes: 30,
                download_speed: 12,
                upload_speed: 3,
                total_bytes: 150,
                last_updated_at: 42,
                history: vec![WatchdogRelaySample {
                    timestamp: 40,
                    download_bytes: 100,
                    upload_bytes: 20,
                    download_speed: 10,
                    upload_speed: 2,
                }],
            }],
        );

        let metrics = provider_metrics(&state, "provider-a", ["conn-1"], |state, tunnel_id| {
            tunnel_metrics(
                state,
                "provider-a",
                tunnel_id.to_string(),
                "Connection".into(),
                "runtime",
                1,
                json!({"provider": "test"}),
            )
        });

        assert_eq!(metrics.download_bytes, 120);
        assert_eq!(metrics.upload_bytes, 30);
        assert_eq!(metrics.tunnels.len(), 1);
        assert_eq!(metrics.tunnels[0].download_speed, 12.0);
        assert_eq!(metrics.tunnels[0].upload_speed, 3.0);
        assert_eq!(metrics.tunnels[0].history.len(), 1);
        assert_eq!(metrics.tunnels[0].metadata["provider"], "test");
        assert_eq!(metrics.tunnels[0].metadata["endpointCount"], 1);
    }

    #[test]
    fn relay_stats_are_ignored_for_inactive_tunnels() {
        let state = AppState::default();
        state.provider_runtime.mark_status(
            "provider-a",
            "conn-1",
            TunnelRuntimeState::Stopped,
            "stopped",
        );

        update_stats_if_active_local(
            &state,
            "provider-a".into(),
            "conn-1".into(),
            vec![WatchdogRelayStats {
                endpoint_id: "endpoint-1".into(),
                endpoint_name: "web".into(),
                endpoint_type: "http".into(),
                protocol: WatchdogRelayProtocol::Tcp,
                target: "127.0.0.1:1420".into(),
                relay_host: "127.0.0.1".into(),
                relay_port: 30000,
                download_bytes: 120,
                upload_bytes: 30,
                download_speed: 12,
                upload_speed: 3,
                total_bytes: 150,
                last_updated_at: 42,
                history: Vec::new(),
            }],
        );

        assert!(state
            .watchdog_relay
            .stats("provider-a", "conn-1")
            .is_empty());
    }

    #[test]
    fn relay_access_logs_when_tcp_traffic_starts() {
        let state = AppState::default();
        state
            .provider_runtime
            .mark_running("provider-a", "conn-1", std::process::id(), "running");
        assert!(update_stats_if_active_local(
            &state,
            "provider-a".into(),
            "conn-1".into(),
            vec![relay_stat("endpoint-1", "rdp", 0, 0, 1_000)],
        )
        .is_empty());

        let logs = update_stats_if_active_local(
            &state,
            "provider-a".into(),
            "conn-1".into(),
            vec![relay_stat("endpoint-1", "rdp", 256, 1024, 2_000)],
        );

        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0].message,
            "traffic observed: rdp +256 B up / +1.0 KiB down"
        );
    }

    #[test]
    fn relay_access_logs_are_throttled_until_idle() {
        let state = AppState::default();
        state
            .provider_runtime
            .mark_running("provider-a", "conn-1", std::process::id(), "running");

        assert_eq!(
            update_stats_if_active_local(
                &state,
                "provider-a".into(),
                "conn-1".into(),
                vec![relay_stat("endpoint-1", "tcp", 100, 0, 1_000)],
            )
            .len(),
            1
        );
        assert!(update_stats_if_active_local(
            &state,
            "provider-a".into(),
            "conn-1".into(),
            vec![relay_stat("endpoint-1", "tcp", 200, 0, 2_000)],
        )
        .is_empty());
        assert!(update_stats_if_active_local(
            &state,
            "provider-a".into(),
            "conn-1".into(),
            vec![relay_stat("endpoint-1", "tcp", 200, 0, 7_000)],
        )
        .is_empty());

        let logs = update_stats_if_active_local(
            &state,
            "provider-a".into(),
            "conn-1".into(),
            vec![relay_stat("endpoint-1", "tcp", 300, 0, 8_000)],
        );

        assert_eq!(logs.len(), 1);
    }

    fn relay_stat(
        endpoint_id: &str,
        endpoint_type: &str,
        upload_bytes: u64,
        download_bytes: u64,
        last_updated_at: i64,
    ) -> WatchdogRelayStats {
        WatchdogRelayStats {
            endpoint_id: endpoint_id.into(),
            endpoint_name: String::new(),
            endpoint_type: endpoint_type.into(),
            protocol: WatchdogRelayProtocol::Tcp,
            target: "127.0.0.1:1420".into(),
            relay_host: "127.0.0.1".into(),
            relay_port: 30000,
            download_bytes,
            upload_bytes,
            download_speed: 0,
            upload_speed: 0,
            total_bytes: upload_bytes + download_bytes,
            last_updated_at,
            history: Vec::new(),
        }
    }
}
