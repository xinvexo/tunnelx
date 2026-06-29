use crate::providers::cloudflare::domain::CloudflareTunnel;
use std::path::Path;

pub(super) fn push_unique_nonempty(files: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if !path.is_empty() && !files.iter().any(|item| item == path) {
        files.push(path.to_string());
    }
}

pub(super) fn remove_managed_file(root: &Path, path: &str) {
    let path = path.trim();
    if !path.is_empty() {
        crate::paths::remove_file_if_under(Path::new(path), root);
    }
}

pub(super) fn remove_stale_files(files: Vec<String>, tunnel: &CloudflareTunnel, root: &Path) {
    for path in files {
        if is_current_tunnel_file(&path, tunnel) {
            continue;
        }
        remove_managed_file(root, &path);
    }
}

pub(super) fn is_current_tunnel_file(path: &str, tunnel: &CloudflareTunnel) -> bool {
    let path = path.trim();
    if path.is_empty() {
        return false;
    }
    [
        tunnel.config_file.as_str(),
        tunnel.credentials_file.as_str(),
        tunnel.cert_file.as_str(),
    ]
    .into_iter()
    .any(|current| !current.trim().is_empty() && current.trim() == path)
        || tunnel.pending_remote_cleanup.iter().any(|target| {
            [
                target.config_file.as_str(),
                target.credentials_file.as_str(),
                target.cert_file.as_str(),
            ]
            .into_iter()
            .any(|current| !current.trim().is_empty() && current.trim() == path)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::providers::cloudflare::domain::{
        CloudflareCleanupKind, CloudflarePendingRemoteCleanup,
    };

    #[test]
    fn current_tunnel_files_are_protected_from_stale_cleanup() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.config_file = "/tmp/tunnelx/cloudflare/configs/api.yml".into();
        tunnel.credentials_file = "/tmp/tunnelx/cloudflare/credentials/api.json".into();
        tunnel.cert_file = "/tmp/tunnelx/cloudflare/certs/api.pem".into();

        assert!(is_current_tunnel_file(
            "/tmp/tunnelx/cloudflare/configs/api.yml",
            &tunnel
        ));
        assert!(is_current_tunnel_file(
            "/tmp/tunnelx/cloudflare/credentials/api.json",
            &tunnel
        ));
        assert!(is_current_tunnel_file(
            "/tmp/tunnelx/cloudflare/certs/api.pem",
            &tunnel
        ));
        assert!(!is_current_tunnel_file(
            "/tmp/tunnelx/cloudflare/configs/unused.yml",
            &tunnel
        ));
    }

    #[test]
    fn pending_cleanup_files_are_protected_from_stale_cleanup() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel
            .pending_remote_cleanup
            .push(CloudflarePendingRemoteCleanup {
                kind: CloudflareCleanupKind::RemoteTunnel,
                tunnel_id: "11111111-1111-4111-8111-111111111111".into(),
                tunnel_name: "api".into(),
                credentials_file: "/tmp/tunnelx/cloudflare/credentials/pending.json".into(),
                config_file: "/tmp/tunnelx/cloudflare/configs/pending.yml".into(),
                cert_file: "/tmp/tunnelx/cloudflare/certs/pending.pem".into(),
                ..Default::default()
            });

        assert!(is_current_tunnel_file(
            "/tmp/tunnelx/cloudflare/credentials/pending.json",
            &tunnel
        ));
        assert!(is_current_tunnel_file(
            "/tmp/tunnelx/cloudflare/configs/pending.yml",
            &tunnel
        ));
        assert!(is_current_tunnel_file(
            "/tmp/tunnelx/cloudflare/certs/pending.pem",
            &tunnel
        ));
    }
}
