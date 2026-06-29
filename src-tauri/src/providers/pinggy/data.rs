use super::domain::{PinggyData, PinggyEndpoint, PinggyTunnel};
use super::paths;
use crate::domain::gen_id;
use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    normalize_tunnel_name, ProviderTunnelsUpdatedEvent, PINGGY_PROVIDER_ID,
};
use crate::state::AppState;
use crate::store::save_app_data;
use std::path::Path;
use tauri::{AppHandle, Emitter};

pub fn data(state: &AppState) -> PinggyData {
    state
        .config
        .provider_data(PINGGY_PROVIDER_ID)
        .unwrap_or_default()
}

fn emit_update(app: &AppHandle) {
    let _ = app.emit(
        "provider-tunnels-updated",
        ProviderTunnelsUpdatedEvent {
            provider_id: PINGGY_PROVIDER_ID.into(),
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

pub fn create_tunnel(app: &AppHandle, state: &AppState, name: String) -> AppResult<PinggyTunnel> {
    let tunnel = PinggyTunnel::new(normalize_tunnel_name(&name)?);
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<PinggyData, _>(PINGGY_PROVIDER_ID, |data| {
                ensure_unique_name(&data.tunnels, &tunnel.id, &tunnel.name)?;
                data.tunnels.push(tunnel.clone());
                Ok(tunnel)
            })
    })
}

pub fn update_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: PinggyTunnel,
) -> AppResult<PinggyTunnel> {
    update_tunnel_inner(app, state, tunnel, false)
}

pub fn update_tunnel_authenticated(
    app: &AppHandle,
    state: &AppState,
    tunnel: PinggyTunnel,
) -> AppResult<PinggyTunnel> {
    update_tunnel_inner(app, state, tunnel, true)
}

fn update_tunnel_inner(
    app: &AppHandle,
    state: &AppState,
    mut tunnel: PinggyTunnel,
    credential_authenticated: bool,
) -> AppResult<PinggyTunnel> {
    tunnel.name = normalize_tunnel_name(&tunnel.name)?;
    tunnel.token = tunnel.token.trim().to_string();
    tunnel.server = tunnel.server.trim().to_string();
    if tunnel.server.is_empty() {
        tunnel.server = "free.pinggy.io".into();
    }
    for endpoint in &mut tunnel.endpoints {
        if endpoint.id.trim().is_empty() {
            endpoint.id = gen_id();
        }
        endpoint.name = endpoint.name.trim().to_string();
        endpoint.tunnel_type = endpoint.tunnel_type.trim().to_ascii_lowercase();
        endpoint.local_addr = endpoint.local_addr.trim().to_string();
    }
    tunnel
        .endpoints
        .retain(|endpoint| !endpoint.name.is_empty() || !endpoint.local_addr.is_empty());
    ensure_credentials_for_endpoints(&tunnel)?;
    let previous = current_tunnel(state, &tunnel.id)?;
    crate::services::redaction::restore_if_redacted(&mut tunnel.token, &previous.token);
    ensure_credential_change_authenticated(&previous, &tunnel, credential_authenticated)?;
    ensure_runtime_edit_allowed(state, &previous, &tunnel)?;
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<PinggyData, _>(PINGGY_PROVIDER_ID, |data| {
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
            .update_provider_data::<PinggyData, _>(PINGGY_PROVIDER_ID, |data| {
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

pub fn current_tunnel(state: &AppState, id: &str) -> AppResult<PinggyTunnel> {
    data(state)
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::TunnelNotFound(id.into()))
}

fn remove_config_file(app: &AppHandle, tunnel: &PinggyTunnel) {
    if let (Ok(root), Ok(path)) = (paths::root(app), paths::config_file(app, &tunnel.id)) {
        if !tunnel.config_file.trim().is_empty() {
            crate::paths::remove_file_if_matches(Path::new(tunnel.config_file.trim()), &path);
        }
        crate::paths::remove_file_if_under(&path, &root);
    }
}

fn ensure_unique_name(tunnels: &[PinggyTunnel], id: &str, name: &str) -> AppResult<()> {
    if tunnels
        .iter()
        .any(|item| item.id != id && item.name.eq_ignore_ascii_case(name))
    {
        return Err(AppError::Msg("Connection name already exists".into()));
    }
    Ok(())
}

fn ensure_credentials_for_endpoints(tunnel: &PinggyTunnel) -> AppResult<()> {
    if !tunnel.endpoints.is_empty() && tunnel.token.trim().is_empty() {
        return Err(AppError::Msg(
            "Enter the Pinggy Tunnel Token in Settings before creating tunnels".into(),
        ));
    }
    Ok(())
}

fn ensure_credential_change_authenticated(
    previous: &PinggyTunnel,
    next: &PinggyTunnel,
    credential_authenticated: bool,
) -> AppResult<()> {
    if credential_authenticated || normalized(&previous.token) == normalized(&next.token) {
        return Ok(());
    }
    if next.token.trim().is_empty() {
        return Ok(());
    }
    Err(AppError::Msg(
        "Authenticate Pinggy Tunnel Token before saving".into(),
    ))
}

fn ensure_runtime_edit_allowed(
    state: &AppState,
    previous: &PinggyTunnel,
    next: &PinggyTunnel,
) -> AppResult<()> {
    if !runtime_config_changed(previous, next) {
        return Ok(());
    }
    let status = state
        .provider_runtime
        .reconcile(PINGGY_PROVIDER_ID, &previous.id)
        .status;
    if status.is_active() {
        return Err(AppError::Msg(
            "Stop this Pinggy connection before changing runtime config".into(),
        ));
    }
    Ok(())
}

fn runtime_config_changed(previous: &PinggyTunnel, next: &PinggyTunnel) -> bool {
    normalized(&previous.token) != normalized(&next.token)
        || normalized(&previous.server) != normalized(&next.server)
        || previous.server_port != next.server_port
        || previous.debugger_port != next.debugger_port
        || endpoint_config(&previous.endpoints) != endpoint_config(&next.endpoints)
}

fn endpoint_config(endpoints: &[PinggyEndpoint]) -> Vec<(String, String, String)> {
    let mut config = endpoints
        .iter()
        .filter(|endpoint| endpoint.enabled)
        .map(|endpoint| {
            (
                normalized(&endpoint.name),
                normalized(&endpoint.tunnel_type).to_ascii_lowercase(),
                normalized(&endpoint.local_addr),
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
    fn credential_gate_allows_empty_connection() {
        let tunnel = PinggyTunnel::new("edge");

        assert!(ensure_credentials_for_endpoints(&tunnel).is_ok());
    }

    #[test]
    fn credential_gate_rejects_endpoint_without_token() {
        let mut tunnel = PinggyTunnel::new("edge");
        tunnel.endpoints.push(PinggyEndpoint::default());

        assert!(ensure_credentials_for_endpoints(&tunnel).is_err());
    }

    #[test]
    fn credential_gate_allows_endpoint_with_token() {
        let mut tunnel = PinggyTunnel::new("edge");
        tunnel.token = "token".into();
        tunnel.endpoints.push(PinggyEndpoint::default());

        assert!(ensure_credentials_for_endpoints(&tunnel).is_ok());
    }
}
