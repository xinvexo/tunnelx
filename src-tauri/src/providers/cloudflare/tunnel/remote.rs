use super::files::{is_current_tunnel_file, remove_managed_file};
use super::hostname::normalize_hostname;
use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::account::credentials;
use crate::providers::cloudflare::data::persistence::save_change;
use crate::providers::cloudflare::domain::{
    CloudflareAccountConfig, CloudflareAuthMode, CloudflareCleanupKind, CloudflareIngressRule,
    CloudflarePendingRemoteCleanup, CloudflareTunnel,
};
use crate::providers::cloudflare::environment::cli::run_cloudflared_with_cert;
use crate::providers::cloudflare::paths;
use crate::state::AppState;
use std::path::Path;
use tauri::{AppHandle, Runtime};
use uuid::Uuid;

pub(super) fn has_valid_remote_tunnel_id(tunnel: &CloudflareTunnel) -> bool {
    let tunnel_id = tunnel.tunnel_id.trim();
    !tunnel_id.is_empty() && tunnel_id != tunnel.id && has_valid_cloudflare_tunnel_id(tunnel_id)
}

pub(super) fn has_valid_cloudflare_tunnel_id(tunnel_id: &str) -> bool {
    Uuid::parse_str(tunnel_id.trim()).is_ok()
}

pub(super) fn credentials_remote_tunnel_id(tunnel: &CloudflareTunnel) -> Option<String> {
    let credentials_file = tunnel.credentials_file.trim();
    if credentials_file.is_empty() {
        return None;
    }
    credentials::credentials_tunnel_id(Path::new(credentials_file))
        .filter(|id| has_valid_cloudflare_tunnel_id(id))
}

pub(super) fn lookup_remote_tunnel_id(tunnel: &CloudflareTunnel) -> AppResult<Option<String>> {
    Ok(
        tauri::async_runtime::block_on(super::api::remote_tunnel_id_for_tunnel(
            tunnel,
            &tunnel.name,
        ))?
        .filter(|id| has_valid_cloudflare_tunnel_id(id)),
    )
}

pub(super) fn clear_local_remote_binding(tunnel: &mut CloudflareTunnel) {
    tunnel.tunnel_id.clear();
    tunnel.credentials_file.clear();
    for rule in &mut tunnel.ingress {
        rule.dns_routed = false;
    }
}

fn has_remote_release_evidence(tunnel: &CloudflareTunnel) -> bool {
    !tunnel.credentials_file.trim().is_empty() || tunnel.ingress.iter().any(|rule| rule.dns_routed)
}

fn remote_tunnel_id_for_release(tunnel: &CloudflareTunnel) -> AppResult<String> {
    if has_valid_remote_tunnel_id(tunnel) {
        return Ok(tunnel.tunnel_id.trim().to_string());
    }
    if let Some(tunnel_id) = credentials_remote_tunnel_id(tunnel) {
        return Ok(tunnel_id);
    }
    if has_remote_release_evidence(tunnel) {
        return Ok(lookup_remote_tunnel_id(tunnel)?.unwrap_or_default());
    }
    Ok(String::new())
}

fn cleanup_account_snapshot(tunnel: &CloudflareTunnel) -> CloudflareAccountConfig {
    if tunnel.account.auth_mode == CloudflareAuthMode::Authorization {
        if let Ok(cert) = credentials::read_origin_cert(&tunnel.cert_file) {
            return CloudflareAccountConfig {
                auth_mode: CloudflareAuthMode::ApiToken,
                api_token: cert.api_token,
                token_account_id: cert.account_id,
                token_account_name: String::new(),
                authorization_account_id: String::new(),
                authorization_zone_id: String::new(),
            };
        }
    }
    tunnel.account.clone()
}

pub(super) fn cleanup_hostnames(tunnel: &CloudflareTunnel, only_routed: bool) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    tunnel
        .ingress
        .iter()
        .filter(|rule| !only_routed || rule.dns_routed)
        .map(|rule| normalize_hostname(&rule.hostname))
        .filter(|hostname| !hostname.is_empty() && seen.insert(hostname.clone()))
        .collect()
}

