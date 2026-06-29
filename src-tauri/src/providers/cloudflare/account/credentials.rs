use super::authorization::authorize_connection;
use crate::error::{AppError, AppResult};
use crate::paths;
use crate::providers::cloudflare::domain::{CloudflareAuthMode, CloudflareCommandOutput};
use crate::providers::cloudflare::paths as cloudflare_paths;
use crate::providers::cloudflare::tunnel;
use crate::providers::contract::CLOUDFLARE_PROVIDER_ID;
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

#[derive(Deserialize)]
struct CredentialFileJson {
    #[serde(
        default,
        rename = "TunnelID",
        alias = "TunnelId",
        alias = "tunnelID",
        alias = "tunnelId",
        alias = "tunnel_id"
    )]
    tunnel_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub(crate) struct OriginCertInfo {
    #[serde(default, rename = "zoneID")]
    pub zone_id: String,
    #[serde(default, rename = "accountID")]
    pub account_id: String,
    #[serde(default, rename = "apiToken")]
    pub api_token: String,
}

fn parse_pem_block_type(line: &str, prefix: &str, suffix: &str) -> Option<String> {
    line.strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .map(|value| value.trim().to_string())
}

pub(crate) fn parse_origin_cert_text(text: &str) -> AppResult<OriginCertInfo> {
    let mut current_type: Option<String> = None;
    let mut payload = String::new();

    for line in text.lines().map(str::trim) {
        if let Some(block_type) = parse_pem_block_type(line, "-----BEGIN ", "-----") {
            current_type = Some(block_type);
            payload.clear();
            continue;
        }

        if let Some(block_type) = parse_pem_block_type(line, "-----END ", "-----") {
            if current_type.as_deref() == Some("ARGO TUNNEL TOKEN")
                && block_type == "ARGO TUNNEL TOKEN"
            {
                let bytes = STANDARD.decode(payload.as_bytes()).map_err(|e| {
                    AppError::Msg(format!(
                        "Failed to parse Cloudflare authorization credentials: {e}"
                    ))
                })?;
                let cert: OriginCertInfo = serde_json::from_slice(&bytes)?;
                if cert.account_id.trim().is_empty()
                    || cert.zone_id.trim().is_empty()
                    || cert.api_token.trim().is_empty()
                {
                    return Err(AppError::Msg(
                        "Cloudflare authorization credentials are missing account or token information; authorize again".into(),
                    ));
                }
                return Ok(cert);
            }
            current_type = None;
            payload.clear();
            continue;
        }

        if current_type.as_deref() == Some("ARGO TUNNEL TOKEN") && !line.is_empty() {
            payload.push_str(line);
        }
    }

    Err(AppError::Msg(
        "No authorization payload was found in the Cloudflare credentials; authorize again".into(),
    ))
}

pub(crate) fn read_origin_cert(path: &str) -> AppResult<OriginCertInfo> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::Msg(
            "Authorize the current Cloudflare connection first".into(),
        ));
    }
    let text = fs::read_to_string(trimmed).map_err(|e| {
        AppError::Msg(format!(
            "Failed to read Cloudflare authorization credentials: {e}"
        ))
    })?;
    parse_origin_cert_text(&text)
}

fn parse_credentials(path: &Path) -> Option<CredentialFileJson> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<CredentialFileJson>(&text).ok())
}

pub(crate) fn credentials_tunnel_id(path: &Path) -> Option<String> {
    parse_credentials(path).and_then(|item| item.tunnel_id)
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect()
}

fn json_files_recursive(dir: &Path) -> Vec<PathBuf> {
    let mut files = json_files(dir);
    for child in fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
    {
        files.extend(json_files(&child));
    }
    files
}

