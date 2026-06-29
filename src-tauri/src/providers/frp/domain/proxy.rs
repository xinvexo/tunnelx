//! 隧道（代理）配置，对应 frp v1 的 ProxyConfigurer 各实现。字段名与 frp json tag 对齐。
use super::types::{gen_id, is_false};
use super::PluginOptions;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

type Map = HashMap<String, String>;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HeaderOps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<Map>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyTransport {
    #[serde(default, skip_serializing_if = "is_false")]
    pub use_encryption: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub use_compression: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bandwidth_limit: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub bandwidth_limit_mode: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub proxy_protocol_version: String,
}

impl ProxyTransport {
    fn is_empty(&self) -> bool {
        !self.use_encryption
            && !self.use_compression
            && self.bandwidth_limit.is_empty()
            && self.bandwidth_limit_mode.is_empty()
            && self.proxy_protocol_version.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    /// "tcp" | "http" | ""（空=不启用）
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_failed: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_seconds: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub http_headers: Vec<HttpHeader>,
}

impl HealthCheck {
    fn is_empty(&self) -> bool {
        self.r#type.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoadBalancer {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group_key: String,
}

impl LoadBalancer {
    fn is_empty(&self) -> bool {
        self.group.is_empty() && self.group_key.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NatTraversal {
    #[serde(default, skip_serializing_if = "is_false")]
    pub disable_assisted_addrs: bool,
}

/// 所有隧道共有字段（frp ProxyBaseConfig + ProxyBackend，去掉 type/enabled）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyBase {
    pub name: String,
    #[serde(default, skip_serializing_if = "ProxyTransport::is_empty")]
    pub transport: ProxyTransport,
    #[serde(default, skip_serializing_if = "HealthCheck::is_empty")]
    pub health_check: HealthCheck,
    #[serde(default, skip_serializing_if = "LoadBalancer::is_empty")]
    pub load_balancer: LoadBalancer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadatas: Option<Map>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Map>,
    #[serde(rename = "localIP", default, skip_serializing_if = "String::is_empty")]
    pub local_ip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PluginOptions>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TcpProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UdpProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HttpProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subdomain: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub http_user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub http_password: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_header_rewrite: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_headers: Option<HeaderOps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_headers: Option<HeaderOps>,
    #[serde(
        rename = "routeByHTTPUser",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub route_by_http_user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HttpsProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subdomain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TcpmuxProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_domains: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub subdomain: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub http_user: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub http_password: String,
    #[serde(
        rename = "routeByHTTPUser",
        default,
        skip_serializing_if = "String::is_empty"
    )]
    pub route_by_http_user: String,
    /// 目前仅 "httpconnect"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub multiplexer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StcpProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secret_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SudpProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secret_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct XtcpProxy {
    #[serde(flatten)]
    pub base: ProxyBase,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub secret_key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_users: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nat_traversal: Option<NatTraversal>,
}

/// 隧道配置：内部按 "type" 标签区分，序列化即 frp 期望的扁平对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProxyConfig {
    Tcp(TcpProxy),
    Udp(UdpProxy),
    Http(HttpProxy),
    Https(HttpsProxy),
    Tcpmux(TcpmuxProxy),
    Stcp(StcpProxy),
    Sudp(SudpProxy),
    Xtcp(XtcpProxy),
}

impl ProxyConfig {
    pub fn base(&self) -> &ProxyBase {
        match self {
            ProxyConfig::Tcp(p) => &p.base,
            ProxyConfig::Udp(p) => &p.base,
            ProxyConfig::Http(p) => &p.base,
            ProxyConfig::Https(p) => &p.base,
            ProxyConfig::Tcpmux(p) => &p.base,
            ProxyConfig::Stcp(p) => &p.base,
            ProxyConfig::Sudp(p) => &p.base,
            ProxyConfig::Xtcp(p) => &p.base,
        }
    }

    pub fn base_mut(&mut self) -> &mut ProxyBase {
        match self {
            ProxyConfig::Tcp(p) => &mut p.base,
            ProxyConfig::Udp(p) => &mut p.base,
            ProxyConfig::Http(p) => &mut p.base,
            ProxyConfig::Https(p) => &mut p.base,
            ProxyConfig::Tcpmux(p) => &mut p.base,
            ProxyConfig::Stcp(p) => &mut p.base,
            ProxyConfig::Sudp(p) => &mut p.base,
            ProxyConfig::Xtcp(p) => &mut p.base,
        }
    }
}

/// 持久化层的隧道：在 frp 配置外面包一层 id/enabled，供 UI 使用；生成运行配置时只写 config。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileProxy {
    #[serde(default = "gen_id")]
    pub id: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub config: ProxyConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_proxy_roundtrip() {
        let base = ProxyBase {
            name: "web".into(),
            local_ip: "127.0.0.1".into(),
            local_port: Some(80),
            ..Default::default()
        };
        let p = ProxyConfig::Http(HttpProxy {
            base,
            subdomain: "app".into(),
            custom_domains: vec!["a.com".into()],
            ..Default::default()
        });
        let j = serde_json::to_string(&p).unwrap();
        assert!(j.contains("\"type\":\"http\""), "json={j}");
        assert!(j.contains("\"name\":\"web\""));
        assert!(j.contains("\"localIP\":\"127.0.0.1\""));
        assert!(j.contains("\"subdomain\":\"app\""));
        // 不该出现 tcp 专属字段
        assert!(!j.contains("remotePort"));

        let back: ProxyConfig = serde_json::from_str(&j).unwrap();
        match back {
            ProxyConfig::Http(h) => {
                assert_eq!(h.base.name, "web");
                assert_eq!(h.base.local_port, Some(80));
                assert_eq!(h.subdomain, "app");
            }
            _ => panic!("Expected Http variant, got {:?}", back),
        }
    }

    #[test]
    fn profile_proxy_wrapper_roundtrip() {
        let p = ProfileProxy {
            id: "x".into(),
            enabled: true,
            config: ProxyConfig::Tcp(TcpProxy {
                base: ProxyBase {
                    name: "ssh".into(),
                    local_port: Some(22),
                    ..Default::default()
                },
                remote_port: Some(6000),
            }),
        };
        let j = serde_json::to_string(&p).unwrap();
        let back: ProfileProxy = serde_json::from_str(&j).unwrap();
        assert_eq!(back.id, "x");
        assert!(matches!(back.config, ProxyConfig::Tcp(_)));
    }
}
