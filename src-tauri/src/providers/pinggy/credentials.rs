use super::data;
use super::domain::PinggyTunnel;
use crate::error::{AppError, AppResult};
use crate::providers::contract::PINGGY_PROVIDER_ID;
use crate::services::provider_log;
use crate::state::AppState;
use tauri::AppHandle;

pub fn authenticate_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: PinggyTunnel,
) -> AppResult<PinggyTunnel> {
    let tunnel_id = tunnel.id.clone();
    provider_log::credential_verification(
        app,
        state,
        PINGGY_PROVIDER_ID,
        &tunnel_id,
        "Pinggy",
        "Tunnel Token",
        || {
            validate_token(&tunnel.token)
                .and_then(|_| data::update_tunnel_authenticated(app, state, tunnel))
        },
    )
}

pub fn validate_token(token: &str) -> AppResult<()> {
    let token = token.trim();
    if token.is_empty() {
        return Err(AppError::Msg("Pinggy Tunnel Token is required".into()));
    }
    if token.len() < 10 {
        return Err(AppError::Msg(
            "Pinggy Tunnel Token must be at least 10 characters".into(),
        ));
    }
    if token
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(AppError::Msg(
            "Pinggy Tunnel Token cannot contain spaces".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tunnel_token_accepts_manage_tokens_value() {
        assert!(validate_token("LkWtbpY5TOf").is_ok());
    }

    #[test]
    fn tunnel_token_rejects_empty_or_spaced_values() {
        assert!(validate_token("").is_err());
        assert!(validate_token("short").is_err());
        assert!(validate_token("abc def").is_err());
        assert!(validate_token("abc\ndef").is_err());
    }
}
