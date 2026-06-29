pub(crate) mod authorization;
pub(crate) mod credentials;

use super::domain::CloudflareData;
use crate::state::AppState;

pub fn data(state: &AppState) -> CloudflareData {
    let mut data = state.config.cloudflare();
    redact_internal_state(&mut data);
    data
}

fn redact_internal_state(data: &mut CloudflareData) {
    for tunnel in &mut data.tunnels {
        tunnel.pending_remote_cleanup.clear();
        if !tunnel.account.api_token.trim().is_empty() {
            tunnel.account.api_token = crate::services::redaction::REDACTED_SECRET.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::providers::cloudflare::domain::{
        CloudflareCleanupKind, CloudflarePendingRemoteCleanup, CloudflareTunnel,
    };

    #[test]
    fn public_data_redacts_pending_remote_cleanup() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.account.api_token = "secret-token".into();
        tunnel
            .pending_remote_cleanup
            .push(CloudflarePendingRemoteCleanup {
                kind: CloudflareCleanupKind::RemoteTunnel,
                tunnel_id: "11111111-1111-4111-8111-111111111111".into(),
                tunnel_name: "api".into(),
                ..Default::default()
            });
        let mut data = CloudflareData {
            tunnels: vec![tunnel],
        };

        redact_internal_state(&mut data);

        assert!(data.tunnels[0].pending_remote_cleanup.is_empty());
        assert_eq!(
            data.tunnels[0].account.api_token,
            crate::services::redaction::REDACTED_SECRET
        );
    }
}
