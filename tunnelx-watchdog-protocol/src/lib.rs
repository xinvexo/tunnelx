use serde::{Deserialize, Serialize};

pub mod redaction;

pub const WATCHDOG_PROTOCOL_VERSION: u16 = 7;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WatchdogFrame {
    Request {
        version: u16,
        id: String,
        seq: u64,
        #[serde(flatten)]
        request: WatchdogRequest,
    },
    Response {
        version: u16,
        id: String,
        seq: u64,
        #[serde(flatten)]
        response: WatchdogResponse,
    },
    Event {
        version: u16,
        id: String,
        seq: u64,
        #[serde(flatten)]
        event: WatchdogEvent,
    },
}

impl WatchdogFrame {
    pub fn request(id: impl Into<String>, seq: u64, request: WatchdogRequest) -> Self {
        Self::Request {
            version: WATCHDOG_PROTOCOL_VERSION,
            id: id.into(),
            seq,
            request,
        }
    }

    pub fn response(id: impl Into<String>, seq: u64, response: WatchdogResponse) -> Self {
        Self::Response {
            version: WATCHDOG_PROTOCOL_VERSION,
            id: id.into(),
            seq,
            response,
        }
    }

    pub fn event(id: impl Into<String>, seq: u64, event: WatchdogEvent) -> Self {
        Self::Event {
            version: WATCHDOG_PROTOCOL_VERSION,
            id: id.into(),
            seq,
            event,
        }
    }

    pub fn version(&self) -> u16 {
        match self {
            WatchdogFrame::Request { version, .. }
            | WatchdogFrame::Response { version, .. }
            | WatchdogFrame::Event { version, .. } => *version,
        }
    }

