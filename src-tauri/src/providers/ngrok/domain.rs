use crate::domain::{gen_id, is_false, now_ms};
use serde::{Deserialize, Serialize};

fn default_addr() -> String {
    "http://localhost:8080".into()
}

fn default_proto() -> String {
    "http".into()
}

fn is_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NgrokEndpoint {
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default = "default_proto")]
    pub proto: String,
    #[serde(default = "default_addr")]
    pub addr: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default = "is_true", skip_serializing_if = "is_false")]
    pub enabled: bool,
}

impl Default for NgrokEndpoint {
    fn default() -> Self {
        Self {
            id: gen_id(),
            name: String::new(),
            proto: default_proto(),
            addr: default_addr(),
            domain: String::new(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NgrokTunnel {
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub authtoken: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub config_file: String,
    #[serde(default)]
    pub endpoints: Vec<NgrokEndpoint>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl NgrokTunnel {
    pub fn new(name: impl Into<String>) -> Self {
        let ts = now_ms();
        Self {
            id: gen_id(),
            name: name.into(),
            authtoken: String::new(),
            region: String::new(),
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
pub struct NgrokData {
    #[serde(default)]
    pub tunnels: Vec<NgrokTunnel>,
}