pub(super) fn pending_remote_cleanup_target(
    tunnel: &CloudflareTunnel,
    kind: CloudflareCleanupKind,
    dns_hostnames: Vec<String>,
) -> Option<CloudflarePendingRemoteCleanup> {
    let dns_hostnames = normalize_cleanup_hostnames(dns_hostnames);
    let tunnel_id = if has_valid_remote_tunnel_id(tunnel) {
        tunnel.tunnel_id.trim().to_string()
    } else {
        credentials_remote_tunnel_id(tunnel).unwrap_or_default()
    };
    let has_remote_target = !tunnel_id.is_empty()
        || !tunnel.credentials_file.trim().is_empty()
        || !dns_hostnames.is_empty();
    if !has_remote_target {
        return None;
    }
    let account = cleanup_account_snapshot(tunnel);
    let needs_cert = account.auth_mode == CloudflareAuthMode::Authorization;
    Some(CloudflarePendingRemoteCleanup {
        kind,
        tunnel_id,
        tunnel_name: tunnel.name.trim().to_string(),
        account,
        cert_file: if kind == CloudflareCleanupKind::RemoteTunnel || needs_cert {
            tunnel.cert_file.trim().to_string()
        } else {
            String::new()
        },
        credentials_file: if kind == CloudflareCleanupKind::RemoteTunnel {
            tunnel.credentials_file.trim().to_string()
        } else {
            String::new()
        },
        config_file: if kind == CloudflareCleanupKind::RemoteTunnel {
            tunnel.config_file.trim().to_string()
        } else {
            String::new()
        },
        dns_hostnames,
    })
}

