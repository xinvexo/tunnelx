use super::domain::{CpolarData, CpolarEndpoint, CpolarTunnel};
use super::paths;
use crate::domain::gen_id;
use crate::error::{AppError, AppResult};
use crate::providers::contract::{
    normalize_tunnel_name, ProviderTunnelsUpdatedEvent, CPOLAR_PROVIDER_ID,
};
use crate::state::AppState;
use crate::store::save_app_data;
use std::path::Path;
use tauri::{AppHandle, Emitter};

const CPOLAR_REGION_CODES: &[&str] = &["", "us", "cn", "cn_vip"];

pub fn data(state: &AppState) -> CpolarData {
    state
        .config
        .provider_data(CPOLAR_PROVIDER_ID)
        .unwrap_or_default()
}

fn emit_update(app: &AppHandle) {
    let _ = app.emit(
        "provider-tunnels-updated",
        ProviderTunnelsUpdatedEvent {
            provider_id: CPOLAR_PROVIDER_ID.into(),
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

pub fn create_tunnel(app: &AppHandle, state: &AppState, name: String) -> AppResult<CpolarTunnel> {
    let tunnel = CpolarTunnel::new(normalize_tunnel_name(&name)?);
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<CpolarData, _>(CPOLAR_PROVIDER_ID, |data| {
                ensure_unique_name(&data.tunnels, &tunnel.id, &tunnel.name)?;
                data.tunnels.push(tunnel.clone());
                Ok(tunnel)
            })
    })
}

pub fn update_tunnel(
    app: &AppHandle,
    state: &AppState,
    tunnel: CpolarTunnel,
) -> AppResult<CpolarTunnel> {
    update_tunnel_inner(app, state, tunnel, false)
}

pub fn update_tunnel_authenticated(
    app: &AppHandle,
    state: &AppState,
    tunnel: CpolarTunnel,
) -> AppResult<CpolarTunnel> {
    update_tunnel_inner(app, state, tunnel, true)
}

fn update_tunnel_inner(
    app: &AppHandle,
    state: &AppState,
    mut tunnel: CpolarTunnel,
    credential_authenticated: bool,
) -> AppResult<CpolarTunnel> {
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
        endpoint.hostname = endpoint.hostname.trim().to_string();
        endpoint.remote_addr = endpoint.remote_addr.trim().to_string();
    }
    tunnel.endpoints.retain(|endpoint| {
        !endpoint.name.is_empty()
            || !endpoint.addr.is_empty()
            || !endpoint.hostname.is_empty()
            || !endpoint.remote_addr.is_empty()
    });
    ensure_credentials_for_endpoints(&tunnel)?;
    let previous = current_tunnel(state, &tunnel.id)?;
    crate::services::redaction::restore_if_redacted(&mut tunnel.authtoken, &previous.authtoken);
    ensure_credential_change_authenticated(&previous, &tunnel, credential_authenticated)?;
    ensure_runtime_edit_allowed(state, &previous, &tunnel)?;
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<CpolarData, _>(CPOLAR_PROVIDER_ID, |data| {
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
    if CPOLAR_REGION_CODES.contains(&region.as_str()) {
        Ok(region)
    } else {
        Err(AppError::Msg(format!(
            "Unsupported cpolar region: {region}"
        )))
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
            .update_provider_data::<CpolarData, _>(CPOLAR_PROVIDER_ID, |data| {
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

pub fn current_tunnel(state: &AppState, id: &str) -> AppResult<CpolarTunnel> {
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
) -> AppResult<CpolarTunnel> {
    save_change(app, state, || {
        state
            .config
            .update_provider_data::<CpolarData, _>(CPOLAR_PROVIDER_ID, |data| {
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

pub fn clear_config_path(app: &AppHandle, state: &AppState, id: &str) -> AppResult<CpolarTunnel> {
    save_config_path(app, state, id, String::new())
}

fn remove_config_file(app: &AppHandle, tunnel: &CpolarTunnel) {
    if let (Ok(root), Ok(path)) = (paths::root(app), paths::config_file(app, &tunnel.id)) {
        if !tunnel.config_file.trim().is_empty() {
            crate::paths::remove_file_if_matches(Path::new(tunnel.config_file.trim()), &path);
        }
        crate::paths::remove_file_if_under(&path, &root);
    }
}

fn ensure_unique_name(tunnels: &[CpolarTunnel], id: &str, name: &str) -> AppResult<()> {
    if tunnels
        .iter()
        .any(|item| item.id != id && item.name.eq_ignore_ascii_case(name))
    {
        return Err(AppError::Msg("Connection name already exists".into()));
    }
    Ok(())
}

fn ensure_credentials_for_endpoints(tunnel: &CpolarTunnel) -> AppResult<()> {
    if !tunnel.endpoints.is_empty() && tunnel.authtoken.trim().is_empty() {
        return Err(AppError::Msg(
            "cpolar Authtoken is required before creating tunnels".into(),
        ));
    }
    Ok(())
}

fn ensure_credential_change_authenticated(
    previous: &CpolarTunnel,
    next: &CpolarTunnel,
    credential_authenticated: bool,
) -> AppResult<()> {
    if credential_authenticated || normalized(&previous.authtoken) == normalized(&next.authtoken) {
        return Ok(());
    }
    if next.authtoken.trim().is_empty() {
        return Ok(());
    }
    Err(AppError::Msg(
        "Authenticate cpolar Authtoken before saving".into(),
    ))
}

fn ensure_runtime_edit_allowed(
    state: &AppState,
    previous: &CpolarTunnel,
    next: &CpolarTunnel,
) -> AppResult<()> {
    if !runtime_config_changed(previous, next) {
        return Ok(());
    }
    let status = state
        .provider_runtime
        .reconcile(CPOLAR_PROVIDER_ID, &previous.id)
        .status;
    if status.is_active() {
        return Err(AppError::Msg(
            "Stop this cpolar connection before changing runtime config".into(),
        ));
    }
    Ok(())
}

fn runtime_config_changed(previous: &CpolarTunnel, next: &CpolarTunnel) -> bool {
    normalized(&previous.authtoken) != normalized(&next.authtoken)
        || normalized(&previous.region) != normalized(&next.region)
        || endpoint_config(&previous.endpoints) != endpoint_config(&next.endpoints)
}

fn endpoint_config(endpoints: &[CpolarEndpoint]) -> Vec<(String, String, String, String, String)> {
    let mut config = endpoints
        .iter()
        .filter(|endpoint| endpoint.enabled)
        .map(|endpoint| {
            (
                normalized(&endpoint.name),
                normalized(&endpoint.proto).to_ascii_lowercase(),
                normalized(&endpoint.addr),
                normalized(&endpoint.hostname),
                normalized(&endpoint.remote_addr),
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
        let tunnel = CpolarTunnel::new("edge");

        assert!(ensure_credentials_for_endpoints(&tunnel).is_ok());
    }

    #[test]
    fn credential_gate_rejects_endpoint_without_authtoken() {
        let mut tunnel = CpolarTunnel::new("edge");
        tunnel.endpoints.push(CpolarEndpoint::default());

        assert!(ensure_credentials_for_endpoints(&tunnel).is_err());
    }

    #[test]
    fn credential_gate_allows_endpoint_with_authtoken() {
        let mut tunnel = CpolarTunnel::new("edge");
        tunnel.authtoken = "token".into();
        tunnel.endpoints.push(CpolarEndpoint::default());

        assert!(ensure_credentials_for_endpoints(&tunnel).is_ok());
    }

    #[test]
    fn region_normalization_accepts_supported_codes_only() {
        assert_eq!(normalize_region(" CN_VIP ").unwrap(), "cn_vip");
        assert!(normalize_region("moon").is_err());
    }
}
