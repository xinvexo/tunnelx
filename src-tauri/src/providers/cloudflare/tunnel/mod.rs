pub(crate) mod api;
pub(crate) mod config;
pub(crate) mod files;
pub(crate) mod hostname;
pub(crate) mod remote;
pub(crate) mod runtime;

use self::config::{ensure_ids, write_tunnel_config};
use self::files::{push_unique_nonempty, remove_managed_file, remove_stale_files};
use self::hostname::{normalize_hostname, validate_ingress_zones};
use self::remote::{
    append_pending_remote_cleanup, cleanup_hostnames, clear_local_remote_binding,
    flush_pending_remote_cleanup, has_valid_cloudflare_tunnel_id, has_valid_remote_tunnel_id,
    lookup_remote_tunnel_id, pending_remote_cleanup_target, preserve_pending_remote_cleanup,
    release_remote_resources,
};
use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::account::credentials;
use crate::providers::cloudflare::data::persistence::save_change;
use crate::providers::cloudflare::domain::{
    CloudflareAuthMode, CloudflareCleanupKind, CloudflareTunnel,
};
use crate::providers::cloudflare::environment::cli::run_cloudflared_checked_with_cert;
use crate::providers::cloudflare::paths;
use crate::providers::contract::CLOUDFLARE_PROVIDER_ID;
use crate::services::provider_log;
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Runtime};
use uuid::Uuid;

const REMOTE_CREATE_ATTEMPTS: usize = 3;
const REMOTE_CREATE_RETRY_DELAY: Duration = Duration::from_millis(1200);

pub fn create_tunnel(
    app: &AppHandle,
    state: &AppState,
    name: String,
) -> AppResult<CloudflareTunnel> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::Msg("Tunnel name is required".into()));
    }
    let tunnel = CloudflareTunnel::new(trimmed);
    ensure_unique_local_tunnel_name(state, &tunnel.id, &tunnel.name)?;
    save_change(app, state, || state.config.upsert_cloudflare_tunnel(tunnel))
}

pub(crate) fn ensure_named_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
) -> AppResult<bool> {
    tunnel.name = tunnel.name.trim().to_string();
    if tunnel.name.is_empty() {
        return Err(AppError::Msg("Tunnel name is required".into()));
    }
    let (credentials_path, credentials_file) = managed_credentials_file(app, tunnel)?;
    if !tunnel.tunnel_id.trim().is_empty() && !has_valid_remote_tunnel_id(tunnel) {
        let old_credentials = tunnel.credentials_file.trim().to_string();
        clear_local_remote_binding(tunnel);
        if !old_credentials.is_empty() {
            let _ = fs::remove_file(old_credentials);
        }
        persist_tunnel(app, state, tunnel)?;
    }
    if has_valid_remote_tunnel_id(tunnel) {
        if credentials_ready(tunnel) {
            return Ok(false);
        }
        return restore_missing_credentials(
            app,
            state,
            tunnel,
            &credentials_path,
            &credentials_file,
        );
    }

    if tunnel.account.auth_mode == CloudflareAuthMode::ApiToken {
        return create_api_token_tunnel(app, state, tunnel, &credentials_path, &credentials_file);
    }

    create_authorized_tunnel(app, state, tunnel, &credentials_path, &credentials_file)
}

fn managed_credentials_file(
    app: &AppHandle,
    tunnel: &CloudflareTunnel,
) -> AppResult<(PathBuf, String)> {
    let credentials_dir = paths::connection_credentials_dir(app, &tunnel.id)?;
    fs::create_dir_all(&credentials_dir)?;
    let credentials_path = credentials_dir.join("credentials.json");
    let credentials_file = credentials_path.to_string_lossy().to_string();
    Ok((credentials_path, credentials_file))
}

fn credentials_ready(tunnel: &CloudflareTunnel) -> bool {
    let credentials_file = tunnel.credentials_file.trim();
    !credentials_file.is_empty() && Path::new(credentials_file).exists()
}

