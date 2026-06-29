use crate::domain::{gen_id, is_false, now_ms};
use serde::{Deserialize, Serialize};

fn default_service() -> String {
    "http://localhost:8080".into()
}

fn is_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CloudflareAuthMode {
    #[default]
    Authorization,
    ApiToken,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareAccountConfig {
    #[serde(default)]
    pub auth_mode: CloudflareAuthMode,
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub token_account_id: String,
    #[serde(default)]
    pub token_account_name: String,
    #[serde(default)]
    pub authorization_account_id: String,
    #[serde(default)]
    pub authorization_zone_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CloudflareCleanupKind {
    #[default]
    RemoteTunnel,
    DnsRoutes,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CloudflarePendingRemoteCleanup {
    #[serde(default)]
    pub kind: CloudflareCleanupKind,
    #[serde(default)]
    pub tunnel_id: String,
    #[serde(default)]
    pub tunnel_name: String,
    #[serde(default)]
    pub account: CloudflareAccountConfig,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub credentials_file: String,
    #[serde(default)]
    pub config_file: String,
    #[serde(default)]
    pub dns_hostnames: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareIngressRule {
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default = "default_service")]
    pub service: String,
    #[serde(skip)]
    pub runtime_http_host_header: String,
    #[serde(default = "is_true", skip_serializing_if = "is_false")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dns_routed: bool,
}

impl Default for CloudflareIngressRule {
    fn default() -> Self {
        Self {
            id: gen_id(),
            name: String::new(),
            hostname: String::new(),
            service: default_service(),
            runtime_http_host_header: String::new(),
            enabled: true,
            dns_routed: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareTunnel {
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub account: CloudflareAccountConfig,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub tunnel_id: String,
    #[serde(default)]
    pub credentials_file: String,
    #[serde(default)]
    pub config_file: String,
    #[serde(default)]
    pub ingress: Vec<CloudflareIngressRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_remote_cleanup: Vec<CloudflarePendingRemoteCleanup>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl CloudflareTunnel {
    pub fn new(name: impl Into<String>) -> Self {
        let ts = now_ms();
        Self {
            id: gen_id(),
            name: name.into(),
            account: CloudflareAccountConfig::default(),
            cert_file: String::new(),
            tunnel_id: String::new(),
            credentials_file: String::new(),
            config_file: String::new(),
            ingress: Vec::new(),
            pending_remote_cleanup: Vec::new(),
            created_at: ts,
            updated_at: ts,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_ms();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareData {
    #[serde(default)]
    pub tunnels: Vec<CloudflareTunnel>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflaredStatus {
    pub available: bool,
    pub managed: bool,
    pub version: Option<String>,
    pub path: String,
    pub cert_path: Option<String>,
    pub home_credentials_dir: Option<String>,
    pub managed_credentials_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareAccount {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareZone {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudflareCommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
