use super::domain::CpolarTunnel;
use super::{data, environment, paths};
use crate::error::{AppError, AppResult};
use crate::providers::cli::{output_text, run_with_timeout};
use crate::providers::contract::CPOLAR_PROVIDER_ID;
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
    tunnel: CpolarTunnel,
) -> AppResult<CpolarTunnel> {
    let tunnel_id = tunnel.id.clone();
    provider_log::credential_verification(
        app,
        state,
        CPOLAR_PROVIDER_ID,
        &tunnel_id,
        "cpolar",
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
        return Err(AppError::Msg("cpolar Authtoken is required".into()));
    }
    let exe = environment::resolve_command(app)?;
    let temp_dir = paths::root(app)?
        .join("auth-checks")
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&temp_dir)?;
    let mut command = Command::new(exe);
    command
        .arg("authtoken")
        .arg(authtoken)
        .env("HOME", &temp_dir)
        .env("USERPROFILE", &temp_dir)
        .env("APPDATA", &temp_dir)
        .env("LOCALAPPDATA", &temp_dir);
    let result =
        run_with_timeout(&mut command, "cpolar authentication", AUTH_TIMEOUT).and_then(|output| {
            if output.status.success() {
                Ok(())
            } else {
                let detail = output_text(&output);
                Err(AppError::Msg(if detail.is_empty() {
                    "cpolar authentication failed".into()
                } else {
                    format!("cpolar authentication failed: {detail}")
                }))
            }
        });
    let _ = fs::remove_dir_all(&temp_dir);
    result
}