fn create_authorized_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
    credentials_path: &Path,
    credentials_file: &str,
) -> AppResult<bool> {
    let _ = fs::remove_file(credentials_path);
    let mut last_error: Option<AppError> = None;
    for attempt in 0..REMOTE_CREATE_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(REMOTE_CREATE_RETRY_DELAY);
        }
        let secret = tunnel_secret();
        match tauri::async_runtime::block_on(api::create_remote_tunnel_for_tunnel(tunnel, &secret))
        {
            Ok(tunnel_id) => {
                write_api_token_credentials(
                    tunnel,
                    credentials_path,
                    credentials_file,
                    &tunnel_id,
                    secret,
                )?;
                bind_tunnel_credentials(
                    app,
                    state,
                    tunnel,
                    tunnel_id,
                    credentials_file.to_string(),
                )?;
                return Ok(true);
            }
            Err(error) if tunnel_name_exists_error(&error) => {
                return fetch_authorized_tunnel_credentials(
                    app,
                    state,
                    tunnel,
                    credentials_path,
                    credentials_file,
                );
            }
            Err(error) if cloudflare_transient_error(&error) => {
                last_error = Some(error);
                if let Ok(recovered) = fetch_authorized_tunnel_credentials(
                    app,
                    state,
                    tunnel,
                    credentials_path,
                    credentials_file,
                ) {
                    return Ok(recovered);
                }
            }
            Err(error) => return Err(error),
        }
    }

    if let Ok(recovered) =
        fetch_authorized_tunnel_credentials(app, state, tunnel, credentials_path, credentials_file)
    {
        return Ok(recovered);
    }

    Err(last_error
        .unwrap_or_else(|| AppError::Msg("Cloudflare named tunnel creation failed".into())))
}

fn create_api_token_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
    credentials_path: &Path,
    credentials_file: &str,
) -> AppResult<bool> {
    let _ = fs::remove_file(credentials_path);
    let mut last_error: Option<AppError> = None;
    for attempt in 0..REMOTE_CREATE_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(REMOTE_CREATE_RETRY_DELAY);
        }
        let secret = tunnel_secret();
        let tunnel_id = match tauri::async_runtime::block_on(api::create_remote_tunnel_for_tunnel(
            tunnel, &secret,
        )) {
            Ok(tunnel_id) => tunnel_id,
            Err(error) if tunnel_name_exists_error(&error) => {
                replace_existing_api_token_tunnel(app, state, tunnel)?;
                last_error = Some(error);
                continue;
            }
            Err(error) if cloudflare_transient_error(&error) => {
                let _ = replace_existing_api_token_tunnel(app, state, tunnel);
                last_error = Some(error);
                continue;
            }
            Err(error) => return Err(error),
        };
        write_api_token_credentials(
            tunnel,
            credentials_path,
            credentials_file,
            &tunnel_id,
            secret,
        )?;
        bind_tunnel_credentials(app, state, tunnel, tunnel_id, credentials_file.to_string())?;
        return Ok(true);
    }

    Err(last_error
        .unwrap_or_else(|| AppError::Msg("Cloudflare named tunnel creation failed".into())))
}

fn write_api_token_credentials(
    tunnel: &CloudflareTunnel,
    credentials_path: &Path,
    credentials_file: &str,
    tunnel_id: &str,
    secret: [u8; 32],
) -> AppResult<()> {
    let account_id = match tunnel.account.auth_mode {
        CloudflareAuthMode::Authorization => {
            credentials::read_origin_cert(&tunnel.cert_file)?.account_id
        }
        CloudflareAuthMode::ApiToken => tunnel.account.token_account_id.trim().to_string(),
    };
    let credentials = serde_json::json!({
        "AccountTag": account_id,
        "TunnelSecret": STANDARD.encode(secret),
        "TunnelID": tunnel_id,
        "TunnelName": tunnel.name,
    });
    let bytes = serde_json::to_vec_pretty(&credentials)?;
    crate::paths::write_secret_file(credentials_path, &bytes)?;
    crate::paths::harden_permissions(credentials_path);
    if !Path::new(credentials_file).exists() {
        return Err(AppError::Msg("Cloudflare credentials write failed".into()));
    }
    Ok(())
}

fn restore_missing_credentials(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
    credentials_path: &Path,
    credentials_file: &str,
) -> AppResult<bool> {
    if tunnel.account.auth_mode == CloudflareAuthMode::Authorization {
        return fetch_authorized_tunnel_credentials(
            app,
            state,
            tunnel,
            credentials_path,
            credentials_file,
        );
    }

    replace_existing_api_token_tunnel(app, state, tunnel)?;
    create_api_token_tunnel(app, state, tunnel, credentials_path, credentials_file)
}

