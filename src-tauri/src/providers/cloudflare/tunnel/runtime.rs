use super::config::write_tunnel_config;
use super::hostname::{normalize_hostname, validate_ingress_zones};
use super::remote::{
    append_pending_remote_cleanup, cleanup_hostnames, clear_local_remote_binding,
    flush_pending_remote_cleanup, pending_remote_cleanup_target, release_remote_resources,
    remote_tunnel_missing_error,
};
use super::{ensure_named_tunnel, pending_dns_hostnames, route_pending_dns};
use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::account::credentials::adopt_credentials_for_tunnel;
use crate::providers::cloudflare::data::persistence::save_change;
use crate::providers::cloudflare::domain::{CloudflareCleanupKind, CloudflareTunnel};
use crate::providers::cloudflare::environment::cli::resolve_cloudflared_command;
use crate::providers::cloudflare::paths as cloudflare_paths;
use crate::providers::contract::{
    watchdog_exit_clean, TunnelRuntimeInfo, TunnelRuntimeState, TunnelRuntimeStatusEvent,
    CLOUDFLARE_PROVIDER_ID,
};
use crate::services::{process_watchdog, provider_log, watchdog_relay};
use crate::state::AppState;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
mod process;

use process::cleanup_managed_cloudflared_processes;
use tunnelx_watchdog_protocol::{
    WatchdogCleanupAction, WatchdogCleanupPlan, WatchdogEvent, WatchdogHttpRequest,
    WatchdogRelayProtocol, WatchdogRequest, WatchdogStartProcessRequest,
    WatchdogStopProcessRequest,
};

const CLOUDFLARED_STOP_GRACE: Duration = Duration::from_secs(2);
const CLOUDFLARED_KILL_GRACE: Duration = Duration::from_secs(1);
const WATCHDOG_HTTP_REQUEST_TIMEOUT_MS: u64 = 20_000;
const WATCHDOG_COMMAND_TIMEOUT_MS: u64 = 15_000;
const WATCHDOG_CLEANUP_ATTEMPTS: usize = 3;
const WATCHDOG_CLEANUP_RETRY_DELAY_MS: u64 = 800;
const WATCHDOG_STOP_TIMEOUT_MARGIN: Duration = Duration::from_secs(5);
const WATCHDOG_STOP_TIMEOUT_CAP: Duration = Duration::from_secs(60 * 60);

pub fn start_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let preparing = match state.provider_runtime.begin_start(
        CLOUDFLARE_PROVIDER_ID,
        id,
        "cloudflared preparing",
    ) {
        Ok(info) => info,
        Err(error) => {
            emit_runtime_log(app, state, id, format!("cloudflared start failed: {error}"));
            return Err(error);
        }
    };
    emit_runtime_log(app, state, id, "cloudflared preparing".into());
    emit_status(app, preparing);
    emit_runtime_log(app, state, id, "resolving cloudflared executable".into());
    let path = match resolve_cloudflared_command(app) {
        Ok(path) => path,
        Err(error) => {
            let message = format!("cloudflared preflight failed: {error}");
            emit_runtime_log(app, state, id, message.clone());
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                id,
                TunnelRuntimeState::Errored,
                message,
            );
            emit_status(app, info);
            return Err(error);
        }
    };
    emit_runtime_log(app, state, id, "cloudflared executable ready".into());

    let tunnel = match prepare_tunnel(app, state, id) {
        Ok(tunnel) => tunnel,
        Err(error) => {
            let message = format!("cloudflared preflight failed: {error}");
            emit_runtime_log(app, state, id, message.clone());
            watchdog_relay::release(state, CLOUDFLARE_PROVIDER_ID, id);
            rollback_remote_after_start_failure(app, state, id);
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                id,
                TunnelRuntimeState::Errored,
                message,
            );
            emit_status(app, info);
            return Err(error);
        }
    };
    let cleanup = match cloudflare_watchdog_cleanup(app, &tunnel) {
        Ok(cleanup) => cleanup,
        Err(error) => {
            let message = format!("cloudflared cleanup plan failed before start: {error}");
            emit_runtime_log(app, state, id, message.clone());
            watchdog_relay::release(state, CLOUDFLARE_PROVIDER_ID, id);
            rollback_remote_after_start_failure(app, state, id);
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                id,
                TunnelRuntimeState::Errored,
                message.clone(),
            );
            emit_status(app, info);
            return Err(AppError::Msg(message));
        }
    };
    let stop_timeout = watchdog_stop_timeout(cleanup.as_ref());
    state
        .provider_runtime
        .set_stop_timeout(CLOUDFLARE_PROVIDER_ID, id, stop_timeout);

    emit_runtime_log(app, state, id, "sending cloudflared start command".into());
    let starting = state.provider_runtime.mark_status(
        CLOUDFLARE_PROVIDER_ID,
        id,
        TunnelRuntimeState::Starting,
        "cloudflared starting",
    );
    emit_runtime_log(app, state, id, "cloudflared starting".into());
    emit_status(app, starting.clone());

    match process_watchdog::send(
        app,
        state,
        WatchdogRequest::StartProcess(WatchdogStartProcessRequest {
            provider_id: CLOUDFLARE_PROVIDER_ID.into(),
            tunnel_id: id.to_string(),
            program: path.clone(),
            args: cloudflared_start_args(&tunnel.config_file),
            env: Vec::new(),
            stop_strategy: None,
            cleanup: cleanup.map(Box::new),
        }),
    ) {
        Ok(()) => {
            emit_runtime_log(app, state, id, "cloudflared start command accepted".into());
            Ok(state.provider_runtime.reconcile(CLOUDFLARE_PROVIDER_ID, id))
        }
        Err(error) => {
            let message = format!("failed to start cloudflared ({path}): {error}");
            emit_runtime_log(app, state, id, message.clone());
            watchdog_relay::release(state, CLOUDFLARE_PROVIDER_ID, id);
            rollback_remote_after_start_failure(app, state, id);
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                id,
                TunnelRuntimeState::Errored,
                message.clone(),
            );
            emit_status(app, info);
            Err(AppError::Msg(message))
        }
    }
}

