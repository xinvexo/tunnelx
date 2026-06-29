//! Profile / 隧道的写操作编排：改内存 → 落盘 → emit 事件。
use crate::domain::AppData;
use crate::error::AppResult;
use crate::providers::contract::{ProviderTunnelsUpdatedEvent, FRP_PROVIDER_ID};
use crate::providers::frp::domain::{Profile, ProfileProxy, ProfileSummary};
use crate::state::AppState;
use crate::store::save_app_data;
use std::collections::BTreeMap;
use tauri::{AppHandle, Emitter};

fn emit_profiles_updated(app: &AppHandle, state: &AppState) {
    let _ = app.emit("profiles-updated", state.config.list_summaries());
    let _ = app.emit(
        "provider-tunnels-updated",
        ProviderTunnelsUpdatedEvent {
            provider_id: FRP_PROVIDER_ID.into(),
        },
    );
}

fn save_change<T>(
    app: &AppHandle,
    state: &AppState,
    mutate: impl FnOnce() -> AppResult<T>,
) -> AppResult<T> {
    let before: AppData = state.config.snapshot()?;
    let value = mutate()?;
    if let Err(error) = save_app_data(app, state) {
        state.config.replace(before);
        return Err(error);
    }
    Ok(value)
}

fn proxy_content_by_id(proxies: &[ProfileProxy]) -> AppResult<BTreeMap<String, serde_json::Value>> {
    proxies
        .iter()
        .map(|proxy| Ok((proxy.id.clone(), serde_json::to_value(proxy)?)))
        .collect()
}

/// 检测代理配置是否有运行时相关的变更（需要重启 frpc）。
/// 优化：使用缓存的序列化结果避免重复序列化。
fn proxy_runtime_config_changed(
    before: &[ProfileProxy],
    after: &[ProfileProxy],
) -> AppResult<bool> {
    if before.len() != after.len() {
        return Ok(true);
    }

    // BTreeMap 按 key 排序，所以只要 ID 和内容相同，序列化结果就相同
    let before_map = proxy_content_by_id(before)?;
    let after_map = proxy_content_by_id(after)?;

    // 只序列化一次，然后比较 map
    Ok(before_map != after_map)
}

pub fn list_profiles(state: &AppState) -> Vec<ProfileSummary> {
    state.config.list_summaries()
}

pub fn get_profile(state: &AppState, id: &str) -> AppResult<Profile> {
    state.config.get_profile(id)
}

pub fn create_profile(app: &AppHandle, state: &AppState, name: String) -> AppResult<Profile> {
    let profile = save_change(app, state, || state.config.create_profile(name))?;
    emit_profiles_updated(app, state);
    Ok(profile)
}

pub fn update_profile(app: &AppHandle, state: &AppState, profile: Profile) -> AppResult<Profile> {
    let saved = save_change(app, state, || state.config.update_profile(profile))?;
    super::frpc_service::mark_process_restart_required(app, state, &saved.id);
    let _ = app.emit("profile-updated", &saved);
    emit_profiles_updated(app, state);
    Ok(saved)
}

pub fn delete_profile(app: &AppHandle, state: &AppState, id: &str) -> AppResult<()> {
    super::frpc_service::cleanup_for_delete(app, state, id)?;
    save_change(app, state, || state.config.delete_profile(id))?;
    emit_profiles_updated(app, state);
    Ok(())
}

pub fn reorder_profiles(app: &AppHandle, state: &AppState, ids: Vec<String>) -> AppResult<()> {
    save_change(app, state, || state.config.reorder_profiles(&ids))?;
    emit_profiles_updated(app, state);
    Ok(())
}

pub fn update_proxies(
    app: &AppHandle,
    state: &AppState,
    profile_id: &str,
    proxies: Vec<ProfileProxy>,
) -> AppResult<()> {
    let previous = state.config.get_profile(profile_id)?.proxies;
    let runtime_config_changed = proxy_runtime_config_changed(&previous, &proxies)?;
    save_change(app, state, || {
        state.config.update_proxies(profile_id, proxies)?;
        Ok(())
    })?;
    if runtime_config_changed {
        super::frpc_service::hot_reload(app, state, profile_id);
    }
    if let Ok(p) = state.config.get_profile(profile_id) {
        let _ = app.emit("profile-updated", &p);
    }
    emit_profiles_updated(app, state);
    Ok(())
}
