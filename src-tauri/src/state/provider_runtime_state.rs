use crate::error::{AppError, AppResult};
use crate::providers::contract::{empty_details, TunnelRuntimeInfo, TunnelRuntimeState};
use crate::sync_ext::MutexExt;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const MAX_PROVIDER_LOG_LINES: usize = 1000;
const PROVIDER_LOG_DROP_COUNT: usize = 100;

pub struct ProviderRuntimeInstance {
    pub provider_id: String,
    pub tunnel_id: String,
    pub status: TunnelRuntimeState,
    pub pid: Option<u32>,
    pub message: String,
    pub details: Value,
    pub logs: VecDeque<String>,
    pub stop_timeout: Option<Duration>,
}

impl Default for ProviderRuntimeInstance {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            tunnel_id: String::new(),
            status: TunnelRuntimeState::Stopped,
            pid: None,
            message: String::new(),
            details: empty_details(),
            logs: VecDeque::new(),
            stop_timeout: None,
        }
    }
}

impl ProviderRuntimeInstance {
    fn info(&self) -> TunnelRuntimeInfo {
        TunnelRuntimeInfo {
            provider_id: self.provider_id.clone(),
            tunnel_id: self.tunnel_id.clone(),
            status: self.status,
            pid: self.pid,
            message: self.message.clone(),
            details: self.details.clone(),
        }
    }
}

#[derive(Default)]
pub struct ProviderRuntimeInner {
    pub instances: HashMap<String, ProviderRuntimeInstance>,
}

impl ProviderRuntimeInner {
    fn instance_mut(&mut self, provider_id: &str, tunnel_id: &str) -> &mut ProviderRuntimeInstance {
        let key = runtime_key(provider_id, tunnel_id);
        self.instances
            .entry(key)
            .or_insert_with(|| ProviderRuntimeInstance {
                provider_id: provider_id.to_string(),
                tunnel_id: tunnel_id.to_string(),
                ..Default::default()
            })
    }
}

#[derive(Clone, Default)]
pub struct ProviderRuntimeState(Arc<Mutex<ProviderRuntimeInner>>);