fn cloudflared_start_args(config_file: &str) -> Vec<String> {
    vec![
        "tunnel".into(),
        "--config".into(),
        config_file.into(),
        "run".into(),
    ]
}

fn rollback_remote_after_start_failure(app: &AppHandle, state: &AppState, id: &str) {
    match release_stopped_remote(app, state, id) {
        Ok(Some(message)) => emit_runtime_log(
            app,
            state,
            id,
            format!("start failure rollback completed: {message}"),
        ),
        Ok(None) => {}
        Err(error) => emit_runtime_log(
            app,
            state,
            id,
            format!("remote rollback after start failure failed: {error}"),
        ),
    }
}

pub fn stop_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let current = state.provider_runtime.reconcile(CLOUDFLARE_PROVIDER_ID, id);
    if !current.status.is_active() {
        watchdog_relay::release(state, CLOUDFLARE_PROVIDER_ID, id);
        return if has_local_remote_binding_for_id(state, id) {
            finish_stopped(app, state, id, current)
        } else {
            Ok(current)
        };
    }

    let stopping = state.provider_runtime.mark_status(
        CLOUDFLARE_PROVIDER_ID,
        id,
        TunnelRuntimeState::Stopping,
        "cloudflared stopping",
    );
    emit_runtime_log(app, state, id, "cloudflared stopping".into());
    emit_status(app, stopping);

    let sent = process_watchdog::send_if_alive(
        state,
        WatchdogRequest::StopProcess(WatchdogStopProcessRequest {
            provider_id: CLOUDFLARE_PROVIDER_ID.into(),
            tunnel_id: id.to_string(),
        }),
    )?;

    if !sent {
        watchdog_relay::release(state, CLOUDFLARE_PROVIDER_ID, id);
        let stopped = state.provider_runtime.mark_status(
            CLOUDFLARE_PROVIDER_ID,
            id,
            TunnelRuntimeState::Stopped,
            "cloudflared stopped",
        );
        emit_runtime_log(app, state, id, "cloudflared stopped".into());
        emit_status(app, stopped.clone());
        let _ = cleanup_managed_cloudflared_processes(app);
        return finish_stopped(app, state, id, stopped);
    }

    let timeout = cloudflare_stop_timeout(state, id);
    let stopped = match wait_for_watchdog_stop(state, id, timeout) {
        Some(info) => info,
        None => {
            let message = "cloudflared stop timed out while waiting for watchdog cleanup";
            emit_runtime_log(app, state, id, message.into());
            watchdog_relay::release(state, CLOUDFLARE_PROVIDER_ID, id);
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                id,
                TunnelRuntimeState::Errored,
                message,
            );
            emit_status(app, info);
            return Err(AppError::Msg(message.into()));
        }
    };
    emit_runtime_log(app, state, id, "cloudflared stopped".into());
    emit_status(app, stopped.clone());
    finish_stopped(app, state, id, stopped)
}

fn cloudflare_stop_timeout(state: &AppState, id: &str) -> Duration {
    state
        .provider_runtime
        .stop_timeout(CLOUDFLARE_PROVIDER_ID, id)
        .unwrap_or_else(|| estimated_cloudflare_stop_timeout(state, id))
}

fn estimated_cloudflare_stop_timeout(state: &AppState, id: &str) -> Duration {
    state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .map(|tunnel| {
            if !has_local_remote_binding(&tunnel) {
                return watchdog_stop_timeout(None);
            }
            let dns_request_count = unique_ingress_hostname_count(&tunnel).saturating_mul(2);
            estimated_watchdog_stop_timeout(dns_request_count.saturating_add(1))
        })
        .unwrap_or_else(|| watchdog_stop_timeout(None))
}

