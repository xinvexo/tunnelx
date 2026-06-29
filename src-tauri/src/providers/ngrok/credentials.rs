use super::domain::NgrokTunnel;
use super::{data, environment, paths};
use crate::error::{AppError, AppResult};
use crate::providers::cli::{output_text, run_with_timeout};
use crate::providers::contract::NGROK_PROVIDER_ID;
use crate::services::provider_log;
use crate::state::AppState;
use std::fs;
use std::process::Command;
use std::time::Duration;
use tauri::AppHandle;
use uuid::Uuid;

const AUTH_TIMEOUT: Duration = Duration::from_secs(20);

pub fn authenticate_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: NgrokTunnel,
) -> AppResult<NgrokTunnel> {
    let tunnel_id = tunnel.id.clone();
    provider_log::credential_verification(
        app,
        state,
        NGROK_PROVIDER_ID,
        &tunnel_id,
        "ngrok",
        "Authtoken",
        || {
            validate_authtoken(app, &tunnel.authtoken)
                .and_then(|_| data::update_tunnel_authenticated(app, state, tunnel))
        },
    )
}

pub fn validate_authtoken(app: &AppHandle, authtoken: &str) -> AppResult<()> {
    let authtoken = authtoken.trim();
    if authtoken.is_empty() {
        return Err(AppError::Msg("ngrok Authtoken is required".into()));
    }
    let exe = environment::resolve_command(app)?;
    let temp_dir = paths::root(app)?
        .join("auth-checks")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&temp_dir)?;
    let config = temp_dir.join("ngrok.yml");
    let mut command = Command::new(exe);
    command
        .arg("config")
        .arg("add-authtoken")
        .arg(authtoken)
        .arg("--config")
        .arg(&config);
    let result =
        run_with_timeout(&mut command, "ngrok authentication", AUTH_TIMEOUT).and_then(|output| {
            if output.status.success() {
                Ok(())
            } else {
                let detail = output_text(&output);
                Err(AppError::Msg(if detail.is_empty() {
                    "ngrok authentication failed".into()
                } else {
                    format!("ngrok authentication failed: {detail}")
                }))
            }
        });
    let _ = fs::remove_dir_all(&temp_dir);
    result
}