fn fetch_authorized_tunnel_credentials(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
    credentials_path: &Path,
    credentials_file: &str,
) -> AppResult<bool> {
    let target = if has_valid_remote_tunnel_id(tunnel) {
        Some(tunnel.tunnel_id.trim().to_string())
    } else {
        lookup_remote_tunnel_id(tunnel)?
    };
    let Some(target) = target else {
        return Err(AppError::Msg(
            "No recoverable Cloudflare named tunnel was found".into(),
        ));
    };

    let _ = fs::remove_file(credentials_path);
    run_cloudflared_checked_with_cert(
        app,
        &[
            "tunnel",
            "token",
            "--credentials-file",
            credentials_file,
            &target,
        ],
        &tunnel.cert_file,
    )?;
    if !credentials_path.exists() {
        return Err(AppError::Msg(
            "cloudflared did not write runtime credentials; reauthorize before starting".into(),
        ));
    }
    crate::paths::harden_permissions(credentials_path);
    let tunnel_id = credentials::credentials_tunnel_id(credentials_path)
        .filter(|id| has_valid_cloudflare_tunnel_id(id))
        .or_else(|| Some(target.clone()))
        .ok_or_else(|| AppError::Msg("cloudflared did not return a Tunnel ID".into()))?;
    bind_tunnel_credentials(app, state, tunnel, tunnel_id, credentials_file.to_string())?;
    Ok(false)
}

fn persist_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: &CloudflareTunnel,
) -> AppResult<CloudflareTunnel> {
    let snapshot = tunnel.clone();
    save_change(app, state, || {
        state.config.upsert_cloudflare_tunnel(snapshot)
    })
}

fn cloudflare_transient_error(error: &AppError) -> bool {
    let text = error.to_string().to_ascii_lowercase();
    text.contains("timeout")
        || text.contains("timed out")
        || text.contains("request canceled")
        || text.contains("connection")
        || text.contains("temporarily unavailable")
        || text.contains("too many requests")
        || text.contains("429")
        || text.contains("500")
        || text.contains("502")
        || text.contains("503")
        || text.contains("504")
}

fn replace_existing_api_token_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
) -> AppResult<()> {
    let remote_id = if has_valid_remote_tunnel_id(tunnel) {
        Some(tunnel.tunnel_id.trim().to_string())
    } else {
        lookup_remote_tunnel_id(tunnel)?
    };
    if let Some(remote_id) = remote_id.filter(|id| !id.trim().is_empty()) {
        tauri::async_runtime::block_on(api::delete_remote_tunnel_for_tunnel(tunnel, &remote_id))?;
    }
    clear_local_remote_binding(tunnel);
    persist_tunnel(app, state, tunnel)?;
    Ok(())
}

fn bind_tunnel_credentials(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
    tunnel_id: String,
    credentials_file: String,
) -> AppResult<()> {
    if !has_valid_cloudflare_tunnel_id(&tunnel_id) || tunnel_id == tunnel.id {
        return Err(AppError::Msg(
            "Cloudflare Tunnel ID is invalid; recreate the tunnel".into(),
        ));
    }
    tunnel.tunnel_id = tunnel_id.clone();
    tunnel.credentials_file = credentials_file;
    persist_tunnel(app, state, tunnel)?;
    Ok(())
}

