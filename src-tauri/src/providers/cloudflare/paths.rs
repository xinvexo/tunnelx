//! Cloudflare Tunnel provider 的本地路径约定。
//!
//! `<app_data>/cloudflare/credentials/<connection-id>/*.json` —— Cloudflare Tunnel credentials
//! `<app_data>/cloudflare/certs/<connection-id>/cert.pem` —— Cloudflare authorization cert
//! `<app_data>/cloudflare/auth/<connection-id>/<attempt-id>` —— one-shot authorization webview profile
//! `<app_data>/cloudflare/configs/*.yml`      —— cloudflared 运行配置
//! `<app_data>/cloudflare/bin/cloudflared`     —— TunnelX 托管的 cloudflared
use crate::error::AppResult;
use std::path::PathBuf;
use tauri::{AppHandle, Runtime};

pub(crate) fn sanitize_filename(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = cleaned.trim_matches(['.', '-']).to_string();
    if trimmed.is_empty() {
        "cloudflare".into()
    } else {
        trimmed
    }
}

pub fn root<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(crate::paths::data_dir(app)?.join("cloudflare"))
}

pub fn credentials_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("credentials"))
}

pub fn connection_credentials_dir<R: Runtime>(
    app: &AppHandle<R>,
    connection_id: &str,
) -> AppResult<PathBuf> {
    Ok(credentials_dir(app)?.join(sanitize_filename(connection_id)))
}

pub fn certs_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("certs"))
}

pub fn cert_file<R: Runtime>(app: &AppHandle<R>, connection_id: &str) -> AppResult<PathBuf> {
    Ok(certs_dir(app)?
        .join(sanitize_filename(connection_id))
        .join("cert.pem"))
}

pub fn auth_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("auth"))
}

pub fn auth_browser_dir<R: Runtime>(app: &AppHandle<R>, connection_id: &str) -> AppResult<PathBuf> {
    Ok(auth_dir(app)?.join(sanitize_filename(connection_id)))
}

pub fn configs_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("configs"))
}

pub fn bin_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(root(app)?.join("bin"))
}

pub fn exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "cloudflared.exe"
    } else {
        "cloudflared"
    }
}

pub fn managed_exe<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(bin_dir(app)?.join(exe_name()))
}
