use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::domain::CloudflareCommandOutput;
use crate::providers::cloudflare::paths;
use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tokio::sync::Notify;
use tokio::time::sleep;
use uuid::Uuid;

const BASE_LOGIN_URL: &str = "https://dash.cloudflare.com/argotunnel";
const CALLBACK_STORE_URL: &str = "https://login.cloudflareaccess.org/";
const AUTH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const POLL_TIMEOUT: Duration = Duration::from_secs(12);

struct AuthorizationWindow {
    label: String,
    profile_root: PathBuf,
}

#[derive(Default)]
struct AuthorizationCancel {
    cancelled: AtomicBool,
    notify: Notify,
}

impl AuthorizationCancel {
    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::SeqCst) {
            self.notify.notify_one();
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }
        self.notify.notified().await;
    }
}

pub(crate) async fn authorize_connection(
    app: &AppHandle,
    connection_id: &str,
    cert_target: &Path,
) -> AppResult<CloudflareCommandOutput> {
    let transfer_key = transfer_key();
    let auth_url = build_authorization_url(&transfer_key)?;
    let cancel = Arc::new(AuthorizationCancel::default());
    let auth_window = open_authorization_window(app, connection_id, &auth_url, cancel.clone())?;

    let result = poll_authorization_cert(&transfer_key, &cancel, AUTH_TIMEOUT).await;
    close_authorization_window(app, &auth_window.label);
    cleanup_authorization_profile(&auth_window.profile_root).await;

    match result {
        Ok(bytes) => {
            crate::paths::write_secret_file(cert_target, &bytes)?;
            Ok(CloudflareCommandOutput {
                success: true,
                stdout: "Cloudflare authorization completed".into(),
                stderr: String::new(),
            })
        }
        Err(error) => Ok(CloudflareCommandOutput {
            success: false,
            stdout: String::new(),
            stderr: error.to_string(),
        }),
    }
}

pub(crate) fn cleanup_stale_authorization_profiles(app: &AppHandle) {
    if let Ok(dir) = paths::auth_dir(app) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn transfer_key() -> String {
    let mut key = [0_u8; 32];
    key[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    key[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    URL_SAFE.encode(key)
}

fn build_authorization_url(transfer_key: &str) -> AppResult<String> {
    let mut url = reqwest::Url::parse(BASE_LOGIN_URL)
        .map_err(|e| AppError::Msg(format!("Invalid Cloudflare authorization URL: {e}")))?;
    url.query_pairs_mut()
        .append_pair("aud", "")
        .append_pair("callback", &format!("{CALLBACK_STORE_URL}{transfer_key}"));
    Ok(url.to_string())
}

fn open_authorization_window(
    app: &AppHandle,
    connection_id: &str,
    auth_url: &str,
    cancel: Arc<AuthorizationCancel>,
) -> AppResult<AuthorizationWindow> {
    let label = format!(
        "cloudflare-auth-{}",
        paths::sanitize_filename(connection_id)
    );
    close_authorization_window(app, &label);
    let url = auth_url
        .parse()
        .map_err(|e| AppError::Msg(format!("Invalid Cloudflare authorization URL: {e}")))?;
    let profile_root = paths::auth_browser_dir(app, connection_id)?;
    let _ = std::fs::remove_dir_all(&profile_root);
    let data_dir = profile_root.join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&data_dir)?;

    let window = match WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title("Cloudflare Authorization")
        .inner_size(560.0, 760.0)
        .min_inner_size(420.0, 620.0)
        .resizable(true)
        .center()
        .data_directory(data_dir)
        .build()
    {
        Ok(window) => window,
        Err(error) => {
            cleanup_authorization_profile_now(&profile_root);
            return Err(AppError::Msg(format!(
                "Failed to open Cloudflare authorization window: {error}"
            )));
        }
    };
    let cancel_on_close = cancel.clone();
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        ) {
            cancel_on_close.cancel();
        }
    });
    let _ = window.show();
    let _ = window.set_focus();
    Ok(AuthorizationWindow {
        label,
        profile_root,
    })
}

fn close_authorization_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.destroy();
    }
}

fn cleanup_authorization_profile_now(profile_root: &Path) {
    let _ = std::fs::remove_dir_all(profile_root);
}

async fn cleanup_authorization_profile(profile_root: &Path) {
    for attempt in 0..8 {
        match tokio::fs::remove_dir_all(profile_root).await {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) if attempt < 7 => sleep(Duration::from_millis(250)).await,
            Err(_) => return,
        }
    }
}

async fn poll_authorization_cert(
    transfer_key: &str,
    cancel: &AuthorizationCancel,
    timeout: Duration,
) -> AppResult<Vec<u8>> {
    let client = reqwest::Client::builder().timeout(POLL_TIMEOUT).build()?;
    let url = format!("{CALLBACK_STORE_URL}{transfer_key}");
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if cancel.is_cancelled() {
            return Err(AppError::Msg(
                "Cloudflare authorization was cancelled".into(),
            ));
        }

        let request = client.get(&url).send();
        let response = tokio::select! {
            _ = cancel.cancelled() => return Err(AppError::Msg("Cloudflare authorization was cancelled".into())),
            response = request => response,
        };

        match response {
            Ok(response) if response.status().is_success() => {
                let bytes = response.bytes().await?;
                if !bytes.is_empty() {
                    return Ok(bytes.to_vec());
                }
            }
            Ok(response) if response.status().is_server_error() => {
                return Err(AppError::Msg(format!(
                    "Cloudflare authorization service returned HTTP {}",
                    response.status()
                )));
            }
            Ok(_) => {}
            Err(error) if error.is_timeout() => {}
            Err(error) => return Err(error.into()),
        }

        sleep(Duration::from_millis(800)).await;
    }

    Err(AppError::Msg(
        "Cloudflare authorization timed out; try again".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_url_points_to_cloudflare_transfer() {
        let key = URL_SAFE.encode([7_u8; 32]);
        let url = build_authorization_url(&key).unwrap();
        let parsed = reqwest::Url::parse(&url).unwrap();

        assert_eq!(parsed.as_str().split('?').next(), Some(BASE_LOGIN_URL));
        assert!(parsed
            .query_pairs()
            .any(|(name, value)| name == "aud" && value.is_empty()));
        assert!(parsed.query_pairs().any(
            |(name, value)| name == "callback" && value == format!("{CALLBACK_STORE_URL}{key}")
        ));
    }
}