fn unique_ingress_hostname_count(tunnel: &CloudflareTunnel) -> usize {
    tunnel
        .ingress
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| {
            rule.hostname
                .trim()
                .trim_end_matches('.')
                .to_ascii_lowercase()
        })
        .filter(|hostname| !hostname.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn estimated_watchdog_stop_timeout(http_request_count: usize) -> Duration {
    let one_attempt = checked_duration_mul(
        Duration::from_millis(WATCHDOG_HTTP_REQUEST_TIMEOUT_MS),
        http_request_count,
    );
    let cleanup = checked_duration_add(
        checked_duration_mul(one_attempt, WATCHDOG_CLEANUP_ATTEMPTS),
        checked_duration_mul(
            Duration::from_millis(WATCHDOG_CLEANUP_RETRY_DELAY_MS),
            WATCHDOG_CLEANUP_ATTEMPTS.saturating_sub(1),
        ),
    );
    bounded_stop_timeout(checked_duration_add(
        CLOUDFLARED_STOP_GRACE + CLOUDFLARED_KILL_GRACE + WATCHDOG_STOP_TIMEOUT_MARGIN,
        cleanup,
    ))
}

fn watchdog_stop_timeout(cleanup: Option<&WatchdogCleanupPlan>) -> Duration {
    let cleanup_timeout = cleanup
        .map(watchdog_cleanup_timeout)
        .unwrap_or_else(|| Duration::from_millis(0));
    bounded_stop_timeout(checked_duration_add(
        CLOUDFLARED_STOP_GRACE + CLOUDFLARED_KILL_GRACE + WATCHDOG_STOP_TIMEOUT_MARGIN,
        cleanup_timeout,
    ))
}

fn watchdog_cleanup_timeout(cleanup: &WatchdogCleanupPlan) -> Duration {
    let attempts = cleanup
        .retry_attempts
        .unwrap_or(WATCHDOG_CLEANUP_ATTEMPTS)
        .max(1);
    let retry_delay = Duration::from_millis(
        cleanup
            .retry_delay_ms
            .unwrap_or(WATCHDOG_CLEANUP_RETRY_DELAY_MS),
    );
    let one_attempt = cleanup
        .actions
        .iter()
        .map(cleanup_action_timeout)
        .fold(Duration::from_millis(0), checked_duration_add);
    checked_duration_add(
        checked_duration_mul(one_attempt, attempts),
        checked_duration_mul(retry_delay, attempts.saturating_sub(1)),
    )
}

fn cleanup_action_timeout(action: &WatchdogCleanupAction) -> Duration {
    match action {
        WatchdogCleanupAction::Http { request } => http_request_timeout(request),
        WatchdogCleanupAction::HttpJsonDeleteMatches {
            list_request,
            delete_request,
            ..
        } => checked_duration_add(
            http_request_timeout(list_request),
            http_request_timeout(delete_request),
        ),
        WatchdogCleanupAction::Command { timeout_ms, .. } => {
            Duration::from_millis(timeout_ms.unwrap_or(WATCHDOG_COMMAND_TIMEOUT_MS))
        }
        WatchdogCleanupAction::RemoveFile { .. } => Duration::from_millis(0),
    }
}

fn http_request_timeout(request: &WatchdogHttpRequest) -> Duration {
    Duration::from_millis(
        request
            .timeout_ms
            .unwrap_or(WATCHDOG_HTTP_REQUEST_TIMEOUT_MS),
    )
}

fn checked_duration_add(left: Duration, right: Duration) -> Duration {
    left.checked_add(right).unwrap_or(WATCHDOG_STOP_TIMEOUT_CAP)
}

fn checked_duration_mul(duration: Duration, factor: usize) -> Duration {
    duration
        .checked_mul(factor.min(u32::MAX as usize) as u32)
        .unwrap_or(WATCHDOG_STOP_TIMEOUT_CAP)
}

fn bounded_stop_timeout(timeout: Duration) -> Duration {
    timeout.min(WATCHDOG_STOP_TIMEOUT_CAP)
}

pub fn stop_all_tunnels(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let mut errors = Vec::new();
    let ids = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .map(|tunnel| tunnel.id)
        .collect::<Vec<_>>();
    for id in ids {
        if let Err(error) = shutdown_tunnel(app, state, &id) {
            errors.push(format!("{id}: {error}"));
        }
    }
    if let Err(error) = cleanup_managed_cloudflared_processes(app) {
        errors.push(format!(
            "managed cloudflared process cleanup failed: {error}"
        ));
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Msg(errors.join("; ")))
    }
}

pub fn cleanup_on_start(app: &AppHandle, state: &AppState) -> AppResult<()> {
    let mut errors = Vec::new();
    if let Err(error) = cleanup_managed_cloudflared_processes(app) {
        errors.push(format!("local cloudflared process cleanup failed: {error}"));
    }
    for tunnel in state.config.cloudflare().tunnels {
        if has_local_remote_binding(&tunnel) {
            match release_stopped_remote(app, state, &tunnel.id) {
                Ok(Some(message)) => {
                    emit_runtime_log(
                        app,
                        state,
                        &tunnel.id,
                        format!("startup cleanup completed: {message}"),
                    );
                    let info = state.provider_runtime.mark_status(
                        CLOUDFLARE_PROVIDER_ID,
                        &tunnel.id,
                        TunnelRuntimeState::Stopped,
                        message,
                    );
                    emit_status(app, info);
                }
                Ok(None) => {}
                Err(error) => {
                    let message =
                        format!("startup Cloudflare remote resource cleanup failed: {error}");
                    emit_runtime_log(app, state, &tunnel.id, message.clone());
                    let info = state.provider_runtime.mark_status(
                        CLOUDFLARE_PROVIDER_ID,
                        &tunnel.id,
                        TunnelRuntimeState::Errored,
                        message.clone(),
                    );
                    emit_status(app, info);
                    errors.push(format!("{}({}): {error}", tunnel.name, tunnel.id));
                }
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::Msg(errors.join("; ")))
    }
}

fn shutdown_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<TunnelRuntimeInfo> {
    let current = state.provider_runtime.reconcile(CLOUDFLARE_PROVIDER_ID, id);
    if current.status.is_active() {
        return stop_tunnel(app, state, id);
    }
    if has_local_remote_binding_for_id(state, id) {
        finish_stopped(app, state, id, current)
    } else {
        Ok(current)
    }
}

fn wait_for_watchdog_stop(
    state: &AppState,
    id: &str,
    timeout: Duration,
) -> Option<TunnelRuntimeInfo> {
    state.provider_runtime.wait_for_inactive(
        CLOUDFLARE_PROVIDER_ID,
        id,
        timeout,
        Duration::from_millis(50),
    )
}

fn has_local_remote_binding(tunnel: &CloudflareTunnel) -> bool {
    !tunnel.tunnel_id.trim().is_empty()
        || !tunnel.credentials_file.trim().is_empty()
        || tunnel.ingress.iter().any(|rule| rule.dns_routed)
        || !tunnel.pending_remote_cleanup.is_empty()
}

fn has_local_remote_binding_for_id(state: &AppState, id: &str) -> bool {
    state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .map(|tunnel| has_local_remote_binding(&tunnel))
        .unwrap_or(false)
}

fn cloudflare_watchdog_cleanup(
    app: &AppHandle,
    tunnel: &CloudflareTunnel,
) -> AppResult<Option<WatchdogCleanupPlan>> {
    let mut actions =
        tauri::async_runtime::block_on(super::api::remote_cleanup_actions_for_tunnel(tunnel))?;
    let root = cloudflare_paths::root(app)?;
    actions.extend(
        [tunnel.credentials_file.trim(), tunnel.config_file.trim()]
            .into_iter()
            .filter_map(|path| managed_runtime_file_cleanup_action(path, &root)),
    );
    if actions.is_empty() {
        return Ok(None);
    }
    Ok(Some(WatchdogCleanupPlan {
        actions,
        retry_attempts: Some(3),
        retry_delay_ms: Some(800),
    }))
}

fn managed_runtime_file_cleanup_action(path: &str, root: &Path) -> Option<WatchdogCleanupAction> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let path_ref = Path::new(path);
    crate::paths::path_is_under(path_ref, root).then(|| WatchdogCleanupAction::RemoveFile {
        path: path.to_string(),
    })
}