fn save_credentials_file(app: &AppHandle, tunnel_id: &str, src: &Path) -> AppResult<String> {
    let file_name = credentials_tunnel_id(src)
        .clone()
        .filter(|id| !id.trim().is_empty())
        .map(|id| format!("{id}.json"))
        .or_else(|| {
            src.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "credentials.json".into());
    let dest_dir = cloudflare_paths::connection_credentials_dir(app, tunnel_id)?;
    fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(cloudflare_paths::sanitize_filename(&file_name));
    let bytes = fs::read(src)?;
    paths::write_secret_file(&dest, &bytes)?;
    Ok(dest.to_string_lossy().to_string())
}

fn find_credentials_source(app: &AppHandle, cloudflare_tunnel_id: &str) -> Option<PathBuf> {
    if cloudflare_tunnel_id.trim().is_empty() {
        return None;
    }
    let dirs = vec![cloudflare_paths::credentials_dir(app).ok()?];
    dirs.into_iter()
        .flat_map(|dir| json_files_recursive(&dir))
        .find(|path| credentials_tunnel_id(path).as_deref() == Some(cloudflare_tunnel_id))
}

pub(crate) fn adopt_credentials_for_tunnel(
    app: &AppHandle,
    connection_id: &str,
    cloudflare_tunnel_id: &str,
) -> Option<String> {
    let source = find_credentials_source(app, cloudflare_tunnel_id)?;
    save_credentials_file(app, connection_id, &source).ok()
}

fn replace_authorization_cert(pending: &Path, dest: &Path) -> AppResult<Option<PathBuf>> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let backup = dest.with_extension("previous.pem");
    let _ = fs::remove_file(&backup);
    let backup = if dest.exists() {
        fs::rename(dest, &backup).map_err(|e| {
            AppError::Msg(format!(
                "Failed to back up Cloudflare authorization credentials: {e}"
            ))
        })?;
        Some(backup)
    } else {
        None
    };

    if let Err(error) = fs::rename(pending, dest) {
        if let Some(backup) = backup.as_ref() {
            let _ = fs::rename(backup, dest);
        }
        return Err(AppError::Msg(format!(
            "Failed to save Cloudflare authorization credentials: {error}"
        )));
    }
    paths::harden_permissions(dest);
    Ok(backup)
}

fn restore_authorization_cert(dest: &Path, backup: Option<&Path>) {
    let _ = fs::remove_file(dest);
    if let Some(backup) = backup {
        let _ = fs::rename(backup, dest);
        paths::harden_permissions(dest);
    }
}

pub async fn login_connection(
    app: &AppHandle,
    state: &AppState,
    tunnel_id: String,
) -> AppResult<CloudflareCommandOutput> {
    crate::store::ensure_app_data_writable(state)?;
    ensure_stopped_for_authorization(state, &tunnel_id)?;
    let dest = cloudflare_paths::cert_file(app, &tunnel_id)?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let pending = dest.with_extension("pending.pem");
    let _ = fs::remove_file(&pending);
    let output = authorize_connection(app, &tunnel_id, &pending).await?;
    if !output.success {
        let _ = fs::remove_file(&pending);
        return Ok(output);
    }
    if !pending.exists() {
        return Err(AppError::Msg(
            "cloudflared did not produce authorization credentials; try authorizing again".into(),
        ));
    }
    paths::harden_permissions(&pending);
    let pending_file = pending.to_string_lossy().to_string();
    let origin_cert = read_origin_cert(&pending_file)?;
    let mut tunnel = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == tunnel_id)
        .ok_or_else(|| AppError::Msg("Cloudflare connection not found".into()))?;
    let mut previous_for_release = tunnel.clone();
    let cert_file = dest.to_string_lossy().to_string();
    let cert_backup = match replace_authorization_cert(&pending, &dest) {
        Ok(backup) => backup,
        Err(error) => {
            let _ = fs::remove_file(&pending);
            return Err(error);
        }
    };
    if previous_for_release.cert_file.trim() == cert_file {
        previous_for_release.cert_file = cert_backup
            .as_ref()
            .map(|backup| backup.to_string_lossy().to_string())
            .unwrap_or_default();
    }

    tunnel.cert_file = cert_file;
    tunnel.account.auth_mode = CloudflareAuthMode::Authorization;
    tunnel.account.api_token.clear();
    tunnel.account.authorization_account_id = origin_cert.account_id.clone();
    tunnel.account.authorization_zone_id = origin_cert.zone_id.clone();
    if let Err(error) =
        tunnel::save_tunnel_releasing_previous_as(app, state, tunnel, previous_for_release)
    {
        restore_authorization_cert(&dest, cert_backup.as_deref());
        let _ = fs::remove_file(&pending);
        return Err(error);
    }
    if let Some(backup) = cert_backup {
        let _ = fs::remove_file(backup);
    }
    Ok(output)
}