fn tunnel_secret() -> [u8; 32] {
    let mut secret = [0_u8; 32];
    secret[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    secret[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    secret
}

fn tunnel_name_exists_error(error: &AppError) -> bool {
    tunnel_name_exists_text(&error.to_string())
}

fn tunnel_name_exists_text(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("tunnel with name already exists")
        || text.contains("name already exists")
        || (text.contains("tunnel") && text.contains("name") && text.contains("already exists"))
}

pub fn save_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: CloudflareTunnel,
) -> AppResult<CloudflareTunnel> {
    save_tunnel_inner(app, state, tunnel, None)
}

pub(crate) fn save_tunnel_releasing_previous_as(
    app: &AppHandle,
    state: &AppState,
    tunnel: CloudflareTunnel,
    previous_for_release: CloudflareTunnel,
) -> AppResult<CloudflareTunnel> {
    save_tunnel_inner(app, state, tunnel, Some(previous_for_release))
}

fn save_tunnel_inner(
    app: &AppHandle,
    state: &AppState,
    mut tunnel: CloudflareTunnel,
    previous_for_release: Option<CloudflareTunnel>,
) -> AppResult<CloudflareTunnel> {
    crate::store::ensure_app_data_writable(state)?;
    let previous = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == tunnel.id);
    let previous_config_file = previous
        .as_ref()
        .map(|item| item.config_file.clone())
        .unwrap_or_default();
    tunnel.name = tunnel.name.trim().to_string();
    if tunnel.name.is_empty() {
        return Err(AppError::Msg("Tunnel name is required".into()));
    }
    ensure_unique_local_tunnel_name(state, &tunnel.id, &tunnel.name)?;
    if let Some(previous) = previous.as_ref() {
        crate::services::redaction::restore_if_redacted(
            &mut tunnel.account.api_token,
            &previous.account.api_token,
        );
    }
    ensure_api_token_verified_before_save(app, state, previous.as_ref(), &tunnel)?;
    if let Some(previous) = previous.as_ref() {
        preserve_pending_remote_cleanup(previous, &mut tunnel);
    }
    let mut stale_files = normalize_auth_material(&mut tunnel, previous.as_ref());
    let remote_identity_changed = previous
        .as_ref()
        .map(|previous| cloudflare_remote_identity_changed(previous, &tunnel))
        .unwrap_or(false);
    tunnel.ingress = ensure_ids(tunnel.ingress);
    clear_incomplete_dns_flags(&mut tunnel);
    validate_ingress_zones(&tunnel)?;
    if remote_identity_changed {
        ensure_tunnel_stopped_for_identity_change(state, &tunnel.id)?;
        let release_source = previous_for_release.as_ref().or(previous.as_ref());
        queue_previous_identity_cleanup(release_source, &mut tunnel, &mut stale_files);
        clear_local_remote_binding(&mut tunnel);
        tunnel.config_file.clear();
    } else if let Some(previous) = previous.as_ref() {
        preserve_existing_dns_flags(previous, &mut tunnel);
        ensure_stopped_for_runtime_config_change(state, previous, &tunnel)?;
        queue_removed_dns_cleanup(previous, &mut tunnel);
    }
    tunnel.config_file = write_tunnel_config(app, &tunnel)?;
    if !previous_config_file.trim().is_empty() && previous_config_file != tunnel.config_file {
        push_unique_nonempty(&mut stale_files, &previous_config_file);
    }
    let saved = save_change(app, state, || state.config.upsert_cloudflare_tunnel(tunnel))?;
    if let Err(error) = flush_pending_remote_cleanup(app, state, &saved.id) {
        emit_tunnel_log(
            app,
            state,
            &saved.id,
            format!("pending Cloudflare cleanup is not complete yet: {error}"),
        );
    }
    let managed_root = paths::root(app)?;
    remove_stale_files(stale_files, &saved, &managed_root);
    Ok(current_tunnel(state, &saved.id).unwrap_or(saved))
}

fn cloudflare_remote_identity_changed(
    previous: &CloudflareTunnel,
    next: &CloudflareTunnel,
) -> bool {
    if previous.account.auth_mode != next.account.auth_mode {
        return true;
    }
    match next.account.auth_mode {
        CloudflareAuthMode::Authorization => {
            normalized_text(&previous.account.authorization_account_id)
                != normalized_text(&next.account.authorization_account_id)
                || normalized_text(&previous.account.authorization_zone_id)
                    != normalized_text(&next.account.authorization_zone_id)
        }
        CloudflareAuthMode::ApiToken => {
            normalized_text(&previous.account.api_token) != normalized_text(&next.account.api_token)
                || normalized_text(&previous.account.token_account_id)
                    != normalized_text(&next.account.token_account_id)
        }
    }
}

fn ensure_api_token_verified_before_save(
    app: &AppHandle,
    state: &AppState,
    previous: Option<&CloudflareTunnel>,
    next: &CloudflareTunnel,
) -> AppResult<()> {
    if !api_token_requires_verification(previous, next) {
        return Ok(());
    }
    provider_log::credential_verification(
        app,
        state,
        CLOUDFLARE_PROVIDER_ID,
        &next.id,
        "Cloudflare",
        "API token",
        || {
            tauri::async_runtime::block_on(api::verify_api_token_account(&next.account))?;
            Ok(())
        },
    )
}

fn api_token_requires_verification(
    previous: Option<&CloudflareTunnel>,
    next: &CloudflareTunnel,
) -> bool {
    if next.account.auth_mode != CloudflareAuthMode::ApiToken {
        return false;
    }
    if next.account.api_token.trim().is_empty() || next.account.token_account_id.trim().is_empty() {
        return true;
    }
    let Some(previous) = previous else {
        return true;
    };
    previous.account.auth_mode != CloudflareAuthMode::ApiToken
        || normalized_text(&previous.account.api_token) != normalized_text(&next.account.api_token)
        || normalized_text(&previous.account.token_account_id)
            != normalized_text(&next.account.token_account_id)
}

fn normalized_text(value: &str) -> String {
    value.trim().to_string()
}

