use crate::error::{AppError, AppResult};
use crate::paths;
use crate::providers::cloudflare::domain::{CloudflareIngressRule, CloudflareTunnel};
use crate::providers::cloudflare::paths as cloudflare_paths;
use std::fs;
use tauri::AppHandle;

pub(crate) fn ensure_ids(ingress: Vec<CloudflareIngressRule>) -> Vec<CloudflareIngressRule> {
    ingress
        .into_iter()
        .map(|mut item| {
            if item.id.trim().is_empty() {
                item.id = crate::domain::gen_id();
            }
            item.hostname = item.hostname.trim().to_string();
            item.service = item.service.trim().to_string();
            item
        })
        .filter(|item| !item.hostname.is_empty() || !item.service.is_empty())
        .collect()
}

fn yaml_string(value: &str) -> AppResult<String> {
    let encoded = serde_yaml_ng::to_string(value).map_err(|e| AppError::Msg(e.to_string()))?;
    Ok(encoded.trim().to_string())
}

pub(crate) fn write_tunnel_config(app: &AppHandle, tunnel: &CloudflareTunnel) -> AppResult<String> {
    let dir = cloudflare_paths::configs_dir(app)?;
    fs::create_dir_all(&dir)?;
    let stem = if tunnel.tunnel_id.trim().is_empty() {
        cloudflare_paths::sanitize_filename(&tunnel.name)
    } else {
        cloudflare_paths::sanitize_filename(&tunnel.tunnel_id)
    };
    let path = dir.join(format!("{stem}.yml"));
    let text = tunnel_config_text(tunnel)?;
    paths::write_secret_file(&path, text.as_bytes())?;
    Ok(path.to_string_lossy().to_string())
}

fn tunnel_config_text(tunnel: &CloudflareTunnel) -> AppResult<String> {
    let tunnel_ref = if tunnel.tunnel_id.trim().is_empty() {
        tunnel.name.trim()
    } else {
        tunnel.tunnel_id.trim()
    };
    let mut text = String::new();
    text.push_str(&format!("tunnel: {}\n", yaml_string(tunnel_ref)?));
    if !tunnel.credentials_file.trim().is_empty() {
        text.push_str(&format!(
            "credentials-file: {}\n",
            yaml_string(tunnel.credentials_file.trim())?
        ));
    }
    text.push_str("ingress:\n");
    for rule in tunnel.ingress.iter().filter(|rule| {
        rule.enabled && !rule.hostname.trim().is_empty() && !rule.service.trim().is_empty()
    }) {
        text.push_str(&format!(
            "  - hostname: {}\n",
            yaml_string(rule.hostname.trim())?
        ));
        text.push_str(&format!(
            "    service: {}\n",
            yaml_string(rule.service.trim())?
        ));
        if !rule.runtime_http_host_header.trim().is_empty() {
            text.push_str("    originRequest:\n");
            text.push_str(&format!(
                "      httpHostHeader: {}\n",
                yaml_string(rule.runtime_http_host_header.trim())?
            ));
        }
    }
    text.push_str("  - service: http_status:404\n");
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_config_lets_cloudflared_choose_protocol() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.tunnel_id = "remote-id".into();
        tunnel.credentials_file = "/tmp/remote-id.json".into();
        tunnel.ingress = vec![CloudflareIngressRule {
            hostname: "test.example.com".into(),
            service: "http://localhost:8080".into(),
            ..Default::default()
        }];

        let text = tunnel_config_text(&tunnel).unwrap();

        assert!(!text.contains("protocol:"));
        assert!(text.contains("tunnel: remote-id\n"));
        assert!(text.contains("credentials-file: /tmp/remote-id.json\n"));
    }

    #[test]
    fn tunnel_config_writes_runtime_http_host_header() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.tunnel_id = "remote-id".into();
        tunnel.ingress = vec![CloudflareIngressRule {
            hostname: "test.example.com".into(),
            service: "http://127.0.0.1:60998".into(),
            runtime_http_host_header: "127.0.0.1:1420".into(),
            ..Default::default()
        }];

        let text = tunnel_config_text(&tunnel).unwrap();

        assert!(text.contains("    service: http://127.0.0.1:60998\n"));
        assert!(text.contains("    originRequest:\n"));
        assert!(text.contains("      httpHostHeader: 127.0.0.1:1420\n"));
    }
}
