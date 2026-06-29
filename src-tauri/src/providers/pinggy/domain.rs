use crate::domain::{gen_id, is_false, now_ms};
use serde::{Deserialize, Serialize};

fn default_local_addr() -> String {
    "http://localhost:8080".into()
}

fn default_tunnel_type() -> String {
    "http".into()
}

fn default_server() -> String {
    "free.pinggy.io".into()
}

fn default_server_port() -> u16 {
    443
}

fn is_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinggyEndpoint {
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default = "default_tunnel_type")]
    pub tunnel_type: String,
    #[serde(default = "default_local_addr")]
    pub local_addr: String,
    #[serde(default = "is_true", skip_serializing_if = "is_false")]
    pub enabled: bool,
}

impl Default for PinggyEndpoint {
    fn default() -> Self {
        Self {
            id: gen_id(),
            name: String::new(),
            tunnel_type: default_tunnel_type(),
            local_addr: default_local_addr(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PinggyTunnel {
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_server")]
    pub server: String,
    #[serde(default = "default_server_port")]
    pub server_port: u16,
    #[serde(default)]
    pub debugger_port: Option<u16>,
    #[serde(default)]
    pub config_file: String,
    #[serde(default)]
    pub endpoints: Vec<PinggyEndpoint>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl PinggyTunnel {
    pub fn new(name: impl Into<String>) -> Self {
        let ts = now_ms();
        Self {
            id: gen_id(),
            name: name.into(),
            token: String::new(),
            server: default_server(),
            server_port: default_server_port(),
            debugger_port: None,
            config_file: String::new(),
            endpoints: Vec::new(),
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
pub struct PinggyData {
    #[serde(default)]
    pub tunnels: Vec<PinggyTunnel>,
}