fn queue_removed_dns_cleanup(previous: &CloudflareTunnel, next: &mut CloudflareTunnel) {
    let hostnames = removed_dns_route_hostnames(previous, next);
    if hostnames.is_empty() {
        return;
    }
    if let Some(target) =
        pending_remote_cleanup_target(previous, CloudflareCleanupKind::DnsRoutes, hostnames)
    {
        append_pending_remote_cleanup(next, target);
    }
}

fn queue_previous_identity_cleanup(
    previous: Option<&CloudflareTunnel>,
    next: &mut CloudflareTunnel,
    stale_files: &mut Vec<String>,
) {
    let Some(previous) = previous else {
        return;
    };
    push_unique_nonempty(stale_files, &previous.credentials_file);
    push_unique_nonempty(stale_files, &previous.config_file);
    if let Some(target) = pending_remote_cleanup_target(
        previous,
        CloudflareCleanupKind::RemoteTunnel,
        cleanup_hostnames(previous, false),
    ) {
        append_pending_remote_cleanup(next, target);
    }
}

fn current_tunnel(state: &AppState, id: &str) -> Option<CloudflareTunnel> {
    state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
}

fn emit_tunnel_log<R: Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: String,
) {
    provider_log::emit_system(app, state, CLOUDFLARE_PROVIDER_ID, tunnel_id, line);
}

