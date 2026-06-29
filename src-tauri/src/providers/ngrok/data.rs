use super::domain::{NgrokData, NgrokEndpoint, NgrokTunnel};
use super::paths;
use crate::domain::gen_id;
use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    normalize_tunnel_name, ProviderTunnelsUpdatedEvent, NGROK_PROVIDER_ID,
};
use crate::state::AppState;
use crate::store::save_app_data;
use std::path::Path;
use tauri::{AppHandle, Emitter};

const NGROK_REGION_CODES: &[&str] = &["", "us", "eu", "ap", "au", "sa", "jp", "in"];

pub fn data(state: &AppState) -> NgrokData {
    state
        .config
        .provider_data(NGROK_PROVIDER_ID)
        .unwrap_or_default()
}

fn emit_update(app: &AppHandle) {
    let _ = app.emit(
        "provider-tunnels-updated",
        ProviderTunnelsUpdatedEvent {
            provider_id: NGROK_PROVIDER_ID.into(),
        },
    );
}

fn save_change<T>(
    app: &AppHandle,
    state: &AppState,
    mutate: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let before = state.config.snapshot()?;
    let value = mutate()?;
    if let Err(error) = save_app_data(app, state) {
        state.config.replace(before);
        return Err(error);
    }
    emit_update(app);
    Ok(value)
}

pub fn create_tunnel(app: &AppHandle, state: &AppState, name: String) -> AppResult<NgrokTunnel> {
    let tunnel = NgrokTunnel::new(normalize_tunnel_name(&name)?);
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<NgrokData, _>(NGROK_PROVIDER_ID, |data| {
                ensure_unique_name(&data.tunnels, &tunnel.id, &tunnel.name)?;
                data.tunnels.push(tunnel.clone());
                Ok(tunnel)
            })
    })
}

pub fn update_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: NgrokTunnel,
) -> AppResult<NgrokTunnel> {
    update_tunnel_inner(app, state, tunnel, false)
}

pub fn update_tunnel_authenticated(
    app: &AppHandle,
    state: &AppState,
    tunnel: NgrokTunnel,
) -> AppResult<NgrokTunnel> {
    update_tunnel_inner(app, state, tunnel, true)
}

fn update_tunnel_inner(
    app: &AppHandle,
    state: &AppState,
    mut tunnel: NgrokTunnel,
    credential_authenticated: bool,
) -> AppResult<NgrokTunnel> {
    tunnel.name = normalize_tunnel_name(&tunnel.name)?;
    tunnel.authtoken = tunnel.authtoken.trim().to_string();
    tunnel.region = normalize_region(&tunnel.region)?;
    for endpoint in &mut tunnel.endpoints {
        if endpoint.id.trim().is_empty() {
            endpoint.id = gen_id();
        }
        endpoint.name = endpoint.name.trim().to_string();
        endpoint.proto = endpoint.proto.trim().to_ascii_lowercase();
        endpoint.addr = endpoint.addr.trim().to_string();
        endpoint.domain = endpoint.domain.trim().to_string();
    }
    tunnel.endpoints.retain(|endpoint| {
        !endpoint.name.is_empty() || !endpoint.addr.is_empty() || !endpoint.domain.is_empty()
    });
    ensure_credentials_for_endpoints(&tunnel)?;
    let previous = current_tunnel(state, &tunnel.id)?;
    crate::services::redaction::restore_if_redacted(&mut tunnel.authtoken, &previous.authtoken);
    ensure_credential_change_authenticated(&previous, &tunnel, credential_authenticated)?;
    ensure_runtime_edit_allowed(state, &previous, &tunnel)?;
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<NgrokData, _>(NGROK_PROVIDER_ID, |data| {
                ensure_unique_name(&data.tunnels, &tunnel.id, &tunnel.name)?;
                let slot = data
                    .tunnels
                    .iter_mut()
                    .find(|item| item.id == tunnel.id)
                    .ok_or_else(|| AppError::TunnelNotFound(tunnel.id.clone()))?;
                tunnel.created_at = slot.created_at;
                tunnel.config_file = slot.config_file.clone();
                tunnel.touch();
                *slot = tunnel.clone();
                Ok(tunnel)
            })
    })
}

fn normalize_region(region: &str) -> AppResult<String> {
    let region = region.trim().to_ascii_lowercase();
    if NGROK_REGION_CODES.contains(&region.as_str()) {
        Ok(region)
    } else {
        Err(AppError::Msg(format!("Unsupported ngrok region: {region}")))
    }
}

pub fn delete_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    super::runtime::cleanup_for_delete(app, state, id)?;
    let old = data(state)
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::TunnelNotFound(id.into()))?;
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<NgrokData, _>(NGROK_PROVIDER_ID, |data| {
                let before = data.tunnels.len();
                data.tunnels.retain(|item| item.id != id);
                if data.tunnels.len() == before {
                    return Err(AppError::TunnelNotFound(id.into()));
                }
                Ok(())
            })
    })?;
    remove_config_file(app, &old);
    Ok(())
}

pub fn current_tunnel(state: &AppState, id: &str) -> AppResult<NgrokTunnel> {
    data(state)
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::TunnelNotFound(id.into()))
}

pub fn save_config_path(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    config_file: String,
) -> AppResult<NgrokTunnel> {
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<NgrokData, _>(NGROK_PROVIDER_ID, |data| {
                let tunnel = data
                    .tunnels
                    .iter_mut()
                    .find(|item| item.id == id)
                    .ok_or_else(|| AppError::TunnelNotFound(id.into()))?;
                tunnel.config_file = config_file;
                tunnel.touch();
                Ok(tunnel.clone())
            })
    })
}

