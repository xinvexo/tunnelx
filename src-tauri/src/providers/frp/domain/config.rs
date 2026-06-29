//! Profile -> frpc 客户端配置（frp v1 JSON）的转换。
//!
//! `build_frpc_json` 是 frpc_service 写运行期配置的核心序列化器。
use super::Profile;
use crate::error::AppResult;

/// 把一个 Profile 序列化为 frpc 可直接加载的客户端配置 JSON：
/// 通用配置（ServerConfig，字段与 frp json tag 一一对应）+ 已启用隧道的 `proxies` 数组。
/// 禁用的隧道不写出（frp 没有 enabled 概念，靠省略实现）。
pub fn build_frpc_json(profile: &Profile) -> AppResult<String> {
    let mut root = serde_json::to_value(&profile.server)?;
    let proxies = profile
        .proxies
        .iter()
        .filter(|proxy| proxy.enabled)
        .map(|proxy| serde_json::to_value(&proxy.config))
        .collect::<Result<Vec<_>, _>>()?;
    if let serde_json::Value::Object(map) = &mut root {
        map.insert("proxies".to_string(), serde_json::Value::Array(proxies));
    }
    Ok(serde_json::to_string_pretty(&root)?)
}