fn removed_dns_route_hostnames(
    previous: &CloudflareTunnel,
    next: &CloudflareTunnel,
) -> Vec<String> {
    let next_hosts = next
        .ingress
        .iter()
        .filter(|rule| rule.enabled && !rule.service.trim().is_empty())
        .map(|rule| normalize_hostname(&rule.hostname))
        .filter(|hostname| !hostname.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    previous
        .ingress
        .iter()
        .filter(|rule| rule.dns_routed)
        .map(|rule| normalize_hostname(&rule.hostname))
        .filter(|hostname| !hostname.is_empty() && !next_hosts.contains(hostname))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn clear_incomplete_dns_flags(tunnel: &mut CloudflareTunnel) {
    for rule in &mut tunnel.ingress {
        if rule.hostname.trim().is_empty() || rule.service.trim().is_empty() {
            rule.dns_routed = false;
        }
        if !rule.enabled {
            rule.dns_routed = false;
        }
    }
}

fn preserve_existing_dns_flags(previous: &CloudflareTunnel, next: &mut CloudflareTunnel) {
    let routed_hosts = previous
        .ingress
        .iter()
        .filter(|rule| rule.dns_routed)
        .map(|rule| normalize_hostname(&rule.hostname))
        .filter(|hostname| !hostname.is_empty())
        .collect::<std::collections::BTreeSet<_>>();
    if routed_hosts.is_empty() {
        return;
    }
    for rule in &mut next.ingress {
        let hostname = normalize_hostname(&rule.hostname);
        if !hostname.is_empty()
            && rule.enabled
            && !rule.service.trim().is_empty()
            && routed_hosts.contains(&hostname)
        {
            rule.dns_routed = true;
        }
    }
}

fn ensure_unique_local_tunnel_name(state: &AppState, id: &str, name: &str) -> AppResult<()> {
    let tunnels = state.config.cloudflare().tunnels;
    if has_duplicate_local_tunnel_name(&tunnels, id, name) {
        return Err(AppError::Msg(
            "Cloudflare connection name already exists; choose another name".into(),
        ));
    }
    Ok(())
}

fn has_duplicate_local_tunnel_name(tunnels: &[CloudflareTunnel], id: &str, name: &str) -> bool {
    let name = name.trim();
    tunnels
        .iter()
        .any(|item| item.id != id && item.name.trim().eq_ignore_ascii_case(name))
}

fn ensure_tunnel_stopped_for_identity_change(state: &AppState, tunnel_id: &str) -> AppResult<()> {
    let status = state
        .provider_runtime
        .info(CLOUDFLARE_PROVIDER_ID, tunnel_id)
        .status;
    if status.is_active() {
        return Err(AppError::Msg(
            "Stop this Cloudflare connection before changing auth mode or credentials".into(),
        ));
    }
    Ok(())
}

fn ensure_stopped_for_runtime_config_change(
    state: &AppState,
    previous: &CloudflareTunnel,
    next: &CloudflareTunnel,
) -> AppResult<()> {
    let status = state
        .provider_runtime
        .info(CLOUDFLARE_PROVIDER_ID, &next.id)
        .status;
    if status.is_active() && runtime_config_changed(previous, next) {
        return Err(AppError::Msg(
            "Stop this Cloudflare connection before changing ingress or runtime config".into(),
        ));
    }
    Ok(())
}

fn runtime_config_changed(previous: &CloudflareTunnel, next: &CloudflareTunnel) -> bool {
    runtime_tunnel_ref(previous) != runtime_tunnel_ref(next)
        || normalized_text(&previous.credentials_file) != normalized_text(&next.credentials_file)
        || runtime_ingress(previous) != runtime_ingress(next)
}

fn runtime_tunnel_ref(tunnel: &CloudflareTunnel) -> String {
    let tunnel_id = tunnel.tunnel_id.trim();
    if tunnel_id.is_empty() {
        normalized_text(&tunnel.name)
    } else {
        normalized_text(tunnel_id)
    }
}

fn runtime_ingress(tunnel: &CloudflareTunnel) -> Vec<(String, String)> {
    tunnel
        .ingress
        .iter()
        .filter(|rule| {
            rule.enabled && !rule.hostname.trim().is_empty() && !rule.service.trim().is_empty()
        })
        .map(|rule| {
            (
                normalize_hostname(&rule.hostname),
                normalized_text(&rule.service),
            )
        })
        .collect()
}

fn normalize_auth_material(
    tunnel: &mut CloudflareTunnel,
    previous: Option<&CloudflareTunnel>,
) -> Vec<String> {
    let mut stale_files = Vec::new();
    match tunnel.account.auth_mode {
        CloudflareAuthMode::Authorization => {
            tunnel.account.api_token.clear();
            tunnel.account.token_account_id.clear();
            tunnel.account.token_account_name.clear();
        }
        CloudflareAuthMode::ApiToken => {
            push_unique_nonempty(&mut stale_files, &tunnel.cert_file);
            if let Some(previous) = previous {
                push_unique_nonempty(&mut stale_files, &previous.cert_file);
            }
            tunnel.cert_file.clear();
            tunnel.account.authorization_account_id.clear();
            tunnel.account.authorization_zone_id.clear();
        }
    }
    stale_files
}

pub fn delete_tunnel(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    remote: bool,
    delete_credentials: bool,
) -> AppResult<CloudflareTunnel> {
    crate::store::ensure_app_data_writable(state)?;
    flush_pending_remote_cleanup(app, state, id)?;
    let tunnel_before_stop = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Msg("Cloudflare Tunnel not found".into()))?;
    let old_config_file = tunnel_before_stop.config_file.clone();
    let old_cert_file = tunnel_before_stop.cert_file.clone();
    let old_credentials_file = tunnel_before_stop.credentials_file.clone();

    let _ = runtime::stop_tunnel(app, state, id)?;
    let mut tunnel = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .unwrap_or(tunnel_before_stop);
    if remote {
        let _ = release_remote_resources(app, &mut tunnel)?;
    }
    let managed_root = paths::root(app)?;
    remove_managed_file(&managed_root, &old_config_file);
    remove_managed_file(&managed_root, &tunnel.config_file);
    remove_managed_file(&managed_root, &old_cert_file);
    remove_managed_file(&managed_root, &tunnel.cert_file);
    if let Ok(dir) = paths::connection_credentials_dir(app, id) {
        let _ = fs::remove_dir_all(dir);
    }
    if delete_credentials {
        remove_managed_file(&managed_root, &old_credentials_file);
        remove_managed_file(&managed_root, &tunnel.credentials_file);
    }
    save_change(app, state, || state.config.delete_cloudflare_tunnel(id))
}

pub(crate) fn pending_dns_hostnames(tunnel: &CloudflareTunnel) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    tunnel
        .ingress
        .iter()
        .filter_map(|rule| {
            let host = normalize_hostname(&rule.hostname);
            if !rule.enabled || host.is_empty() || rule.dns_routed || !seen.insert(host.clone()) {
                None
            } else {
                Some(host)
            }
        })
        .collect()
}

pub(crate) fn route_pending_dns(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
) -> AppResult<Vec<String>> {
    let target = tunnel.tunnel_id.trim().to_string();
    if target.is_empty() {
        return Err(AppError::Msg(
            "Create or sync the Cloudflare named tunnel before starting".into(),
        ));
    }
    if !has_valid_cloudflare_tunnel_id(&target) {
        return Err(AppError::Msg(
            "Cloudflare Tunnel ID is invalid; recreate the tunnel".into(),
        ));
    }

    let pending = pending_dns_hostnames(tunnel);

    let mut routed = Vec::new();
    for host in pending {
        emit_tunnel_log(
            app,
            state,
            &tunnel.id,
            format!("creating Cloudflare DNS route: {host}"),
        );
        if tunnel.account.auth_mode == CloudflareAuthMode::ApiToken {
            tauri::async_runtime::block_on(api::route_dns_for_tunnel(tunnel, &target, &host))?;
        } else {
            run_cloudflared_checked_with_cert(
                app,
                &["tunnel", "route", "dns", &target, &host],
                &tunnel.cert_file,
            )?;
        }
        mark_dns_routed(tunnel, &host);
        emit_tunnel_log(
            app,
            state,
            &tunnel.id,
            format!("Cloudflare DNS route ready: {host}"),
        );
        routed.push(host);
    }
    Ok(routed)
}