fn finish_stopped(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    stopped: TunnelRuntimeInfo,
) -> AppResult<TunnelRuntimeInfo> {
    match release_stopped_remote(app, state, id) {
        Ok(Some(message)) => {
            emit_runtime_log(app, state, id, message.clone());
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                id,
                TunnelRuntimeState::Stopped,
                message,
            );
            emit_status(app, info.clone());
            Ok(info)
        }
        Ok(None) => Ok(stopped),
        Err(error) => {
            let message = format!("cloudflared stopped, but remote cleanup failed: {error}");
            emit_runtime_log(app, state, id, message.clone());
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                id,
                TunnelRuntimeState::Errored,
                message.clone(),
            );
            emit_status(app, info);
            Err(AppError::Msg(message))
        }
    }
}

fn release_stopped_remote(
    app: &AppHandle<impl tauri::Runtime>,
    state: &AppState,
    id: &str,
) -> AppResult<Option<String>> {
    let mut tunnel = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Msg("Cloudflare Tunnel not found".into()))?;
    let old_credentials = tunnel.credentials_file.clone();
    let old_config = tunnel.config_file.clone();
    let released = release_remote_resources(app, &mut tunnel)?;
    if released {
        tunnel.config_file.clear();
        save_change(app, state, || {
            state.config.upsert_cloudflare_tunnel(tunnel.clone())
        })?;
        remove_nonempty_runtime_file(app, &old_credentials);
        remove_nonempty_runtime_file(app, &old_config);
    }

    let pending_released = flush_pending_remote_cleanup(app, state, id)?;
    if released || pending_released {
        Ok(Some("Cloudflare DNS and named tunnel released".into()))
    } else {
        Ok(None)
    }
}

fn clear_watchdog_released_binding<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    id: &str,
) -> AppResult<bool> {
    let mut tunnel = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Msg("Cloudflare Tunnel not found".into()))?;
    if !has_local_remote_binding(&tunnel) {
        return Ok(false);
    }

    let old_credentials = tunnel.credentials_file.clone();
    let old_config = tunnel.config_file.clone();
    tunnel.tunnel_id.clear();
    tunnel.credentials_file.clear();
    tunnel.config_file.clear();
    for rule in &mut tunnel.ingress {
        rule.dns_routed = false;
    }

    save_change(app, state, || {
        state.config.upsert_cloudflare_tunnel(tunnel.clone())
    })?;
    remove_nonempty_runtime_file(app, &old_credentials);
    remove_nonempty_runtime_file(app, &old_config);
    Ok(true)
}

pub fn tunnel_status(_app: &AppHandle, state: &AppState, id: &str) -> TunnelRuntimeInfo {
    state.provider_runtime.reconcile(CLOUDFLARE_PROVIDER_ID, id)
}