    pub fn supported_version(&self) -> bool {
        self.version() == WATCHDOG_PROTOCOL_VERSION
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "name", content = "payload", rename_all = "snake_case")]
pub enum WatchdogRequest {
    Attach(WatchdogAttachRequest),
    PrepareRelay(WatchdogPrepareRelayRequest),
    ReleaseRelay(WatchdogReleaseRelayRequest),
    StartProcess(WatchdogStartProcessRequest),
    StopProcess(WatchdogStopProcessRequest),
    Shutdown,
    Snapshot,
    Heartbeat(WatchdogHeartbeatRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogAttachRequest {
    pub token: String,
    pub parent_pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogHeartbeatRequest {
    pub parent_pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogPrepareRelayRequest {
    pub provider_id: String,
    pub tunnel_id: String,
    #[serde(default)]
    pub endpoints: Vec<WatchdogRelayEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogReleaseRelayRequest {
    pub provider_id: String,
    pub tunnel_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogRelayEndpoint {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub endpoint_type: String,
    pub protocol: WatchdogRelayProtocol,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatchdogRelayProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogRelayBinding {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub endpoint_type: String,
    pub protocol: WatchdogRelayProtocol,
    pub target: String,
    pub relay_host: String,
    pub relay_port: u16,
    pub relay_target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogStartProcessRequest {
    pub provider_id: String,
    pub tunnel_id: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: Vec<WatchdogEnvVar>,
    #[serde(default)]
    pub stop_strategy: Option<Box<WatchdogStopStrategy>>,
    #[serde(default)]
    pub cleanup: Option<Box<WatchdogCleanupPlan>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogStopProcessRequest {
    pub provider_id: String,
    pub tunnel_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "name", content = "payload", rename_all = "snake_case")]
pub enum WatchdogResponse {
    Accepted {
        request: String,
    },
    RelayPrepared {
        request: String,
        provider_id: String,
        tunnel_id: String,
        endpoints: Vec<WatchdogRelayBinding>,
    },
    Rejected {
        message: String,
    },
    Failed {
        message: String,
    },
}

impl WatchdogResponse {
    pub fn is_accepted(&self) -> bool {
        matches!(
            self,
            WatchdogResponse::Accepted { .. } | WatchdogResponse::RelayPrepared { .. }
        )
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            WatchdogResponse::Accepted { .. } | WatchdogResponse::RelayPrepared { .. } => None,
            WatchdogResponse::Rejected { message } | WatchdogResponse::Failed { message } => {
                Some(message)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WatchdogStopStrategy {
    Http {
        request: WatchdogHttpRequest,
        #[serde(default)]
        grace_ms: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogCleanupPlan {
    #[serde(default)]
    pub actions: Vec<WatchdogCleanupAction>,
    #[serde(default)]
    pub retry_attempts: Option<usize>,
    #[serde(default)]
    pub retry_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WatchdogCleanupAction {
    Http {
        request: Box<WatchdogHttpRequest>,
    },
    Command {
        program: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: Vec<WatchdogEnvVar>,
        #[serde(default)]
        timeout_ms: Option<u64>,
        #[serde(default)]
        accepted_codes: Vec<i32>,
    },
    HttpJsonDeleteMatches {
        list_request: Box<WatchdogHttpRequest>,
        items_field: String,
        id_field: String,
        match_field: String,
        match_value: String,
        #[serde(default)]
        match_ignore_ascii_case: bool,
        #[serde(default)]
        match_trim_trailing_dot: bool,
        delete_request: Box<WatchdogHttpRequest>,
        id_placeholder: String,
    },
    RemoveFile {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogHttpRequest {
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: Vec<WatchdogHttpHeader>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub accepted_statuses: Vec<u16>,
    #[serde(default)]
    pub json_expectation: Option<WatchdogJsonExpectation>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogHttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogEnvVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogJsonExpectation {
    pub success_field: String,
    pub errors_field: String,
    pub error_code_field: String,
    pub error_message_field: String,
    #[serde(default)]
    pub ignored_error_codes: Vec<u32>,
    #[serde(default)]
    pub ignored_error_message_contains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogRelaySample {
    pub timestamp: i64,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchdogRelayStats {
    pub endpoint_id: String,
    pub endpoint_name: String,
    pub endpoint_type: String,
    pub protocol: WatchdogRelayProtocol,
    pub target: String,
    pub relay_host: String,
    pub relay_port: u16,
    pub download_bytes: u64,
    pub upload_bytes: u64,
    pub download_speed: u64,
    pub upload_speed: u64,
    pub total_bytes: u64,
    pub last_updated_at: i64,
    #[serde(default)]
    pub history: Vec<WatchdogRelaySample>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "name",
    content = "payload",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum WatchdogEvent {
    OwnerAttached {
        pid: u32,
        active: usize,
    },
    /// A process was created by this watchdog session after a start request.
    ProcessStarted {
        provider_id: String,
        tunnel_id: String,
        pid: u32,
    },
    /// A process was already owned by the watchdog when the owner reattached.
    ProcessSnapshot {
        provider_id: String,
        tunnel_id: String,
        pid: u32,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        recent_logs: Vec<String>,
    },
    ProcessLog {
        provider_id: String,
        tunnel_id: String,
        line: String,
    },
    ProcessExit {
        provider_id: String,
        tunnel_id: String,
        success: bool,
        #[serde(default)]
        forced: bool,
        #[serde(default)]
        code: Option<i32>,
        #[serde(default)]
        cleanup_success: Option<bool>,
        #[serde(default)]
        cleanup_error: Option<String>,
    },
    ProcessError {
        provider_id: String,
        tunnel_id: String,
        message: String,
    },
    RelayError {
        provider_id: String,
        tunnel_id: String,
        endpoint_id: String,
        endpoint_name: String,
        endpoint_type: String,
        target: String,
        message: String,
    },
    RelayStats {
        provider_id: String,
        tunnel_id: String,
        endpoints: Vec<WatchdogRelayStats>,
    },
    OwnerLost {
        timeout_ms: u64,
    },
    OwnerHeartbeatExpired,
}

impl WatchdogEvent {
    pub fn provider_id(&self) -> Option<&str> {
        match self {
            WatchdogEvent::ProcessStarted { provider_id, .. }
            | WatchdogEvent::ProcessSnapshot { provider_id, .. }
            | WatchdogEvent::ProcessLog { provider_id, .. }
            | WatchdogEvent::ProcessExit { provider_id, .. }
            | WatchdogEvent::ProcessError { provider_id, .. }
            | WatchdogEvent::RelayError { provider_id, .. }
            | WatchdogEvent::RelayStats { provider_id, .. } => Some(provider_id),
            WatchdogEvent::OwnerAttached { .. }
            | WatchdogEvent::OwnerLost { .. }
            | WatchdogEvent::OwnerHeartbeatExpired => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_start_process_uses_current_envelope() {
        let frame = WatchdogFrame::request(
            "req-1",
            7,
            WatchdogRequest::StartProcess(WatchdogStartProcessRequest {
                provider_id: "provider-a".into(),
                tunnel_id: "tunnel-a".into(),
                program: "/bin/provider-worker".into(),
                args: vec!["--config".into(), "/tmp/tunnel.json".into()],
                env: vec![WatchdogEnvVar {
                    name: "PROVIDER_HOME".into(),
                    value: "/tmp/provider-home".into(),
                }],
                stop_strategy: Some(Box::new(WatchdogStopStrategy::Http {
                    request: WatchdogHttpRequest {
                        method: "POST".into(),
                        url: "http://127.0.0.1:7400/api/stop".into(),
                        headers: vec![WatchdogHttpHeader {
                            name: "Authorization".into(),
                            value: "Basic abc".into(),
                        }],
                        body: None,
                        accepted_statuses: Vec::new(),
                        json_expectation: None,
                        timeout_ms: Some(600),
                    },
                    grace_ms: Some(2000),
                })),
                cleanup: None,
            }),
        );

        let value = serde_json::to_value(&frame).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "type": "request",
                "version": 7,
                "id": "req-1",
                "seq": 7,
                "name": "start_process",
                "payload": {
                    "providerId": "provider-a",
                    "tunnelId": "tunnel-a",
                    "program": "/bin/provider-worker",
                    "args": ["--config", "/tmp/tunnel.json"],
                    "env": [{
                        "name": "PROVIDER_HOME",
                        "value": "/tmp/provider-home"
                    }],
                    "stopStrategy": {
                        "kind": "http",
                        "request": {
                            "method": "POST",
                            "url": "http://127.0.0.1:7400/api/stop",
                            "headers": [{
                                "name": "Authorization",
                                "value": "Basic abc"
                            }],
                            "body": null,
                            "acceptedStatuses": [],
                            "jsonExpectation": null,
                            "timeoutMs": 600
                        },
                        "graceMs": 2000
                    },
                    "cleanup": null
                }
            })
        );
        let back: WatchdogFrame = serde_json::from_value(value).unwrap();
        assert_eq!(back, frame);
    }

    #[test]
    fn response_and_event_round_trip() {
        let frames = [
            WatchdogFrame::response(
                "req-1",
                8,
                WatchdogResponse::Accepted {
                    request: "start_process".into(),
                },
            ),
            WatchdogFrame::event(
                "event-1",
                9,
                WatchdogEvent::ProcessStarted {
                    provider_id: "provider-a".into(),
                    tunnel_id: "p1".into(),
                    pid: 1234,
                },
            ),
            WatchdogFrame::event(
                "event-2",
                10,
                WatchdogEvent::ProcessSnapshot {
                    provider_id: "provider-a".into(),
                    tunnel_id: "p1".into(),
                    pid: 1234,
                    args: vec!["--config".into(), "/tmp/tunnel.yml".into()],
                    recent_logs: vec!["public URL ready".into()],
                },
            ),
            WatchdogFrame::event(
                "event-3",
                11,
                WatchdogEvent::ProcessExit {
                    provider_id: "provider-a".into(),
                    tunnel_id: "p1".into(),
                    success: true,
                    forced: true,
                    code: Some(0),
                    cleanup_success: Some(true),
                    cleanup_error: None,
                },
            ),
        ];
        for frame in frames {
            let json = serde_json::to_string(&frame).unwrap();
            let back: WatchdogFrame = serde_json::from_str(&json).unwrap();
            assert_eq!(back, frame);
        }
    }

    #[test]
    fn frame_reports_unsupported_protocol_versions() {
        let frame = WatchdogFrame::request("req-1", 1, WatchdogRequest::Shutdown);
        assert!(frame.supported_version());
        let value = serde_json::json!({
            "type": "request",
            "version": 99,
            "id": "req-1",
            "seq": 1,
            "name": "shutdown"
        });
        let back: WatchdogFrame = serde_json::from_value(value).unwrap();
        assert_eq!(back.version(), 99);
        assert!(!back.supported_version());
    }

    #[test]
    fn cleanup_plan_round_trips_generic_actions() {
        let plan = WatchdogCleanupPlan {
            actions: vec![
                WatchdogCleanupAction::RemoveFile {
                    path: "/tmp/a.json".into(),
                },
                WatchdogCleanupAction::Command {
                    program: "provider".into(),
                    args: vec!["cleanup".into()],
                    env: vec![WatchdogEnvVar {
                        name: "TOKEN".into(),
                        value: "secret".into(),
                    }],
                    timeout_ms: Some(1000),
                    accepted_codes: vec![0, 1],
                },
            ],
            retry_attempts: Some(2),
            retry_delay_ms: Some(250),
        };

        let value = serde_json::to_value(&plan).unwrap();
        let back: WatchdogCleanupPlan = serde_json::from_value(value).unwrap();
        assert_eq!(back, plan);
    }
}