fn mark_dns_routed(tunnel: &mut CloudflareTunnel, routed_host: &str) {
    let routed_host = normalize_hostname(routed_host);
    for rule in &mut tunnel.ingress {
        if rule.enabled && normalize_hostname(&rule.hostname) == routed_host {
            rule.dns_routed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::providers::cloudflare::domain::CloudflareIngressRule;

    #[test]
    fn normalize_auth_material_keeps_auth_modes_exclusive() {
        let mut previous = CloudflareTunnel::new("api");
        previous.cert_file = "/tmp/tunnelx/cloudflare/certs/old.pem".into();

        let mut token_mode = previous.clone();
        token_mode.account.auth_mode = CloudflareAuthMode::ApiToken;
        token_mode.account.api_token = "token".into();
        token_mode.account.token_account_id = "account".into();
        token_mode.account.authorization_account_id = "auth-account".into();
        token_mode.account.authorization_zone_id = "zone".into();

        let stale = normalize_auth_material(&mut token_mode, Some(&previous));

        assert_eq!(
            stale,
            vec!["/tmp/tunnelx/cloudflare/certs/old.pem".to_string()]
        );
        assert!(token_mode.cert_file.is_empty());
        assert!(token_mode.account.authorization_account_id.is_empty());
        assert!(token_mode.account.authorization_zone_id.is_empty());
        assert_eq!(token_mode.account.api_token, "token");
        assert_eq!(token_mode.account.token_account_id, "account");

        let mut authorization_mode = CloudflareTunnel::new("api");
        authorization_mode.account.auth_mode = CloudflareAuthMode::Authorization;
        authorization_mode.account.api_token = "token".into();
        authorization_mode.account.token_account_id = "account".into();
        authorization_mode.account.token_account_name = "name".into();

        let stale = normalize_auth_material(&mut authorization_mode, None);

        assert!(stale.is_empty());
        assert!(authorization_mode.account.api_token.is_empty());
        assert!(authorization_mode.account.token_account_id.is_empty());
        assert!(authorization_mode.account.token_account_name.is_empty());
    }

    #[test]
    fn remote_identity_change_ignores_display_only_fields() {
        let mut previous = CloudflareTunnel::new("api");
        previous.account.auth_mode = CloudflareAuthMode::ApiToken;
        previous.account.api_token = "token".into();
        previous.account.token_account_id = "account".into();
        previous.account.token_account_name = "Old name".into();

        let mut renamed = previous.clone();
        renamed.account.token_account_name = "New name".into();
        assert!(!cloudflare_remote_identity_changed(&previous, &renamed));

        let mut changed_token = previous.clone();
        changed_token.account.api_token = "new-token".into();
        assert!(cloudflare_remote_identity_changed(
            &previous,
            &changed_token
        ));

        let mut authorization = previous.clone();
        authorization.account.auth_mode = CloudflareAuthMode::Authorization;
        assert!(cloudflare_remote_identity_changed(
            &previous,
            &authorization
        ));

        let mut authorized = CloudflareTunnel::new("api");
        authorized.account.auth_mode = CloudflareAuthMode::Authorization;
        authorized.account.authorization_account_id = "account".into();
        authorized.account.authorization_zone_id = "zone".into();
        authorized.cert_file = "/tmp/a.pem".into();
        let mut moved_cert = authorized.clone();
        moved_cert.cert_file = "/tmp/b.pem".into();
        assert!(!cloudflare_remote_identity_changed(
            &authorized,
            &moved_cert
        ));
    }

    #[test]
    fn mark_dns_routed_matches_normalized_hostnames() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.ingress = vec![
            CloudflareIngressRule {
                hostname: "API.Example.COM.".into(),
                service: "http://localhost:8080".into(),
                ..Default::default()
            },
            CloudflareIngressRule {
                hostname: "other.example.com".into(),
                service: "http://localhost:8081".into(),
                ..Default::default()
            },
        ];

        mark_dns_routed(&mut tunnel, "api.example.com");

        assert!(tunnel.ingress[0].dns_routed);
        assert!(!tunnel.ingress[1].dns_routed);
    }

    #[test]
    fn local_tunnel_name_duplicate_ignores_current_tunnel() {
        let mut existing = CloudflareTunnel::new("Api");
        existing.id = "existing".into();
        let mut current = CloudflareTunnel::new("api");
        current.id = "current".into();
        let tunnels = vec![existing.clone(), current.clone()];

        assert!(has_duplicate_local_tunnel_name(&tunnels, "current", "API"));
        assert!(!has_duplicate_local_tunnel_name(
            &[current],
            "current",
            "API"
        ));
    }

    #[test]
    fn removed_dns_route_hostnames_tracks_deleted_or_incomplete_ingress() {
        let mut previous = CloudflareTunnel::new("api");
        previous.ingress = vec![
            CloudflareIngressRule {
                hostname: "Api.Example.com.".into(),
                service: "http://localhost:8080".into(),
                dns_routed: true,
                ..Default::default()
            },
            CloudflareIngressRule {
                hostname: "keep.example.com".into(),
                service: "http://localhost:8081".into(),
                dns_routed: true,
                ..Default::default()
            },
            CloudflareIngressRule {
                hostname: "pending.example.com".into(),
                service: "http://localhost:8082".into(),
                dns_routed: false,
                ..Default::default()
            },
        ];
        let mut next = previous.clone();
        next.ingress = vec![
            CloudflareIngressRule {
                hostname: "keep.example.com".into(),
                service: "http://localhost:9000".into(),
                dns_routed: true,
                ..Default::default()
            },
            CloudflareIngressRule {
                hostname: "Api.Example.com.".into(),
                service: String::new(),
                dns_routed: true,
                ..Default::default()
            },
        ];

        assert_eq!(
            removed_dns_route_hostnames(&previous, &next),
            vec!["api.example.com".to_string()]
        );
    }

    #[test]
    fn clear_incomplete_dns_flags_drops_stale_local_state() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.ingress = vec![
            CloudflareIngressRule {
                hostname: "api.example.com".into(),
                service: String::new(),
                dns_routed: true,
                ..Default::default()
            },
            CloudflareIngressRule {
                hostname: "ok.example.com".into(),
                service: "http://localhost:8080".into(),
                dns_routed: true,
                ..Default::default()
            },
        ];

        clear_incomplete_dns_flags(&mut tunnel);

        assert!(!tunnel.ingress[0].dns_routed);
        assert!(tunnel.ingress[1].dns_routed);
    }

    #[test]
    fn preserve_existing_dns_flags_matches_normalized_hostname() {
        let mut previous = CloudflareTunnel::new("api");
        previous.ingress = vec![CloudflareIngressRule {
            hostname: "Api.Example.com.".into(),
            service: "http://localhost:8080".into(),
            dns_routed: true,
            ..Default::default()
        }];
        let mut next = CloudflareTunnel::new("api");
        next.ingress = vec![
            CloudflareIngressRule {
                hostname: "api.example.com".into(),
                service: "http://localhost:9000".into(),
                dns_routed: false,
                ..Default::default()
            },
            CloudflareIngressRule {
                hostname: "api.example.com".into(),
                service: String::new(),
                dns_routed: false,
                ..Default::default()
            },
        ];

        preserve_existing_dns_flags(&previous, &mut next);

        assert!(next.ingress[0].dns_routed);
        assert!(!next.ingress[1].dns_routed);
    }

    #[test]
    fn runtime_config_change_detects_effective_ingress_changes_only() {
        let mut previous = CloudflareTunnel::new("api");
        previous.tunnel_id = "11111111-1111-4111-8111-111111111111".into();
        previous.credentials_file = "/tmp/credentials.json".into();
        previous.ingress = vec![CloudflareIngressRule {
            hostname: "Api.Example.com.".into(),
            service: "http://localhost:8080".into(),
            dns_routed: true,
            ..Default::default()
        }];

        let mut display_only = previous.clone();
        display_only.name = "api-renamed-locally".into();
        display_only.ingress[0].hostname = "api.example.com".into();
        display_only.ingress[0].dns_routed = false;
        assert!(!runtime_config_changed(&previous, &display_only));

        let mut service_changed = previous.clone();
        service_changed.ingress[0].service = "http://localhost:9000".into();
        assert!(runtime_config_changed(&previous, &service_changed));

        let mut credentials_changed = previous.clone();
        credentials_changed.credentials_file = "/tmp/new-credentials.json".into();
        assert!(runtime_config_changed(&previous, &credentials_changed));
    }
}