fn prepare_tunnel(app: &AppHandle, state: &AppState, id: &str) -> AppResult<CloudflareTunnel> {
    crate::store::ensure_app_data_writable(state)?;
    emit_runtime_log(app, state, id, "checking pending Cloudflare cleanup".into());
    flush_pending_remote_cleanup(app, state, id)?;
    emit_runtime_log(app, state, id, "loading Cloudflare tunnel config".into());
    let mut tunnel = state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Msg("Cloudflare Tunnel not found".into()))?;
    let old_credentials = tunnel.credentials_file.clone();
    let old_config = tunnel.config_file.clone();

    if tunnel.name.trim().is_empty() {
        return Err(AppError::Msg("Tunnel name is required".into()));
    }
    if !tunnel
        .ingress
        .iter()
        .any(|rule| !rule.hostname.trim().is_empty() && !rule.service.trim().is_empty())
    {
        return Err(AppError::Msg(
            "At least one complete hostname ingress is required".into(),
        ));
    }
    emit_runtime_log(app, state, id, "validating Cloudflare ingress zones".into());
    validate_ingress_zones(&tunnel)?;

    let named_tunnel_action = if tunnel.tunnel_id.trim().is_empty() {
        "creating Cloudflare named tunnel"
    } else {
        "checking Cloudflare named tunnel"
    };
    emit_runtime_log(app, state, id, named_tunnel_action.into());
    let mut created_remote = ensure_named_tunnel(app, state, &mut tunnel)?;
    emit_named_tunnel_ready_log(app, state, id, &tunnel, created_remote);

    if tunnel.credentials_file.trim().is_empty() {
        emit_runtime_log(app, state, id, "locating cloudflared credentials".into());
        tunnel.credentials_file =
            match adopt_credentials_for_tunnel(app, &tunnel.id, &tunnel.tunnel_id) {
                Some(credentials_file) => credentials_file,
                None => {
                    return fail_prepared_tunnel(
                        app,
                        state,
                        &tunnel,
                        created_remote,
                        AppError::Msg("Runtime credentials not found; recreate the tunnel".into()),
                    );
                }
            };
    }
    emit_runtime_log(app, state, id, "checking cloudflared credentials".into());
    if !Path::new(tunnel.credentials_file.trim()).exists() {
        return fail_prepared_tunnel(
            app,
            state,
            &tunnel,
            created_remote,
            AppError::Msg("Runtime credentials file is missing; recreate the tunnel".into()),
        );
    }
    let pending_dns = pending_dns_hostnames(&tunnel);
    emit_runtime_log(app, state, id, "checking Cloudflare DNS routes".into());
    if pending_dns.is_empty() {
        emit_runtime_log(app, state, id, "Cloudflare DNS routes already ready".into());
    }
    let routed_hosts = match route_pending_dns(app, state, &mut tunnel) {
        Ok(hosts) => hosts,
        Err(error) if remote_tunnel_missing_error(&error) && created_remote => {
            emit_runtime_log(
                app,
                state,
                id,
                "Cloudflare named tunnel is not visible yet; retrying DNS routes".into(),
            );
            match route_pending_dns_after_create(app, state, &mut tunnel) {
                Ok(hosts) => hosts,
                Err(error) => {
                    return fail_prepared_tunnel(app, state, &tunnel, created_remote, error);
                }
            }
        }
        Err(error) if remote_tunnel_missing_error(&error) => {
            emit_runtime_log(
                app,
                state,
                id,
                "Cloudflare named tunnel is missing; recreating".into(),
            );
            clear_remote_state(app, &mut tunnel);
            created_remote = ensure_named_tunnel(app, state, &mut tunnel)?;
            emit_named_tunnel_ready_log(app, state, id, &tunnel, created_remote);
            match route_pending_dns_after_create(app, state, &mut tunnel) {
                Ok(hosts) => hosts,
                Err(error) => {
                    return fail_prepared_tunnel(app, state, &tunnel, created_remote, error);
                }
            }
        }
        Err(error) => return fail_prepared_tunnel(app, state, &tunnel, created_remote, error),
    };
    emit_runtime_log(app, state, id, "writing cloudflared config".into());
    let mut runtime_tunnel = tunnel.clone();
    if let Err(error) = prepare_traffic_relay(app, state, &mut runtime_tunnel) {
        return fail_prepared_tunnel(app, state, &tunnel, created_remote, error);
    }
    tunnel.config_file = match write_tunnel_config(app, &runtime_tunnel) {
        Ok(config_file) => config_file,
        Err(error) => return fail_prepared_tunnel(app, state, &tunnel, created_remote, error),
    };
    emit_runtime_log(app, state, id, "cloudflared config ready".into());
    if old_credentials != tunnel.credentials_file
        || old_config != tunnel.config_file
        || created_remote
        || !routed_hosts.is_empty()
    {
        emit_runtime_log(app, state, id, "saving Cloudflare tunnel state".into());
        let saved = match save_change(app, state, || {
            state.config.upsert_cloudflare_tunnel(tunnel.clone())
        }) {
            Ok(saved) => saved,
            Err(error) => {
                remove_nonempty_runtime_file(app, &tunnel.config_file);
                return fail_prepared_tunnel(app, state, &tunnel, created_remote, error);
            }
        };
        emit_runtime_log(app, state, id, "Cloudflare tunnel state saved".into());
        Ok(saved)
    } else {
        Ok(tunnel)
    }
}

fn fail_prepared_tunnel<T>(
    app: &AppHandle,
    state: &AppState,
    tunnel: &CloudflareTunnel,
    created_remote: bool,
    error: AppError,
) -> AppResult<T> {
    match queue_start_failure_cleanup(app, state, tunnel, created_remote) {
        Ok(true) => emit_runtime_log(
            app,
            state,
            &tunnel.id,
            "Cloudflare startup cleanup was queued".into(),
        ),
        Ok(false) => {}
        Err(cleanup_error) => {
            let message =
                format!("{error}; failed to save Cloudflare startup cleanup: {cleanup_error}");
            emit_runtime_log(app, state, &tunnel.id, message.clone());
            return Err(AppError::Msg(message));
        }
    }
    Err(error)
}

fn queue_start_failure_cleanup(
    app: &AppHandle,
    state: &AppState,
    tunnel: &CloudflareTunnel,
    created_remote: bool,
) -> AppResult<bool> {
    let Some(saved) = start_failure_cleanup_snapshot(tunnel, created_remote) else {
        return Ok(false);
    };
    save_change(app, state, || {
        state.config.upsert_cloudflare_tunnel(saved.clone())
    })?;
    Ok(true)
}

fn start_failure_cleanup_snapshot(
    tunnel: &CloudflareTunnel,
    created_remote: bool,
) -> Option<CloudflareTunnel> {
    let kind = if created_remote {
        CloudflareCleanupKind::RemoteTunnel
    } else {
        CloudflareCleanupKind::DnsRoutes
    };
    let hostnames = cleanup_hostnames(tunnel, !created_remote);
    let target = pending_remote_cleanup_target(tunnel, kind, hostnames)?;
    let cleanup_hostnames = target
        .dns_hostnames
        .iter()
        .map(|hostname| normalize_hostname(hostname))
        .collect::<std::collections::HashSet<_>>();
    let mut saved = tunnel.clone();
    append_pending_remote_cleanup(&mut saved, target);
    if created_remote {
        clear_local_remote_binding(&mut saved);
    } else {
        for rule in &mut saved.ingress {
            if cleanup_hostnames.contains(&normalize_hostname(&rule.hostname)) {
                rule.dns_routed = false;
            }
        }
    }
    Some(saved)
}

fn remove_nonempty_runtime_file(app: &AppHandle<impl tauri::Runtime>, path: &str) {
    let path = path.trim();
    if path.is_empty() {
        return;
    }
    if let Ok(root) = cloudflare_paths::root(app) {
        crate::paths::remove_file_if_under(Path::new(path), &root);
    }
}

