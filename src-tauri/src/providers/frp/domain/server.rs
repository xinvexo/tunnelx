//! frpc 客户端通用配置（对应 frp v1 ClientCommonConfig）。字段名与 frp 的 json tag 一一对应。
//! 注意：默认值为 true 的开关用 Option<bool>，None 时不写出，让 frpc 用它自己的默认值。
use super::types::{is_false, AuthMethod};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type Map = HashMap<String, String>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Oidc {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub client_secret: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub audience: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scope: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token_endpoint_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_endpoint_params: Option<Map>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trusted_ca_file: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub insecure_skip_verify: bool,
}

impl Oidc {
    fn is_empty(&self) -> bool {
        self.client_id.is_empty()
            && self.client_secret.is_empty()
            && self.audience.is_empty()
            && self.scope.is_empty()
            && self.token_endpoint_url.is_empty()
            && self.additional_endpoint_params.is_none()
            && self.trusted_ca_file.is_empty()
            && !self.insecure_skip_verify
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Auth {
    /// 缺省为 token：真实世界的 frpc 配置普遍只写 auth.token 不写 auth.method
    /// （frp 自身也默认 token），读取已有配置形态时必须能解析。
    #[serde(default)]
    pub method: AuthMethod,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub token: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub additional_scopes: Vec<String>,
    #[serde(default, skip_serializing_if = "Oidc::is_empty")]
    pub oidc: Oidc,
}

/// TLS 证书相关文件（frp TLSConfig），在 transport.tls 与 webServer.tls 复用。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TlsFiles {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cert_file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key_file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub trusted_ca_file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Tls {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disable_custom_tls_first_byte: Option<bool>,
    #[serde(flatten)]
    pub files: TlsFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Quic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keepalive_period: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_idle_timeout: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_incoming_streams: Option<i32>,
}

impl Quic {
    fn is_empty(&self) -> bool {
        self.keepalive_period.is_none()
            && self.max_idle_timeout.is_none()
            && self.max_incoming_streams.is_none()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Transport {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub protocol: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dial_server_timeout: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dial_server_keepalive: Option<i64>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub connect_server_local_ip: String,
    #[serde(rename = "proxyURL", default, skip_serializing_if = "String::is_empty")]
    pub proxy_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_mux: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcp_mux_keepalive_interval: Option<i64>,
    #[serde(default, skip_serializing_if = "Quic::is_empty")]
    pub quic: Quic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_timeout: Option<i64>,
    #[serde(default)]
    pub tls: Tls,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Log {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub to: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_days: Option<i64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disable_print_color: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebServer {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub addr: String,
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub port: u16,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub assets_dir: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pprof_enable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsFiles>,
}

fn is_zero_u16(n: &u16) -> bool {
    *n == 0
}

/// frpc 通用配置。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(rename = "clientID", default, skip_serializing_if = "String::is_empty")]
    pub client_id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server_addr: String,
    #[serde(default, skip_serializing_if = "is_zero_u16")]
    pub server_port: u16,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub transport: Transport,
    #[serde(default)]
    pub log: Log,
    #[serde(default)]
    pub web_server: WebServer,
    #[serde(
        rename = "dnsServer",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub dns_server: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nat_hole_stun_server: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub login_fail_exit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udp_packet_size: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadatas: Option<Map>,
}

impl ServerConfig {
    /// 新建 Profile 时的合理初值（不写满 frp 默认，仅给最常用的）。
    pub fn preset() -> Self {
        Self {
            server_addr: "127.0.0.1".into(),
            server_port: 7000,
            transport: Transport {
                protocol: "tcp".into(),
                dial_server_timeout: Some(10),
                dial_server_keepalive: Some(7200),
                pool_count: Some(4),
                heartbeat_interval: Some(30),
                heartbeat_timeout: Some(90),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}
