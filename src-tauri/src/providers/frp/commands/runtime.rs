use crate::error::AppResult;
use crate::providers::frp::services::frpc_service as svc;
use crate::providers::frp::services::frpc_service::ProxyStatusInfo;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficSample {
    pub timestamp: i64,
    pub proxy_name: String,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficStats {
    pub proxy_name: String,
    pub proxy_type: String,
    #[serde(rename = "realLocalIP")]
    pub real_local_ip: String,
    pub real_local_port: u16,
    #[serde(rename = "statsLocalIP")]
    pub stats_local_ip: String,
    pub stats_local_port: u16,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: f64,
    pub upload_speed: f64,
    pub total_bytes: u64,
    pub last_updated_at: i64,
    pub history: Vec<TrafficSample>,
}

#[tauri::command]
pub fn frpc_needs_restart(state: State<AppState>, profile_id: String) -> bool {
    crate::providers::frp::state::frp_state(&state)
        .runtime
        .needs_restart(&profile_id)
}

#[tauri::command]
pub async fn frpc_proxy_status(
    state: State<'_, AppState>,
    profile_id: String,
) -> AppResult<Option<Vec<ProxyStatusInfo>>> {
    svc::proxy_status(&state, &profile_id).await
}

#[tauri::command]
pub fn proxy_traffic_stats(state: State<AppState>, profile_id: String) -> Vec<TrafficStats> {
    if !state.config.settings().traffic_stats_enabled {
        return Vec::new();
    }
    state
        .watchdog_relay
        .stats(crate::providers::contract::FRP_PROVIDER_ID, &profile_id)
        .into_iter()
        .map(|stat| TrafficStats {
            proxy_name: stat.endpoint_name.clone(),
            proxy_type: stat.endpoint_type,
            real_local_ip: stat.target.clone(),
            real_local_port: 0,
            stats_local_ip: stat.relay_host,
            stats_local_port: stat.relay_port,
            download_bytes: stat.download_bytes,
            upload_bytes: stat.upload_bytes,
            download_speed: stat.download_speed,
            upload_speed: stat.upload_speed,
            total_bytes: stat.total_bytes,
            last_updated_at: stat.last_updated_at,
            history: stat
                .history
                .into_iter()
                .map(|sample| TrafficSample {
                    timestamp: sample.timestamp,
                    proxy_name: stat.endpoint_name.clone(),
                    download_bytes: sample.download_bytes,
                    upload_bytes: sample.upload_bytes,
                    download_speed: sample.download_speed,
                    upload_speed: sample.upload_speed,
                })
                .collect(),
        })
        .collect()
}