fn normalize_cleanup_hostnames(hostnames: Vec<String>) -> Vec<String> {
    hostnames
        .into_iter()
        .map(|hostname| normalize_hostname(&hostname))
        .filter(|hostname| !hostname.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(super) fn append_pending_remote_cleanup(
    tunnel: &mut CloudflareTunnel,
    target: CloudflarePendingRemoteCleanup,
) {
    if target.kind == CloudflareCleanupKind::RemoteTunnel {
        tunnel.pending_remote_cleanup.retain(|item| {
            item.kind != CloudflareCleanupKind::DnsRoutes || !same_cleanup_target(item, &target)
        });
    }
    if let Some(existing) = tunnel
        .pending_remote_cleanup
        .iter_mut()
        .find(|item| same_cleanup_target(item, &target) && item.kind == target.kind)
    {
        existing.dns_hostnames.extend(target.dns_hostnames);
        existing.dns_hostnames = normalize_cleanup_hostnames(existing.dns_hostnames.clone());
        return;
    }
    tunnel.pending_remote_cleanup.push(target);
}

fn same_cleanup_target(
    left: &CloudflarePendingRemoteCleanup,
    right: &CloudflarePendingRemoteCleanup,
) -> bool {
    left.tunnel_id.trim() == right.tunnel_id.trim()
        && left.tunnel_name.trim() == right.tunnel_name.trim()
        && left.account == right.account
}

pub(super) fn preserve_pending_remote_cleanup(
    previous: &CloudflareTunnel,
    next: &mut CloudflareTunnel,
) {
    if next.pending_remote_cleanup.is_empty() && !previous.pending_remote_cleanup.is_empty() {
        next.pending_remote_cleanup = previous.pending_remote_cleanup.clone();
    }
}

fn pending_cleanup_tunnel(target: &CloudflarePendingRemoteCleanup) -> CloudflareTunnel {
    let mut tunnel = CloudflareTunnel::new(target.tunnel_name.trim());
    tunnel.account = target.account.clone();
    tunnel.cert_file = target.cert_file.clone();
    tunnel.tunnel_id = target.tunnel_id.clone();
    tunnel.credentials_file = target.credentials_file.clone();
    tunnel.config_file = target.config_file.clone();
    tunnel.ingress = target
        .dns_hostnames
        .iter()
        .map(|hostname| CloudflareIngressRule {
            hostname: hostname.clone(),
            service: String::new(),
            dns_routed: true,
            ..Default::default()
        })
        .collect();
    tunnel
}

pub(crate) fn release_remote_resources(
    app: &AppHandle<impl Runtime>,
    tunnel: &mut CloudflareTunnel,
) -> AppResult<bool> {
    let remote_tunnel_id = remote_tunnel_id_for_release(tunnel)?;
    let allow_name_fallback = remote_tunnel_id.is_empty() && has_remote_release_evidence(tunnel);

    let mut attempted_release = false;
    let mut dns_released = remote_tunnel_id.is_empty();
    let mut named_tunnel_released = false;
    let mut dns_error: Option<AppError> = None;
    let mut named_tunnel_error: Option<AppError> = None;
    if !remote_tunnel_id.is_empty() {
        attempted_release = true;
        let hostnames = tunnel
            .ingress
            .iter()
            .map(|rule| rule.hostname.trim().to_string())
            .filter(|hostname| !hostname.is_empty())
            .collect::<Vec<_>>();
        let dns_result = tauri::async_runtime::block_on(super::api::delete_dns_routes_for_tunnel(
            tunnel,
            &remote_tunnel_id,
            &hostnames,
        ));
        let tunnel_result = tauri::async_runtime::block_on(
            super::api::delete_remote_tunnel_for_tunnel(tunnel, &remote_tunnel_id),
        );
        match dns_result {
            Ok(_) => dns_released = true,
            Err(error) => dns_error = Some(error),
        }
        match tunnel_result {
            Ok(_) => named_tunnel_released = true,
            Err(error) => named_tunnel_error = Some(error),
        }
    }

    if !(dns_released && named_tunnel_released) {
        if tunnel.account.auth_mode == CloudflareAuthMode::ApiToken {
            return if let Some(error) = cleanup_error(dns_error, named_tunnel_error) {
                Err(error)
            } else {
                Ok(false)
            };
        }

        let target = if named_tunnel_released {
            ""
        } else if !remote_tunnel_id.is_empty() {
            remote_tunnel_id.as_str()
        } else if allow_name_fallback {
            tunnel.name.trim()
        } else {
            ""
        };
        if !target.is_empty() {
            attempted_release = true;
            let output = run_cloudflared_with_cert(
                app,
                &["tunnel", "delete", "--force", target],
                &tunnel.cert_file,
            )?;
            if output.success
                || cloudflared_not_found(&output.stderr)
                || cloudflared_not_found(&output.stdout)
            {
                named_tunnel_released = true;
                named_tunnel_error = None;
            } else if let Some(error) = cleanup_error(dns_error.take(), named_tunnel_error.take()) {
                let detail = if output.stderr.trim().is_empty() {
                    output.stdout.trim()
                } else {
                    output.stderr.trim()
                };
                return Err(AppError::Msg(format!(
                    "Cloudflare API cleanup failed: {error}; cloudflared cleanup failed: {detail}"
                )));
            } else {
                let detail = if output.stderr.trim().is_empty() {
                    output.stdout.trim()
                } else {
                    output.stderr.trim()
                };
                return Err(AppError::Msg(format!(
                    "cloudflared cleanup failed: {detail}"
                )));
            }
        } else if let Some(error) = cleanup_error(dns_error.take(), named_tunnel_error.take()) {
            return Err(error);
        }
    }

    let released = attempted_release && dns_released && named_tunnel_released;
    if released {
        clear_local_remote_binding(tunnel);
        Ok(true)
    } else if let Some(error) = cleanup_error(dns_error, named_tunnel_error) {
        Err(error)
    } else {
        Ok(false)
    }
}

pub(crate) fn flush_pending_remote_cleanup(
    app: &AppHandle<impl Runtime>,
    state: &AppState,
    id: &str,
) -> AppResult<bool> {
    let mut tunnel = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Msg("Cloudflare tunnel not found".into()))?;
    if tunnel.pending_remote_cleanup.is_empty() {
        return Ok(false);
    }

    let pending = tunnel.pending_remote_cleanup.clone();
    let mut remaining = Vec::new();
    let mut errors = Vec::new();
    let managed_root = paths::root(app)?;
    for target in pending {
        match execute_pending_remote_cleanup(app, &target) {
            Ok(()) => remove_pending_cleanup_files(&target, &tunnel, &managed_root),
            Err(error) => {
                remaining.push(target);
                errors.push(error.to_string());
            }
        }
    }

    let changed = remaining.len() != tunnel.pending_remote_cleanup.len();
    if changed {
        tunnel.pending_remote_cleanup = remaining;
        save_change(app, state, || {
            state.config.upsert_cloudflare_tunnel(tunnel.clone())
        })?;
    }
    if errors.is_empty() {
        Ok(changed)
    } else {
        Err(AppError::Msg(format!(
            "Cloudflare pending cleanup was not fully completed: {}",
            errors.join("; ")
        )))
    }
}

fn execute_pending_remote_cleanup(
    app: &AppHandle<impl Runtime>,
    target: &CloudflarePendingRemoteCleanup,
) -> AppResult<()> {
    let mut tunnel = pending_cleanup_tunnel(target);
    match target.kind {
        CloudflareCleanupKind::RemoteTunnel => {
            let _ = release_remote_resources(app, &mut tunnel)?;
        }
        CloudflareCleanupKind::DnsRoutes => {
            let remote_tunnel_id = remote_tunnel_id_for_release(&tunnel)?;
            if remote_tunnel_id.trim().is_empty() {
                return Err(AppError::Msg(
                    "Cannot locate the Cloudflare named tunnel; pending DNS routes cannot be cleaned up".into(),
                ));
            }
            tauri::async_runtime::block_on(super::api::delete_dns_routes_for_tunnel(
                &tunnel,
                &remote_tunnel_id,
                &target.dns_hostnames,
            ))?;
        }
    }
    Ok(())
}

fn remove_pending_cleanup_files(
    target: &CloudflarePendingRemoteCleanup,
    current: &CloudflareTunnel,
    managed_root: &Path,
) {
    for path in [
        target.credentials_file.as_str(),
        target.config_file.as_str(),
        target.cert_file.as_str(),
    ] {
        if !is_current_tunnel_file(path, current) {
            remove_managed_file(managed_root, path);
        }
    }
}

fn cleanup_error(
    dns_error: Option<AppError>,
    named_tunnel_error: Option<AppError>,
) -> Option<AppError> {
    match (dns_error, named_tunnel_error) {
        (None, None) => None,
        (Some(error), None) | (None, Some(error)) => Some(error),
        (Some(dns_error), Some(named_tunnel_error)) => Some(AppError::Msg(format!(
            "DNS cleanup failed: {dns_error}; named tunnel cleanup failed: {named_tunnel_error}"
        ))),
    }
}

fn cloudflared_not_found(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("not found") || text.contains("does not exist") || text.contains("not exist")
}

pub(crate) fn remote_tunnel_missing_error(error: &AppError) -> bool {
    remote_tunnel_missing_text(&error.to_string())
}

fn remote_tunnel_missing_text(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("code: 1002")
        || text.contains("tunnel not found")
        || (text.contains("tunnel")
            && (text.contains("not found")
                || text.contains("does not exist")
                || text.contains("not exist")))
}

#[cfg(test)]
mod tests {
    use super::*;

    use base64::engine::general_purpose::STANDARD;

    use base64::Engine;

    use std::fs;

    use std::path::PathBuf;

    fn temp_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tunnelx-cloudflare-remote-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_origin_cert(path: &Path, account_id: &str, zone_id: &str, api_token: &str) {
        let cert = credentials::OriginCertInfo {
            zone_id: zone_id.into(),
            account_id: account_id.into(),
            api_token: api_token.into(),
        };
        let encoded = STANDARD.encode(serde_json::to_vec(&cert).unwrap());
        let pem = format!(
            "-----BEGIN ARGO TUNNEL TOKEN-----\n{encoded}\n-----END ARGO TUNNEL TOKEN-----"
        );
        fs::write(path, pem).unwrap();
    }

    #[test]
    fn local_config_file_is_not_remote_release_evidence() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.config_file = "/tmp/tunnelx/cloudflare/api.yml".into();

        assert!(!has_remote_release_evidence(&tunnel));
        assert_eq!(remote_tunnel_id_for_release(&tunnel).unwrap(), "");
    }

    #[test]
    fn credentials_file_is_remote_release_evidence() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.credentials_file = "/tmp/tunnelx/cloudflare/credentials.json".into();

        assert!(has_remote_release_evidence(&tunnel));
    }

    #[test]
    fn pending_remote_cleanup_snapshots_authorization_token() {
        let dir = temp_test_dir();
        let cert_file = dir.join("cert.pem");
        write_origin_cert(&cert_file, "account-1", "zone-1", "token-1");
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.account.auth_mode = CloudflareAuthMode::Authorization;
        tunnel.cert_file = cert_file.to_string_lossy().to_string();
        tunnel.tunnel_id = "11111111-1111-4111-8111-111111111111".into();
        tunnel.credentials_file = dir.join("credentials.json").to_string_lossy().to_string();
        tunnel.config_file = dir.join("config.yml").to_string_lossy().to_string();
        tunnel.ingress = vec![CloudflareIngressRule {
            hostname: "API.Example.COM.".into(),
            service: "http://localhost:8080".into(),
            dns_routed: true,
            ..Default::default()
        }];

        let target = pending_remote_cleanup_target(
            &tunnel,
            CloudflareCleanupKind::RemoteTunnel,
            cleanup_hostnames(&tunnel, false),
        )
        .unwrap();

        assert_eq!(target.account.auth_mode, CloudflareAuthMode::ApiToken);
        assert_eq!(target.account.api_token, "token-1");
        assert_eq!(target.account.token_account_id, "account-1");
        assert_eq!(target.dns_hostnames, vec!["api.example.com".to_string()]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn dns_cleanup_target_does_not_carry_runtime_files_when_token_snapshot_exists() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.account.auth_mode = CloudflareAuthMode::ApiToken;
        tunnel.account.api_token = "token".into();
        tunnel.account.token_account_id = "account".into();
        tunnel.tunnel_id = "11111111-1111-4111-8111-111111111111".into();
        tunnel.credentials_file = "/tmp/tunnelx/cloudflare/credentials/current.json".into();
        tunnel.config_file = "/tmp/tunnelx/cloudflare/configs/current.yml".into();
        tunnel.cert_file = "/tmp/tunnelx/cloudflare/certs/current.pem".into();

        let target = pending_remote_cleanup_target(
            &tunnel,
            CloudflareCleanupKind::DnsRoutes,
            vec!["Api.Example.COM.".into()],
        )
        .unwrap();

        assert_eq!(target.kind, CloudflareCleanupKind::DnsRoutes);
        assert_eq!(target.dns_hostnames, vec!["api.example.com".to_string()]);
        assert!(target.credentials_file.is_empty());
        assert!(target.config_file.is_empty());
        assert!(target.cert_file.is_empty());
    }

    #[test]
    fn pending_cleanup_does_not_snapshot_platform_connection_id() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.account.auth_mode = CloudflareAuthMode::ApiToken;
        tunnel.account.api_token = "token".into();
        tunnel.account.token_account_id = "account".into();
        tunnel.tunnel_id = tunnel.id.clone();

        let target = pending_remote_cleanup_target(
            &tunnel,
            CloudflareCleanupKind::DnsRoutes,
            vec!["api.example.com".into()],
        )
        .unwrap();

        assert!(target.tunnel_id.is_empty());
        assert_eq!(target.tunnel_name, "api");
    }

    #[test]
    fn append_pending_cleanup_merges_dns_and_prefers_remote_tunnel_cleanup() {
        let mut tunnel = CloudflareTunnel::new("api");
        let mut first = CloudflarePendingRemoteCleanup {
            kind: CloudflareCleanupKind::DnsRoutes,
            tunnel_id: "11111111-1111-4111-8111-111111111111".into(),
            tunnel_name: "api".into(),
            dns_hostnames: vec!["api.example.com".into()],
            ..Default::default()
        };
        first.account.auth_mode = CloudflareAuthMode::ApiToken;
        first.account.api_token = "token".into();
        first.account.token_account_id = "account".into();
        let mut second = first.clone();
        second.dns_hostnames = vec!["Admin.Example.com.".into()];

        append_pending_remote_cleanup(&mut tunnel, first.clone());
        append_pending_remote_cleanup(&mut tunnel, second);

        assert_eq!(tunnel.pending_remote_cleanup.len(), 1);
        assert_eq!(
            tunnel.pending_remote_cleanup[0].dns_hostnames,
            vec![
                "admin.example.com".to_string(),
                "api.example.com".to_string()
            ]
        );

        let mut remote = first;
        remote.kind = CloudflareCleanupKind::RemoteTunnel;
        remote.dns_hostnames = vec!["api.example.com".into()];
        append_pending_remote_cleanup(&mut tunnel, remote);

        assert_eq!(tunnel.pending_remote_cleanup.len(), 1);
        assert_eq!(
            tunnel.pending_remote_cleanup[0].kind,
            CloudflareCleanupKind::RemoteTunnel
        );
    }
}