fn prepare_traffic_relay(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
) -> AppResult<()> {
    if !state.config.settings().traffic_stats_enabled {
        watchdog_relay::release(state, CLOUDFLARE_PROVIDER_ID, &tunnel.id);
        return Ok(());
    }
    let endpoints = tunnel
        .ingress
        .iter()
        .filter(|rule| {
            rule.enabled && !rule.hostname.trim().is_empty() && !rule.service.trim().is_empty()
        })
        .map(|rule| watchdog_relay::RelayEndpointPlan {
            endpoint_id: rule.id.clone(),
            endpoint_name: rule.name.clone(),
            endpoint_type: "ingress".into(),
            protocol: WatchdogRelayProtocol::Tcp,
            target: rule.service.clone(),
        })
        .collect::<Vec<_>>();
    let bindings =
        watchdog_relay::prepare(app, state, CLOUDFLARE_PROVIDER_ID, &tunnel.id, endpoints)?;
    for binding in bindings {
        if let Some(rule) = tunnel
            .ingress
            .iter_mut()
            .find(|rule| rule.id == binding.endpoint_id)
        {
            rule.runtime_http_host_header = http_origin_authority(&binding.target)
                .map(str::to_string)
                .unwrap_or_default();
            watchdog_relay::apply_binding(&mut rule.service, &binding);
        }
    }
    Ok(())
}

fn http_origin_authority(service: &str) -> Option<&str> {
    let service = service.trim();
    let rest = service
        .strip_prefix("http://")
        .or_else(|| service.strip_prefix("https://"))?;
    let authority = rest.split('/').next().unwrap_or_default().trim();
    if authority.is_empty() {
        None
    } else {
        Some(authority)
    }
}

fn emit_named_tunnel_ready_log(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    tunnel: &CloudflareTunnel,
    created_remote: bool,
) {
    let tunnel_id = tunnel.tunnel_id.trim();
    let action = if created_remote { "created" } else { "ready" };
    if tunnel_id.is_empty() {
        emit_runtime_log(app, state, id, format!("Cloudflare named tunnel {action}"));
    } else {
        emit_runtime_log(
            app,
            state,
            id,
            format!("Cloudflare named tunnel {action}: {tunnel_id}"),
        );
    }
}

fn clear_remote_state(app: &AppHandle, tunnel: &mut CloudflareTunnel) {
    let old_credentials = tunnel.credentials_file.trim().to_string();
    tunnel.tunnel_id.clear();
    tunnel.credentials_file.clear();
    for rule in &mut tunnel.ingress {
        rule.dns_routed = false;
    }
    remove_nonempty_runtime_file(app, &old_credentials);
}

fn route_pending_dns_after_create(
    app: &AppHandle,
    state: &AppState,
    tunnel: &mut CloudflareTunnel,
) -> AppResult<Vec<String>> {
    let mut last_error: Option<AppError> = None;
    for attempt in 0..3 {
        if attempt > 0 {
            emit_runtime_log(
                app,
                state,
                &tunnel.id,
                format!(
                    "retrying Cloudflare DNS routes after tunnel propagation ({}/3)",
                    attempt + 1
                ),
            );
            std::thread::sleep(Duration::from_millis(800));
        }
        match route_pending_dns(app, state, tunnel) {
            Ok(hosts) => return Ok(hosts),
            Err(error) if remote_tunnel_missing_error(&error) => {
                emit_runtime_log(
                    app,
                    state,
                    &tunnel.id,
                    format!(
                        "Cloudflare DNS routes are waiting for named tunnel propagation ({}/3): {error}",
                        attempt + 1
                    ),
                );
                last_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| AppError::Msg("Cloudflare named tunnel is missing".into())))
}

pub(crate) fn handle_watchdog_event<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    event: WatchdogEvent,
) {
    match event {
        WatchdogEvent::OwnerAttached { .. }
        | WatchdogEvent::OwnerLost { .. }
        | WatchdogEvent::OwnerHeartbeatExpired
        | WatchdogEvent::RelayError { .. }
        | WatchdogEvent::RelayStats { .. } => {}
        WatchdogEvent::ProcessStarted { tunnel_id, pid, .. } => {
            let info = state.provider_runtime.mark_spawned(
                CLOUDFLARE_PROVIDER_ID,
                &tunnel_id,
                pid,
                "cloudflared process started",
            );
            emit_runtime_log(
                app,
                state,
                &tunnel_id,
                "cloudflared process started; waiting for tunnel registration".into(),
            );
            emit_status(app, info);
        }
        WatchdogEvent::ProcessSnapshot { tunnel_id, pid, .. } => {
            state.provider_runtime.mark_spawned(
                CLOUDFLARE_PROVIDER_ID,
                &tunnel_id,
                pid,
                provider_log::watchdog_recovered_line("cloudflared"),
            );
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                &tunnel_id,
                TunnelRuntimeState::Running,
                provider_log::watchdog_recovered_line("cloudflared"),
            );
            emit_runtime_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_recovered_line("cloudflared"),
            );
            emit_status(app, info);
        }
        WatchdogEvent::ProcessLog {
            tunnel_id, line, ..
        } => {
            if !line.trim().is_empty() {
                let registered = cloudflared_registered_connection(&line);
                emit_native_log(app, state, &tunnel_id, line);
                if registered {
                    let current = state
                        .provider_runtime
                        .info(CLOUDFLARE_PROVIDER_ID, &tunnel_id);
                    if current.status == TunnelRuntimeState::Starting {
                        let info = state.provider_runtime.mark_status(
                            CLOUDFLARE_PROVIDER_ID,
                            &tunnel_id,
                            TunnelRuntimeState::Running,
                            "cloudflared registered tunnel connection",
                        );
                        emit_runtime_log(app, state, &tunnel_id, "cloudflared started".into());
                        emit_status(app, info);
                    }
                }
            }
        }
        WatchdogEvent::ProcessExit {
            tunnel_id,
            success,
            cleanup_success,
            cleanup_error,
            ..
        } => {
            watchdog_relay::clear_local(state, CLOUDFLARE_PROVIDER_ID, &tunnel_id);
            let was_stopping = state
                .provider_runtime
                .info(CLOUDFLARE_PROVIDER_ID, &tunnel_id)
                .status
                == TunnelRuntimeState::Stopping;
            let clean_exit = watchdog_exit_clean(success, cleanup_success);
            let cleanup_detail = || {
                cleanup_error
                    .clone()
                    .filter(|error| !error.trim().is_empty())
                    .unwrap_or_else(|| "watchdog did not report remote cleanup result".into())
            };
            let exit_message = if !success || cleanup_success == Some(false) {
                provider_log::watchdog_exit_state_message_with_cleanup(
                    "cloudflared",
                    "remote cleanup",
                    success,
                    cleanup_success,
                    cleanup_error.as_deref(),
                )
            } else {
                "cloudflared stopped".to_string()
            };
            let info = state.provider_runtime.mark_exit(
                CLOUDFLARE_PROVIDER_ID,
                &tunnel_id,
                clean_exit,
                exit_message,
            );
            let has_remote_binding = has_local_remote_binding_for_id(state, &tunnel_id);
            let info = if cleanup_success == Some(true) {
                sync_watchdog_released_binding(app, state, &tunnel_id, info)
            } else if cleanup_success == Some(false) || was_stopping || !has_remote_binding {
                info
            } else {
                let message = format!(
                    "{}, remote cleanup was not confirmed: {}",
                    info.message,
                    cleanup_detail()
                );
                emit_runtime_log(app, state, &tunnel_id, message.clone());
                state.provider_runtime.mark_status(
                    CLOUDFLARE_PROVIDER_ID,
                    &tunnel_id,
                    TunnelRuntimeState::Errored,
                    message,
                )
            };
            emit_runtime_log(app, state, &tunnel_id, info.message.clone());
            emit_status(app, info);
        }
        WatchdogEvent::ProcessError {
            tunnel_id, message, ..
        } => {
            watchdog_relay::clear_local(state, CLOUDFLARE_PROVIDER_ID, &tunnel_id);
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                &tunnel_id,
                TunnelRuntimeState::Errored,
                message.clone(),
            );
            emit_runtime_log(
                app,
                state,
                &tunnel_id,
                provider_log::watchdog_error_line(&message),
            );
            emit_status(app, info);
        }
    }
}

