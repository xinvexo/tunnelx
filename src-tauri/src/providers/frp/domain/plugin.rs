//! frpc 客户端插件（对应 frp v1 ClientPluginOptions）。设置 plugin 后 localIP/localPort 被忽略。
use super::proxy::HeaderOps;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginOptions {
    #[serde(rename = "http_proxy", rename_all = "camelCase")]
    HttpProxy {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        http_user: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        http_password: String,
    },
    #[serde(rename = "socks5", rename_all = "camelCase")]
    Socks5 {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        username: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        password: String,
    },
    #[serde(rename = "static_file", rename_all = "camelCase")]
    StaticFile {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        local_path: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        strip_prefix: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        http_user: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        http_password: String,
    },
    #[serde(rename = "unix_domain_socket", rename_all = "camelCase")]
    UnixDomainSocket {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        unix_path: String,
    },
    #[serde(rename = "http2http", rename_all = "camelCase")]
    Http2Http {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        local_addr: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        host_header_rewrite: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_headers: Option<HeaderOps>,
    },
    #[serde(rename = "http2https", rename_all = "camelCase")]
    Http2Https {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        local_addr: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        host_header_rewrite: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_headers: Option<HeaderOps>,
    },
    #[serde(rename = "https2http", rename_all = "camelCase")]
    Https2Http {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        local_addr: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        host_header_rewrite: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_headers: Option<HeaderOps>,
        #[serde(
            rename = "enableHTTP2",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        enable_http2: Option<bool>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        crt_path: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        key_path: String,
    },
    #[serde(rename = "https2https", rename_all = "camelCase")]
    Https2Https {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        local_addr: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        host_header_rewrite: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_headers: Option<HeaderOps>,
        #[serde(
            rename = "enableHTTP2",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        enable_http2: Option<bool>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        crt_path: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        key_path: String,
    },
    #[serde(rename = "tls2raw", rename_all = "camelCase")]
    Tls2Raw {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        local_addr: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        crt_path: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        key_path: String,
    },
    #[serde(rename = "virtual_net")]
    VirtualNet {},
}
