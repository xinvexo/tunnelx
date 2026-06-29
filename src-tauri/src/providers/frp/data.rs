use crate::error::{AppError, AppResult};
use crate::providers::contract::{normalize_tunnel_name, FRP_PROVIDER_ID};
use crate::providers::frp::domain::{Profile, ProfileProxy, ProfileSummary};
use crate::state::ConfigState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrpSettings {
    #[serde(default)]
    pub active_frpc_version: Option<String>,
    #[serde(default)]
    pub frpc_mirror: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FrpData {
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub settings: FrpSettings,
}

impl ConfigState {
    pub fn frp_data(&self) -> FrpData {
        self.provider_data(FRP_PROVIDER_ID).unwrap_or_default()
    }

    pub fn frp_settings(&self) -> FrpSettings {
        self.frp_data().settings
    }

    pub fn list_summaries(&self) -> Vec<ProfileSummary> {
        self.frp_data()
            .profiles
            .iter()
            .map(ProfileSummary::from)
            .collect()
    }

    pub fn get_profile(&self, id: &str) -> AppResult<Profile> {
        self.frp_data()
            .profiles
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| AppError::TunnelNotFound(id.into()))
    }

    pub fn create_profile(&self, name: String) -> AppResult<Profile> {
        let profile = Profile::new(normalize_tunnel_name(&name)?);
        self.insert_profile(profile.clone())?;
        Ok(profile)
    }

    pub fn insert_profile(&self, profile: Profile) -> AppResult<()> {
        self.update_provider_data::<FrpData, _>(FRP_PROVIDER_ID, |data| {
            data.profiles.push(profile);
            Ok(())
        })
    }

    pub fn update_profile(&self, mut profile: Profile) -> AppResult<Profile> {
        profile.name = normalize_tunnel_name(&profile.name)?;
        self.update_provider_data::<FrpData, _>(FRP_PROVIDER_ID, |data| {
            let slot = data
                .profiles
                .iter_mut()
                .find(|p| p.id == profile.id)
                .ok_or_else(|| AppError::TunnelNotFound(profile.id.clone()))?;
            profile.created_at = slot.created_at;
            profile.touch();
            *slot = profile.clone();
            Ok(profile)
        })
    }

    pub fn delete_profile(&self, id: &str) -> AppResult<()> {
        self.update_provider_data::<FrpData, _>(FRP_PROVIDER_ID, |data| {
            let before = data.profiles.len();
            data.profiles.retain(|p| p.id != id);
            if data.profiles.len() == before {
                return Err(AppError::TunnelNotFound(id.into()));
            }
            Ok(())
        })
    }

    pub fn reorder_profiles(&self, ids: &[String]) -> AppResult<()> {
        self.update_provider_data::<FrpData, _>(FRP_PROVIDER_ID, |data| {
            if ids.len() != data.profiles.len() {
                return Err(AppError::Msg("Connection order is incomplete".into()));
            }

            let mut next = Vec::with_capacity(data.profiles.len());
            for id in ids {
                let Some(index) = data.profiles.iter().position(|p| &p.id == id) else {
                    return Err(AppError::TunnelNotFound(id.clone()));
                };
                next.push(data.profiles.remove(index));
            }
            data.profiles = next;
            Ok(())
        })
    }

    pub fn update_proxies(&self, profile_id: &str, proxies: Vec<ProfileProxy>) -> AppResult<()> {
        self.update_provider_data::<FrpData, _>(FRP_PROVIDER_ID, |data| {
            let p = data
                .profiles
                .iter_mut()
                .find(|p| p.id == profile_id)
                .ok_or_else(|| AppError::TunnelNotFound(profile_id.into()))?;
            p.proxies = proxies;
            p.touch();
            Ok(())
        })
    }

    pub fn set_active_frpc_version(&self, version: Option<String>) -> AppResult<()> {
        self.update_provider_data::<FrpData, _>(FRP_PROVIDER_ID, |data| {
            data.settings.active_frpc_version = version;
            Ok(())
        })
    }

    pub fn set_frpc_mirror(&self, mirror: String) -> AppResult<()> {
        let trimmed = mirror.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("https://") {
            return Err(AppError::Msg("Mirror URL must start with https://".into()));
        }
        self.update_provider_data::<FrpData, _>(FRP_PROVIDER_ID, |data| {
            data.settings.frpc_mirror = (!trimmed.is_empty()).then(|| trimmed.to_string());
            Ok(())
        })
    }
}