fn cloudflared_registered_connection(line: &str) -> bool {
    line.to_ascii_lowercase()
        .contains("registered tunnel connection")
}

fn sync_watchdog_released_binding<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    info: TunnelRuntimeInfo,
) -> TunnelRuntimeInfo {
    match clear_watchdog_released_binding(app, state, tunnel_id) {
        Ok(true) => {
            emit_runtime_log(
                app,
                state,
                tunnel_id,
                "Cloudflare DNS and named tunnel were released by watchdog".into(),
            );
            let message = if info.message.trim().is_empty() {
                "cloudflared stopped, DNS and named tunnel released".into()
            } else {
                format!("{}, DNS and named tunnel released", info.message)
            };
            state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                tunnel_id,
                info.status,
                message,
            )
        }
        Ok(false) => info,
        Err(error) => {
            let message = format!(
                "watchdog stopped cloudflared, but local cleanup state save failed: {error}"
            );
            emit_runtime_log(app, state, tunnel_id, message.clone());
            state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                tunnel_id,
                TunnelRuntimeState::Errored,
                message,
            )
        }
    }
}

pub(crate) fn handle_watchdog_eof<R: tauri::Runtime>(app: &AppHandle<R>, state: &AppState) {
    for (provider_id, tunnel_id) in state.provider_runtime.active_keys() {
        if provider_id != CLOUDFLARE_PROVIDER_ID {
            continue;
        }
        recover_after_watchdog_eof(app, state, &tunnel_id);
    }
}

fn recover_after_watchdog_eof<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
) {
    watchdog_relay::clear_local(state, CLOUDFLARE_PROVIDER_ID, tunnel_id);
    provider_log::emit_watchdog_stream_closed(app, state, CLOUDFLARE_PROVIDER_ID, tunnel_id);
    emit_runtime_log(
        app,
        state,
        tunnel_id,
        "running fallback Cloudflare cleanup after watchdog exit".into(),
    );
    let local_cleanup = cleanup_managed_cloudflared_processes(app);
    let remote_cleanup = release_stopped_remote(app, state, tunnel_id);
    match (local_cleanup, remote_cleanup) {
        (Ok(_), Ok(Some(message))) => {
            emit_runtime_log(app, state, tunnel_id, message.clone());
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                tunnel_id,
                TunnelRuntimeState::Stopped,
                message,
            );
            emit_status(app, info);
        }
        (Ok(_), Ok(None)) => {
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                tunnel_id,
                TunnelRuntimeState::Stopped,
                "cloudflared stopped",
            );
            emit_status(app, info);
        }
        (local_result, remote_result) => {
            let mut details = Vec::new();
            if let Err(error) = local_result {
                details.push(format!("local process cleanup failed: {error}"));
            }
            if let Err(error) = remote_result {
                details.push(format!("remote resource cleanup failed: {error}"));
            }
            let message = if details.is_empty() {
                "watchdog exited; Cloudflare cleanup state is unknown".into()
            } else {
                details.join("; ")
            };
            emit_runtime_log(app, state, tunnel_id, message.clone());
            let info = state.provider_runtime.mark_status(
                CLOUDFLARE_PROVIDER_ID,
                tunnel_id,
                TunnelRuntimeState::Errored,
                message,
            );
            emit_status(app, info);
        }
    }
}

fn emit_runtime_log<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: String,
) {
    provider_log::emit_system(app, state, CLOUDFLARE_PROVIDER_ID, tunnel_id, line);
}

fn emit_native_log<R: tauri::Runtime>(
    app: &AppHandle<R>,
    state: &AppState,
    tunnel_id: &str,
    line: String,
) {
    provider_log::emit_native(app, state, CLOUDFLARE_PROVIDER_ID, tunnel_id, line);
}