fn ensure_stopped_for_authorization(state: &AppState, tunnel_id: &str) -> AppResult<()> {
    let status = state
        .provider_runtime
        .info(CLOUDFLARE_PROVIDER_ID, tunnel_id)
        .status;
    if status.is_active() {
        return Err(AppError::Msg(
            "Stop the current Cloudflare connection before authorizing again".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tunnelx-cloudflare-credentials-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn parse_origin_cert_reads_token_block() {
        let cert = OriginCertInfo {
            zone_id: "zone-1".into(),
            account_id: "account-1".into(),
            api_token: "token-1".into(),
        };
        let encoded = STANDARD.encode(serde_json::to_vec(&cert).unwrap());
        let pem = format!(
            "-----BEGIN ARGO TUNNEL TOKEN-----\n{encoded}\n-----END ARGO TUNNEL TOKEN-----"
        );

        let parsed = parse_origin_cert_text(&pem).unwrap();

        assert_eq!(parsed.zone_id, "zone-1");
        assert_eq!(parsed.account_id, "account-1");
        assert_eq!(parsed.api_token, "token-1");
    }

    #[test]
    fn credentials_tunnel_id_accepts_cloudflared_field_name() {
        let dir = temp_test_dir();
        let file = dir.join("credentials.json");
        fs::write(
            &file,
            r#"{"AccountTag":"account-1","TunnelSecret":"secret","TunnelID":"11111111-1111-4111-8111-111111111111","TunnelName":"api"}"#,
        )
        .unwrap();

        assert_eq!(
            credentials_tunnel_id(&file).as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn replace_authorization_cert_backs_up_existing_cert() {
        let dir = temp_test_dir();
        let dest = dir.join("cert.pem");
        let pending = dir.join("cert.pending.pem");
        fs::write(&dest, b"old-cert").unwrap();
        fs::write(&pending, b"new-cert").unwrap();

        let backup = replace_authorization_cert(&pending, &dest).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), b"new-cert");
        assert!(!pending.exists());
        let backup = backup.unwrap();
        assert_eq!(fs::read(&backup).unwrap(), b"old-cert");

        restore_authorization_cert(&dest, Some(&backup));

        assert_eq!(fs::read(&dest).unwrap(), b"old-cert");
        assert!(!backup.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn replace_authorization_cert_without_existing_cert_has_no_backup() {
        let dir = temp_test_dir();
        let dest = dir.join("cert.pem");
        let pending = dir.join("cert.pending.pem");
        fs::write(&pending, b"new-cert").unwrap();

        let backup = replace_authorization_cert(&pending, &dest).unwrap();

        assert!(backup.is_none());
        assert_eq!(fs::read(&dest).unwrap(), b"new-cert");
        assert!(!pending.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn authorization_requires_stopped_runtime() {
        let state = AppState::default();
        state
            .provider_runtime
            .mark_running(CLOUDFLARE_PROVIDER_ID, "conn-1", 123, "running");

        assert!(ensure_stopped_for_authorization(&state, "conn-1").is_err());
        assert!(ensure_stopped_for_authorization(&state, "conn-2").is_ok());
    }
}
