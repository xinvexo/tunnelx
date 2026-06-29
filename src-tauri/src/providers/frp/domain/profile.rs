use super::{ProfileProxy, ServerConfig};
use crate::domain::{gen_id, now_ms};
use serde::{Deserialize, Serialize};

/// 一个 frps 连接配置：服务器设置 + 该服务器下的隧道。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(default = "gen_id")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub proxies: Vec<ProfileProxy>,
    #[serde(default)]
    pub frpc_version: Option<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        let ts = now_ms();
        Self {
            id: gen_id(),
            name: name.into(),
            server: ServerConfig::preset(),
            proxies: Vec::new(),
            frpc_version: None,
            created_at: ts,
            updated_at: ts,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_ms();
    }
}

/// 列表页用的轻量摘要。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummary {
    pub id: String,
    pub name: String,
    pub server_addr: String,
    pub server_port: u16,
    pub proxy_count: usize,
    pub enabled_proxy_count: usize,
    pub frpc_version: Option<String>,
    pub updated_at: i64,
}

impl From<&Profile> for ProfileSummary {
    fn from(p: &Profile) -> Self {
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            server_addr: p.server.server_addr.clone(),
            server_port: p.server.server_port,
            proxy_count: p.proxies.len(),
            enabled_proxy_count: p.proxies.iter().filter(|x| x.enabled).count(),
            frpc_version: p.frpc_version.clone(),
            updated_at: p.updated_at,
        }
    }
}