fn emit_status<R: tauri::Runtime>(app: &AppHandle<R>, info: TunnelRuntimeInfo) {
    let _ = app.emit(
        "provider-tunnel-status-changed",
        TunnelRuntimeStatusEvent { info },
    );
    crate::refresh_connection_icon(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::providers::cloudflare::domain::CloudflareIngressRule;

    fn http_request(timeout_ms: u64) -> Box<WatchdogHttpRequest> {
        Box::new(WatchdogHttpRequest {
            method: "GET".into(),
            url: "https://example.com".into(),
            headers: Vec::new(),
            body: None,
            accepted_statuses: Vec::new(),
            json_expectation: None,
            timeout_ms: Some(timeout_ms),
        })
    }

    #[test]
    fn watchdog_stop_timeout_accounts_for_cleanup_retries() {
        let plan = WatchdogCleanupPlan {
            actions: vec![
                WatchdogCleanupAction::Http {
                    request: http_request(1_000),
                },
                WatchdogCleanupAction::HttpJsonDeleteMatches {
                    list_request: http_request(2_000),
                    items_field: "result".into(),
                    id_field: "id".into(),
                    match_field: "content".into(),
                    match_value: "target".into(),
                    match_ignore_ascii_case: true,
                    match_trim_trailing_dot: true,
                    delete_request: http_request(3_000),
                    id_placeholder: "{id}".into(),
                },
            ],
            retry_attempts: Some(2),
            retry_delay_ms: Some(50),
        };

        assert_eq!(
            watchdog_stop_timeout(Some(&plan)),
            Duration::from_millis(20_050)
        );
    }

    #[test]
    fn cloudflared_start_args_let_cloudflared_choose_protocol() {
        assert_eq!(
            cloudflared_start_args("/tmp/tunnelx/cloudflare/config.yml"),
            vec![
                "tunnel".to_string(),
                "--config".to_string(),
                "/tmp/tunnelx/cloudflare/config.yml".to_string(),
                "run".to_string(),
            ]
        );
    }

    #[test]
    fn watchdog_file_cleanup_actions_are_limited_to_managed_root() {
        let base = std::env::temp_dir().join(format!(
            "tunnelx-cloudflare-cleanup-action-{}",
            uuid::Uuid::new_v4()
        ));
        let root = base.join("cloudflare");
        let managed = root.join("configs").join("api.yml");
        let external = base.join("external.yml");
        std::fs::create_dir_all(managed.parent().unwrap()).unwrap();
        std::fs::write(&managed, b"managed").unwrap();
        std::fs::write(&external, b"external").unwrap();

        assert!(matches!(
            managed_runtime_file_cleanup_action(&managed.to_string_lossy(), &root),
            Some(WatchdogCleanupAction::RemoveFile { .. })
        ));
        assert!(managed_runtime_file_cleanup_action(&external.to_string_lossy(), &root).is_none());

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn http_origin_authority_reads_real_local_service() {
        assert_eq!(
            http_origin_authority("http://127.0.0.1:1420"),
            Some("127.0.0.1:1420")
        );
        assert_eq!(
            http_origin_authority("https://[::1]:8443/path"),
            Some("[::1]:8443")
        );
        assert_eq!(http_origin_authority("tcp://127.0.0.1:22"), None);
    }

    #[test]
    fn start_failure_cleanup_snapshot_queues_created_remote_tunnel() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.tunnel_id = "df38a483-44f2-4b29-a8c8-854f59cc3feb".into();
        tunnel.credentials_file = "/tmp/tunnelx/cloudflare/credentials/api.json".into();
        tunnel.config_file = "/tmp/tunnelx/cloudflare/configs/api.yml".into();
        tunnel.ingress = vec![CloudflareIngressRule {
            hostname: "Api.Example.com.".into(),
            service: "http://127.0.0.1:1420".into(),
            dns_routed: true,
            ..Default::default()
        }];

        let saved =
            start_failure_cleanup_snapshot(&tunnel, true).expect("cleanup should be queued");

        assert!(saved.tunnel_id.is_empty());
        assert!(saved.credentials_file.is_empty());
        assert!(!saved.ingress[0].dns_routed);
        assert_eq!(saved.pending_remote_cleanup.len(), 1);
        assert_eq!(
            saved.pending_remote_cleanup[0].kind,
            CloudflareCleanupKind::RemoteTunnel
        );
        assert_eq!(
            saved.pending_remote_cleanup[0].dns_hostnames,
            vec!["api.example.com"]
        );
    }

    #[test]
    fn start_failure_cleanup_snapshot_queues_only_new_dns_routes() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.tunnel_id = "df38a483-44f2-4b29-a8c8-854f59cc3feb".into();
        tunnel.ingress = vec![
            CloudflareIngressRule {
                hostname: "old.example.com".into(),
                service: "http://127.0.0.1:1420".into(),
                dns_routed: false,
                ..Default::default()
            },
            CloudflareIngressRule {
                hostname: "new.example.com".into(),
                service: "http://127.0.0.1:1421".into(),
                dns_routed: true,
                ..Default::default()
            },
        ];

        let saved =
            start_failure_cleanup_snapshot(&tunnel, false).expect("cleanup should be queued");

        assert_eq!(saved.tunnel_id, tunnel.tunnel_id);
        assert!(!saved.ingress[0].dns_routed);
        assert!(!saved.ingress[1].dns_routed);
        assert_eq!(saved.pending_remote_cleanup.len(), 1);
        assert_eq!(
            saved.pending_remote_cleanup[0].kind,
            CloudflareCleanupKind::DnsRoutes
        );
        assert_eq!(
            saved.pending_remote_cleanup[0].dns_hostnames,
            vec!["new.example.com"]
        );
    }
}