pub fn clear_config_path(app: &AppHandle, state: &AppState, id: &str) -> AppResult<NgrokTunnel> {
    save_config_path(app, state, id, String::new())
}

fn remove_config_file(app: &AppHandle, tunnel: &NgrokTunnel) {
    if let (Ok(root), Ok(path)) = (paths::root(app), paths::config_file(app, &tunnel.id)) {
        if !tunnel.config_file.trim().is_empty() {
            crate::paths::remove_file_if_matches(Path::new(tunnel.config_file.trim()), &path);
        }
        crate::paths::remove_file_if_under(&path, &root);
    }
}

fn ensure_unique_name(tunnels: &[NgrokTunnel], id: &str, name: &str) -> AppResult<()> {
    if tunnels
        .iter()
        .any(|item| item.id != id && item.name.eq_ignore_ascii_case(name))
    {
        return Err(AppError::Msg("Connection name already exists".into()));
    }
    Ok(())
}

fn ensure_credentials_for_endpoints(tunnel: &NgrokTunnel) -> AppResult<()> {
    if !tunnel.endpoints.is_empty() && tunnel.authtoken.trim().is_empty() {
        return Err(AppError::Msg(
            "ngrok Authtoken is required before creating endpoints".into(),
        ));
    }
    Ok(())
}

fn ensure_credential_change_authenticated(
    previous: &NgrokTunnel,
    next: &NgrokTunnel,
    credential_authenticated: bool,
) -> AppResult<()> {
    if credential_authenticated || normalized(&previous.authtoken) == normalized(&next.authtoken) {
        return Ok(());
    }
    if next.authtoken.trim().is_empty() {
        return Ok(());
    }
    Err(AppError::Msg(
        "Authenticate ngrok Authtoken before saving".into(),
    ))
}

fn ensure_runtime_edit_allowed(
    state: &AppState,
    previous: &NgrokTunnel,
    next: &NgrokTunnel,
) -> AppResult<()> {
    if !runtime_config_changed(previous, next) {
        return Ok(());
    }
    let status = state
        .provider_runtime
        .reconcile(NGROK_PROVIDER_ID, &previous.id)
        .status;
    if status.is_active() {
        return Err(AppError::Msg(
            "Stop this ngrok connection before changing runtime config".into(),
        ));
    }
    Ok(())
}

fn runtime_config_changed(previous: &NgrokTunnel, next: &NgrokTunnel) -> bool {
    normalized(&previous.authtoken) != normalized(&next.authtoken)
        || normalized(&previous.region) != normalized(&next.region)
        || endpoint_config(&previous.endpoints) != endpoint_config(&next.endpoints)
}

fn endpoint_config(endpoints: &[NgrokEndpoint]) -> Vec<(String, String, String, String)> {
    let mut config = endpoints
        .iter()
        .filter(|endpoint| endpoint.enabled)
        .map(|endpoint| {
            (
                normalized(&endpoint.name),
                normalized(&endpoint.proto).to_ascii_lowercase(),
                normalized(&endpoint.addr),
                normalized(&endpoint.domain),
            )
        })
        .collect::<Vec<_>>();
    config.sort();
    config
}

fn normalized(value: &str) -> String {
    value.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_config_change_ignores_display_name() {
        let previous = NgrokTunnel::new("edge");
        let mut next = previous.clone();
        next.name = "renamed".into();

        assert!(!runtime_config_changed(&previous, &next));
    }

    #[test]
    fn runtime_config_change_detects_endpoint_update() {
        let mut previous = NgrokTunnel::new("edge");
        previous.endpoints.push(NgrokEndpoint::default());
        let mut next = previous.clone();
        next.endpoints[0].addr = "http://localhost:9090".into();

        assert!(runtime_config_changed(&previous, &next));
    }

    #[test]
    fn runtime_config_change_ignores_disabled_endpoint_update() {
        let mut previous = NgrokTunnel::new("edge");
        previous.endpoints.push(NgrokEndpoint::default());
        previous.endpoints[0].enabled = false;
        let mut next = previous.clone();
        next.endpoints[0].addr = "http://localhost:9090".into();

        assert!(!runtime_config_changed(&previous, &next));
    }

    #[test]
    fn credential_gate_allows_empty_connection() {
        let tunnel = NgrokTunnel::new("edge");

        assert!(ensure_credentials_for_endpoints(&tunnel).is_ok());
    }

    #[test]
    fn credential_gate_rejects_endpoint_without_authtoken() {
        let mut tunnel = NgrokTunnel::new("edge");
        tunnel.endpoints.push(NgrokEndpoint::default());

        assert!(ensure_credentials_for_endpoints(&tunnel).is_err());
    }

    #[test]
    fn credential_gate_allows_endpoint_with_authtoken() {
        let mut tunnel = NgrokTunnel::new("edge");
        tunnel.authtoken = "token".into();
        tunnel.endpoints.push(NgrokEndpoint::default());

        assert!(ensure_credentials_for_endpoints(&tunnel).is_ok());
    }

    #[test]
    fn region_normalization_accepts_supported_codes_only() {
        assert_eq!(normalize_region(" EU ").unwrap(), "eu");
        assert!(normalize_region("moon").is_err());
    }
}
