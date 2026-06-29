pub(crate) mod persistence;

use crate::domain::now_ms;
use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::domain::{CloudflareData, CloudflareTunnel};
use crate::providers::contract::CLOUDFLARE_PROVIDER_ID;
use crate::state::ConfigState;

impl ConfigState {
    pub fn cloudflare(&self) -> CloudflareData {
        self.provider_data(CLOUDFLARE_PROVIDER_ID)
            .unwrap_or_default()
    }

    pub fn upsert_cloudflare_tunnel(
        &self,
        mut tunnel: CloudflareTunnel,
    ) -> AppResult<CloudflareTunnel> {
        self.update_provider_data::<CloudflareData, _>(CLOUDFLARE_PROVIDER_ID, |data| {
            if tunnel.created_at == 0 {
                tunnel.created_at = now_ms();
            }
            tunnel.touch();
            if let Some(slot) = data.tunnels.iter_mut().find(|item| item.id == tunnel.id) {
                tunnel.created_at = slot.created_at;
                *slot = tunnel.clone();
            } else {
                data.tunnels.push(tunnel.clone());
            }
            Ok(tunnel)
        })
    }

    pub fn delete_cloudflare_tunnel(&self, id: &str) -> AppResult<CloudflareTunnel> {
        self.update_provider_data::<CloudflareData, _>(CLOUDFLARE_PROVIDER_ID, |data| {
            let Some(index) = data.tunnels.iter().position(|item| item.id == id) else {
                return Err(AppError::Msg("Cloudflare tunnel not found".into()));
            };
            Ok(data.tunnels.remove(index))
        })
    }
}
