use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::domain::{CloudflareTunnel, CloudflareZone};

pub(crate) fn validate_ingress_zones(tunnel: &CloudflareTunnel) -> AppResult<()> {
    let mut seen = std::collections::HashSet::new();
    let hostnames = tunnel
        .ingress
        .iter()
        .filter(|rule| rule.enabled)
        .map(|rule| normalize_hostname(rule.hostname.trim()))
        .filter(|host| !host.is_empty())
        .filter(|host| seen.insert(host.clone()))
        .collect::<Vec<_>>();
    if hostnames.is_empty() {
        return Ok(());
    }
    let invalid_format = hostnames
        .iter()
        .filter(|host| !is_valid_hostname(host))
        .cloned()
        .collect::<Vec<_>>();
    if !invalid_format.is_empty() {
        return Err(AppError::Msg(format!(
            "Invalid hostname format: {}",
            invalid_format.join(", ")
        )));
    }

    let zones = tauri::async_runtime::block_on(super::api::list_zones_for_tunnel(tunnel)).map_err(
        |error| {
            AppError::Msg(format!(
                "Failed to read zones hosted by the current Cloudflare account: {error}"
            ))
        },
    )?;
    if zones.is_empty() {
        return no_available_zones_error();
    }

    validate_hostnames_belong_to_zones(hostnames, &zones)
}

pub(super) fn normalize_hostname(hostname: &str) -> String {
    hostname.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn validate_hostnames_belong_to_zones(
    hostnames: Vec<String>,
    zones: &[CloudflareZone],
) -> AppResult<()> {
    if zones.is_empty() {
        return no_available_zones_error();
    }
    let invalid = hostnames
        .into_iter()
        .filter(|host| {
            !zones
                .iter()
                .any(|zone| super::api::hostname_matches_zone(host, &zone.name))
        })
        .collect::<Vec<_>>();
    if invalid.is_empty() {
        Ok(())
    } else {
        Err(AppError::Msg(format!(
            "Hostname does not belong to a zone hosted by the current Cloudflare account: {}",
            invalid.join(", ")
        )))
    }
}

fn no_available_zones_error() -> AppResult<()> {
    Err(AppError::Msg(
        "The current Cloudflare account has no available hosted zones; DNS routes cannot be created".into(),
    ))
}

fn is_valid_hostname(hostname: &str) -> bool {
    let host = normalize_hostname(hostname);
    if host.is_empty() || host.len() > 253 || host.contains("..") {
        return false;
    }
    host.split('.').enumerate().all(|(index, part)| {
        if part.is_empty() || part.len() > 63 {
            return false;
        }
        if part == "*" {
            return index == 0;
        }
        let bytes = part.as_bytes();
        bytes.first().is_some_and(u8::is_ascii_alphanumeric)
            && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
            && bytes
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(name: &str) -> CloudflareZone {
        CloudflareZone {
            id: name.into(),
            name: name.into(),
            status: "active".into(),
        }
    }

    #[test]
    fn hostname_validation_rejects_bad_shapes() {
        assert!(is_valid_hostname("api.example.com"));
        assert!(is_valid_hostname("*.example.com"));
        assert!(!is_valid_hostname("-api.example.com"));
        assert!(!is_valid_hostname("api..example.com"));
        assert!(!is_valid_hostname("api.example.com/path"));
    }

    #[test]
    fn hosted_zone_validation_requires_matching_zone() {
        let zones = vec![zone("example.com")];

        assert!(validate_hostnames_belong_to_zones(
            vec!["api.example.com".into(), "*.example.com".into()],
            &zones,
        )
        .is_ok());
        assert!(validate_hostnames_belong_to_zones(vec!["www.google.com".into()], &zones).is_err());
    }

    #[test]
    fn hosted_zone_validation_rejects_empty_zone_list() {
        assert!(validate_hostnames_belong_to_zones(vec!["api.example.com".into()], &[]).is_err());
    }
}