impl ProviderRuntimeState {
    pub fn lock(&self) -> MutexGuard<'_, ProviderRuntimeInner> {
        self.0.lock_recover()
    }

    pub fn begin_start(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        message: impl Into<String>,
    ) -> AppResult<TunnelRuntimeInfo> {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        if instance.status.is_active() {
            return Err(AppError::AlreadyRunning);
        }
        instance.status = TunnelRuntimeState::Starting;
        instance.pid = None;
        instance.message = runtime_message(message);
        instance.details = empty_details();
        instance.logs.clear();
        instance.stop_timeout = None;
        Ok(instance.info())
    }

    #[cfg(test)]
    pub fn mark_running(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        pid: u32,
        message: impl Into<String>,
    ) -> TunnelRuntimeInfo {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        instance.status = TunnelRuntimeState::Running;
        instance.pid = Some(pid);
        instance.message = runtime_message(message);
        instance.info()
    }

    pub fn mark_spawned(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        pid: u32,
        message: impl Into<String>,
    ) -> TunnelRuntimeInfo {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        instance.pid = Some(pid);
        instance.message = runtime_message(message);
        if !instance.status.is_active() {
            instance.status = TunnelRuntimeState::Starting;
        }
        instance.info()
    }

    pub fn set_details(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        details: Value,
    ) -> TunnelRuntimeInfo {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        instance.details = runtime_details(details);
        instance.info()
    }

    pub fn mark_status(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        status: TunnelRuntimeState,
        message: impl Into<String>,
    ) -> TunnelRuntimeInfo {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        instance.status = status;
        instance.message = runtime_message(message);
        if !status.is_active() {
            instance.pid = None;
            instance.details = empty_details();
            instance.stop_timeout = None;
        }
        instance.info()
    }

    pub fn mark_exit(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        success: bool,
        message: impl Into<String>,
    ) -> TunnelRuntimeInfo {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        instance.status = if instance.status == TunnelRuntimeState::Stopping || success {
            TunnelRuntimeState::Stopped
        } else {
            TunnelRuntimeState::Errored
        };
        instance.pid = None;
        instance.message = runtime_message(message);
        instance.details = empty_details();
        instance.stop_timeout = None;
        instance.info()
    }

    pub fn set_stop_timeout(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        timeout: Duration,
    ) -> TunnelRuntimeInfo {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        instance.stop_timeout = Some(timeout);
        instance.info()
    }

    pub fn stop_timeout(&self, provider_id: &str, tunnel_id: &str) -> Option<Duration> {
        self.lock()
            .instances
            .get(&runtime_key(provider_id, tunnel_id))
            .and_then(|instance| instance.stop_timeout)
    }

    pub fn reconcile(&self, provider_id: &str, tunnel_id: &str) -> TunnelRuntimeInfo {
        self.info(provider_id, tunnel_id)
    }

    pub fn info(&self, provider_id: &str, tunnel_id: &str) -> TunnelRuntimeInfo {
        self.lock()
            .instances
            .get(&runtime_key(provider_id, tunnel_id))
            .map(ProviderRuntimeInstance::info)
            .unwrap_or_else(|| TunnelRuntimeInfo {
                provider_id: provider_id.to_string(),
                tunnel_id: tunnel_id.to_string(),
                status: TunnelRuntimeState::Stopped,
                pid: None,
                message: String::new(),
                details: empty_details(),
            })
    }

    pub fn wait_for_inactive(
        &self,
        provider_id: &str,
        tunnel_id: &str,
        timeout: Duration,
        poll: Duration,
    ) -> Option<TunnelRuntimeInfo> {
        let deadline = Instant::now() + timeout;
        loop {
            let info = self.info(provider_id, tunnel_id);
            if !info.status.is_active() {
                return Some(info);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(poll);
        }
    }

    pub fn logs(&self, provider_id: &str, tunnel_id: &str) -> Vec<String> {
        self.lock()
            .instances
            .get(&runtime_key(provider_id, tunnel_id))
            .map(|instance| instance.logs.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn push_log(&self, provider_id: &str, tunnel_id: &str, line: String) -> bool {
        let mut inner = self.lock();
        let instance = inner.instance_mut(provider_id, tunnel_id);
        if instance.logs.back() == Some(&line) {
            return false;
        }
        if instance.logs.len() >= MAX_PROVIDER_LOG_LINES {
            for _ in 0..PROVIDER_LOG_DROP_COUNT {
                instance.logs.pop_front();
            }
        }
        instance.logs.push_back(line);
        true
    }

    pub fn clear_logs(&self, provider_id: &str, tunnel_id: &str) {
        let mut inner = self.lock();
        inner.instance_mut(provider_id, tunnel_id).logs.clear();
    }

    pub fn remove(&self, provider_id: &str, tunnel_id: &str) {
        self.lock()
            .instances
            .remove(&runtime_key(provider_id, tunnel_id));
    }

    pub fn active_count(&self) -> usize {
        self.lock()
            .instances
            .values()
            .filter(|instance| instance.status.is_active())
            .count()
    }

    pub fn active_keys(&self) -> Vec<(String, String)> {
        self.lock()
            .instances
            .values()
            .filter(|instance| instance.status.is_active())
            .map(|instance| (instance.provider_id.clone(), instance.tunnel_id.clone()))
            .collect()
    }

    pub fn pids(&self, provider_id: Option<&str>) -> Vec<u32> {
        self.lock()
            .instances
            .values()
            .filter(|instance| {
                provider_id
                    .map(|id| instance.provider_id == id)
                    .unwrap_or(true)
            })
            .filter_map(|instance| instance.pid)
            .collect()
    }
}

fn runtime_key(provider_id: &str, tunnel_id: &str) -> String {
    format!("{provider_id}\u{1f}{tunnel_id}")
}

fn runtime_message(message: impl Into<String>) -> String {
    crate::services::redaction::text(message)
}

fn runtime_details(details: Value) -> Value {
    crate::services::redaction::json_value(details)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_start_clears_existing_logs() {
        let runtime = ProviderRuntimeState::default();
        runtime.push_log("provider-a", "conn-1", "previous failure".into());

        runtime
            .begin_start("provider-a", "conn-1", "starting")
            .unwrap();

        assert!(runtime.logs("provider-a", "conn-1").is_empty());
    }

    #[test]
    fn push_log_drops_adjacent_duplicates() {
        let runtime = ProviderRuntimeState::default();

        assert!(runtime.push_log("provider-a", "conn-1", "same line".into()));
        assert!(!runtime.push_log("provider-a", "conn-1", "same line".into()));
        assert!(runtime.push_log("provider-a", "conn-1", "next line".into()));
        assert!(runtime.push_log("provider-a", "conn-1", "same line".into()));

        assert_eq!(
            runtime.logs("provider-a", "conn-1"),
            vec![
                "same line".to_string(),
                "next line".to_string(),
                "same line".to_string(),
            ]
        );
    }

    #[test]
    fn stop_timeout_is_runtime_only_and_clears_when_inactive() {
        let runtime = ProviderRuntimeState::default();
        runtime
            .begin_start("provider-a", "conn-1", "starting")
            .unwrap();
        runtime.set_stop_timeout("provider-a", "conn-1", Duration::from_secs(30));

        assert_eq!(
            runtime.stop_timeout("provider-a", "conn-1"),
            Some(Duration::from_secs(30))
        );

        runtime.mark_status(
            "provider-a",
            "conn-1",
            TunnelRuntimeState::Stopped,
            "stopped",
        );

        assert_eq!(runtime.stop_timeout("provider-a", "conn-1"), None);
    }

    #[test]
    fn wait_for_inactive_returns_terminal_state() {
        let runtime = ProviderRuntimeState::default();
        runtime
            .begin_start("provider-a", "conn-1", "starting")
            .unwrap();

        let watcher = runtime.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            watcher.mark_status(
                "provider-a",
                "conn-1",
                TunnelRuntimeState::Stopped,
                "stopped",
            );
        });

        let info = runtime
            .wait_for_inactive(
                "provider-a",
                "conn-1",
                Duration::from_secs(1),
                Duration::from_millis(5),
            )
            .unwrap();

        assert_eq!(info.status, TunnelRuntimeState::Stopped);
    }

    #[test]
    fn wait_for_inactive_times_out_when_still_active() {
        let runtime = ProviderRuntimeState::default();
        runtime
            .begin_start("provider-a", "conn-1", "starting")
            .unwrap();

        assert!(runtime
            .wait_for_inactive(
                "provider-a",
                "conn-1",
                Duration::from_millis(1),
                Duration::from_millis(1),
            )
            .is_none());
    }

    #[test]
    fn remove_clears_runtime_instance_and_logs() {
        let runtime = ProviderRuntimeState::default();
        runtime
            .begin_start("provider-a", "conn-1", "starting")
            .unwrap();
        runtime.push_log("provider-a", "conn-1", "line".into());
        runtime.set_stop_timeout("provider-a", "conn-1", Duration::from_secs(30));

        runtime.remove("provider-a", "conn-1");

        let info = runtime.info("provider-a", "conn-1");
        assert_eq!(info.status, TunnelRuntimeState::Stopped);
        assert!(runtime.logs("provider-a", "conn-1").is_empty());
        assert_eq!(runtime.stop_timeout("provider-a", "conn-1"), None);
    }

    #[test]
    fn runtime_status_message_redacts_sensitive_values() {
        let runtime = ProviderRuntimeState::default();

        let info = runtime
            .begin_start(
                "provider-a",
                "conn-1",
                "failed --token secret Authorization=Bearer abc",
            )
            .unwrap();

        assert!(info.message.contains("--token ***"));
        assert!(info.message.contains("Authorization=***"));
        assert!(!info.message.contains("secret"));
        assert!(!info.message.contains("Bearer abc"));
    }

    #[test]
    fn runtime_details_redact_sensitive_values() {
        let runtime = ProviderRuntimeState::default();

        let info = runtime.set_details(
            "provider-a",
            "conn-1",
            serde_json::json!({
                "token": "secret-token",
                "url": "https://api.example.test?token=query-token&ok=1",
                "nested": {
                    "Authorization": "Bearer abc"
                }
            }),
        );

        assert_eq!(info.details["token"], "***");
        assert_eq!(
            info.details["url"],
            "https://api.example.test?token=***&ok=1"
        );
        assert_eq!(info.details["nested"]["Authorization"], "***");
        assert!(!info.details.to_string().contains("secret-token"));
        assert!(!info.details.to_string().contains("query-token"));
        assert!(!info.details.to_string().contains("Bearer abc"));
    }
}
