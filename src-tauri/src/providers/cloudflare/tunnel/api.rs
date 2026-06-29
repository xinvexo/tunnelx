use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::account::credentials::{read_origin_cert, OriginCertInfo};
use crate::providers::cloudflare::domain::{
    CloudflareAccount, CloudflareAccountConfig, CloudflareAuthMode, CloudflareTunnel,
    CloudflareZone,
};
use crate::state::AppState;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tunnelx_watchdog_protocol::{
    WatchdogCleanupAction, WatchdogHttpHeader, WatchdogHttpRequest, WatchdogJsonExpectation,
};
use uuid::Uuid;

const CF_API: &str = "https://api.cloudflare.com/client/v4";
const API_TIMEOUT: Duration = Duration::from_secs(25);
const API_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn api_client() -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(API_TIMEOUT)
        .connect_timeout(API_CONNECT_TIMEOUT)
        .build()
        .map_err(AppError::from)
}

#[derive(Debug, Deserialize)]
struct ApiEnvelope<T> {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    result: Option<T>,
    #[serde(default)]
    errors: Vec<ApiError>,
    #[serde(default)]
    result_info: Option<ResultInfo>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    code: Option<u32>,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ResultInfo {
    #[serde(default)]
    page: u32,
    #[serde(default)]
    total_pages: u32,
}

fn api_error<T>(envelope: ApiEnvelope<T>) -> AppError {
    let message = envelope
        .errors
        .into_iter()
        .map(|item| item.message)
        .collect::<Vec<_>>()
        .join("; ");
    AppError::Msg(if message.is_empty() {
        "Cloudflare API request failed".into()
    } else {
        message
    })
}

fn api_not_found<T>(envelope: &ApiEnvelope<T>) -> bool {
    envelope.errors.iter().any(|item| {
        let message = item.message.to_ascii_lowercase();
        item.code == Some(1002)
            || item.code == Some(1003)
            || item.code == Some(7003)
            || message.contains("not found")
            || message.contains("does not exist")
    })
}

#[derive(Debug, Clone)]
struct CloudflareApiAuth {
    token: String,
    account_id: String,
    zone_id: String,
}

fn tunnel_from_state(state: &AppState, tunnel_id: &str) -> AppResult<CloudflareTunnel> {
    state
        .config
        .cloudflare()
        .tunnels
        .into_iter()
        .find(|item| item.id == tunnel_id)
        .ok_or_else(|| AppError::Msg("Cloudflare connection not found".into()))
}

fn api_token(account: &CloudflareAccountConfig) -> AppResult<String> {
    let token = account.api_token.trim().to_string();
    if token.is_empty() {
        Err(AppError::Msg(
            "Configure the Cloudflare API Token first".into(),
        ))
    } else {
        Ok(token)
    }
}

fn account_id(account: &CloudflareAccountConfig) -> AppResult<String> {
    let id = account.token_account_id.trim().to_string();
    if id.is_empty() {
        Err(AppError::Msg(
            "Configure the Cloudflare Account ID first".into(),
        ))
    } else {
        Ok(id)
    }
}

fn manual_api_auth(account: &CloudflareAccountConfig) -> AppResult<CloudflareApiAuth> {
    Ok(CloudflareApiAuth {
        token: api_token(account)?,
        account_id: account_id(account)?,
        zone_id: String::new(),
    })
}

fn cert_auth(cert: OriginCertInfo) -> CloudflareApiAuth {
    CloudflareApiAuth {
        token: cert.api_token.trim().to_string(),
        account_id: cert.account_id.trim().to_string(),
        zone_id: cert.zone_id.trim().to_string(),
    }
}

fn api_auth_for_tunnel(tunnel: &CloudflareTunnel) -> AppResult<CloudflareApiAuth> {
    match tunnel.account.auth_mode {
        CloudflareAuthMode::Authorization => Ok(cert_auth(read_origin_cert(&tunnel.cert_file)?)),
        CloudflareAuthMode::ApiToken => manual_api_auth(&tunnel.account),
    }
}

pub async fn verify_token(state: AppState, tunnel_id: String) -> AppResult<bool> {
    let account = tunnel_from_state(&state, &tunnel_id)?.account;
    verify_api_token_account(&account).await.map(|_| true)
}

pub async fn verify_token_for_account(account: &CloudflareAccountConfig) -> AppResult<bool> {
    let token = api_token(account)?;
    let client = api_client()?;
    let envelope = client
        .get(format!("{CF_API}/user/tokens/verify"))
        .bearer_auth(token)
        .send()
        .await?
        .json::<ApiEnvelope<serde_json::Value>>()
        .await?;
    if envelope.success {
        Ok(true)
    } else {
        Err(api_error(envelope))
    }
}

pub async fn verify_api_token_account(
    account: &CloudflareAccountConfig,
) -> AppResult<Vec<CloudflareZone>> {
    verify_token_for_account(account).await?;
    let auth = manual_api_auth(account)?;
    list_zones_with_auth(&auth).await
}

#[derive(Debug, Default, Deserialize)]
struct ZoneResult {
    id: String,
    name: String,
    #[serde(default)]
    status: String,
}

pub fn hostname_matches_zone(hostname: &str, zone: &str) -> bool {
    let host = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    let zone = zone.trim().trim_end_matches('.').to_ascii_lowercase();
    !host.is_empty() && !zone.is_empty() && (host == zone || host.ends_with(&format!(".{zone}")))
}

pub async fn list_zones(state: AppState, tunnel_id: String) -> AppResult<Vec<CloudflareZone>> {
    let tunnel = tunnel_from_state(&state, &tunnel_id)?;
    list_zones_for_tunnel(&tunnel).await
}

pub async fn list_zones_for_tunnel(tunnel: &CloudflareTunnel) -> AppResult<Vec<CloudflareZone>> {
    let auth = api_auth_for_tunnel(tunnel)?;
    list_zones_with_auth(&auth).await
}

async fn zone_by_id(auth: &CloudflareApiAuth) -> AppResult<Option<CloudflareZone>> {
    if auth.zone_id.trim().is_empty() {
        return Ok(None);
    }
    let client = api_client()?;
    let envelope = client
        .get(format!("{CF_API}/zones/{}", auth.zone_id))
        .bearer_auth(&auth.token)
        .send()
        .await?
        .json::<ApiEnvelope<ZoneResult>>()
        .await?;
    if envelope.success {
        Ok(envelope.result.map(|item| CloudflareZone {
            id: item.id,
            name: item.name,
            status: item.status,
        }))
    } else if api_not_found(&envelope) {
        Ok(None)
    } else {
        Err(api_error(envelope))
    }
}

async fn list_zones_with_auth(auth: &CloudflareApiAuth) -> AppResult<Vec<CloudflareZone>> {
    let client = api_client()?;
    let mut zones = Vec::new();
    let mut page = 1_u32;

    loop {
        let page_text = page.to_string();
        let envelope = client
            .get(format!("{CF_API}/zones"))
            .bearer_auth(&auth.token)
            .query(&[
                ("account.id", auth.account_id.as_str()),
                ("status", "active"),
                ("page", page_text.as_str()),
                ("per_page", "100"),
            ])
            .send()
            .await?
            .json::<ApiEnvelope<Vec<ZoneResult>>>()
            .await?;
        if !envelope.success {
            let error = api_error(envelope);
            if let Ok(Some(zone)) = zone_by_id(auth).await {
                return Ok(vec![zone]);
            }
            return Err(error);
        }

        zones.extend(
            envelope
                .result
                .unwrap_or_default()
                .into_iter()
                .map(|item| CloudflareZone {
                    id: item.id,
                    name: item.name,
                    status: item.status,
                }),
        );

        let Some(info) = envelope.result_info else {
            break;
        };
        if info.total_pages == 0 || info.page >= info.total_pages {
            break;
        }
        page += 1;
    }

    if zones.is_empty() {
        if let Some(zone) = zone_by_id(auth).await? {
            zones.push(zone);
        }
    }

    zones.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(zones)
}

#[derive(Debug, Deserialize)]
struct AccountResult {
    id: String,
    name: String,
}

pub async fn list_accounts(
    state: AppState,
    tunnel_id: String,
) -> AppResult<Vec<CloudflareAccount>> {
    let tunnel = tunnel_from_state(&state, &tunnel_id)?;
    let auth = api_auth_for_tunnel(&tunnel)?;
    let client = api_client()?;
    let envelope = client
        .get(format!("{CF_API}/accounts"))
        .bearer_auth(auth.token)
        .send()
        .await?
        .json::<ApiEnvelope<Vec<AccountResult>>>()
        .await?;
    if !envelope.success {
        return Err(api_error(envelope));
    }
    Ok(envelope
        .result
        .unwrap_or_default()
        .into_iter()
        .map(|item| CloudflareAccount {
            id: item.id,
            name: item.name,
        })
        .collect())
}

#[derive(Debug, Default, Deserialize)]
struct RemoteTunnelResult {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct CreateTunnelRequest {
    name: String,
    tunnel_secret: String,
}

pub async fn list_remote_tunnels(
    state: AppState,
    tunnel_id: String,
) -> AppResult<Vec<CloudflareTunnel>> {
    let tunnel = tunnel_from_state(&state, &tunnel_id)?;
    list_remote_tunnels_for_tunnel(&tunnel).await
}

pub async fn list_remote_tunnels_for_tunnel(
    tunnel: &CloudflareTunnel,
) -> AppResult<Vec<CloudflareTunnel>> {
    let auth = api_auth_for_tunnel(tunnel)?;
    list_remote_tunnels_with_auth(&auth, &tunnel.account).await
}

async fn list_remote_tunnels_with_auth(
    auth: &CloudflareApiAuth,
    account: &CloudflareAccountConfig,
) -> AppResult<Vec<CloudflareTunnel>> {
    let client = api_client()?;
    let envelope = client
        .get(format!("{CF_API}/accounts/{}/cfd_tunnel", auth.account_id))
        .bearer_auth(&auth.token)
        .query(&[("is_deleted", "false")])
        .send()
        .await?
        .json::<ApiEnvelope<Vec<RemoteTunnelResult>>>()
        .await?;
    if !envelope.success {
        return Err(api_error(envelope));
    }
    Ok(envelope
        .result
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            let mut tunnel = CloudflareTunnel::new(item.name);
            tunnel.account = account.to_owned();
            tunnel.tunnel_id = item.id;
            tunnel
        })
        .collect())
}

pub async fn create_remote_tunnel_for_tunnel(
    tunnel: &CloudflareTunnel,
    tunnel_secret: &[u8],
) -> AppResult<String> {
    let auth = api_auth_for_tunnel(tunnel)?;
    let envelope = api_client()?
        .post(format!("{CF_API}/accounts/{}/cfd_tunnel", auth.account_id))
        .bearer_auth(&auth.token)
        .json(&CreateTunnelRequest {
            name: tunnel.name.trim().to_string(),
            tunnel_secret: STANDARD.encode(tunnel_secret),
        })
        .send()
        .await?
        .json::<ApiEnvelope<RemoteTunnelResult>>()
        .await?;
    if envelope.success {
        envelope
            .result
            .map(|item| item.id)
            .ok_or_else(|| AppError::Msg("Cloudflare API did not return a tunnel ID".into()))
    } else {
        Err(api_error(envelope))
    }
}

pub async fn remote_tunnel_id_for_tunnel(
    tunnel: &CloudflareTunnel,
    name: &str,
) -> AppResult<Option<String>> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    Ok(list_remote_tunnels_for_tunnel(tunnel)
        .await?
        .into_iter()
        .find(|item| item.name == name)
        .map(|item| item.tunnel_id))
}

pub async fn delete_remote_tunnel_for_tunnel(
    tunnel: &CloudflareTunnel,
    tunnel_id: &str,
) -> AppResult<()> {
    let auth = api_auth_for_tunnel(tunnel)?;
    delete_remote_tunnel_with_auth(&auth, tunnel_id).await
}

async fn delete_remote_tunnel_with_auth(
    auth: &CloudflareApiAuth,
    tunnel_id: &str,
) -> AppResult<()> {
    let tunnel_id = tunnel_id.trim();
    if tunnel_id.is_empty() {
        return Ok(());
    }
    let client = api_client()?;
    let envelope = client
        .delete(format!(
            "{CF_API}/accounts/{}/cfd_tunnel/{tunnel_id}",
            auth.account_id
        ))
        .bearer_auth(&auth.token)
        .query(&[("cascade", "true")])
        .send()
        .await?
        .json::<ApiEnvelope<serde_json::Value>>()
        .await?;
    if envelope.success || api_not_found(&envelope) {
        Ok(())
    } else {
        Err(api_error(envelope))
    }
}

fn is_valid_cloudflare_tunnel_id(tunnel_id: &str) -> bool {
    Uuid::parse_str(tunnel_id.trim()).is_ok()
}

fn configured_remote_tunnel_id(tunnel: &CloudflareTunnel) -> Option<String> {
    let tunnel_id = tunnel.tunnel_id.trim();
    if !tunnel_id.is_empty() && tunnel_id != tunnel.id && is_valid_cloudflare_tunnel_id(tunnel_id) {
        return Some(tunnel_id.to_string());
    }
    let credentials_file = tunnel.credentials_file.trim();
    if credentials_file.is_empty() {
        return None;
    }
    crate::providers::cloudflare::account::credentials::credentials_tunnel_id(Path::new(
        credentials_file,
    ))
    .filter(|id| is_valid_cloudflare_tunnel_id(id))
}

fn has_remote_cleanup_evidence(tunnel: &CloudflareTunnel) -> bool {
    configured_remote_tunnel_id(tunnel).is_some()
        || !tunnel.credentials_file.trim().is_empty()
        || tunnel.ingress.iter().any(|rule| rule.dns_routed)
}

async fn remote_cleanup_tunnel_id(tunnel: &CloudflareTunnel) -> AppResult<Option<String>> {
    if let Some(tunnel_id) = configured_remote_tunnel_id(tunnel) {
        return Ok(Some(tunnel_id));
    }
    if has_remote_cleanup_evidence(tunnel) {
        remote_tunnel_id_for_tunnel(tunnel, &tunnel.name).await
    } else {
        Ok(None)
    }
}

pub async fn remote_cleanup_actions_for_tunnel(
    tunnel: &CloudflareTunnel,
) -> AppResult<Vec<WatchdogCleanupAction>> {
    let Some(tunnel_id) = remote_cleanup_tunnel_id(tunnel).await? else {
        return Ok(Vec::new());
    };
    let auth = api_auth_for_tunnel(tunnel)?;
    let mut cleanup_target = tunnel.clone();
    cleanup_target.tunnel_id = tunnel_id.clone();
    let mut actions = remote_dns_cleanup_actions(&auth, &cleanup_target).await?;
    actions.push(WatchdogCleanupAction::Http {
        request: Box::new(cloudflare_delete_request(
            format!(
                "{CF_API}/accounts/{}/cfd_tunnel/{tunnel_id}?cascade=true",
                auth.account_id
            ),
            &auth.token,
        )),
    });
    Ok(actions)
}

#[derive(Debug, Serialize)]
struct DnsRouteRequest {
    #[serde(rename = "type")]
    route_type: &'static str,
    user_hostname: String,
    overwrite_existing: bool,
}

pub async fn route_dns_for_tunnel(
    tunnel: &CloudflareTunnel,
    tunnel_id: &str,
    hostname: &str,
) -> AppResult<()> {
    let tunnel_id = tunnel_id.trim();
    let hostname = hostname.trim();
    if tunnel_id.is_empty() || hostname.is_empty() {
        return Ok(());
    }
    let auth = api_auth_for_tunnel(tunnel)?;
    let zones = list_zones_with_auth(&auth).await?;
    let zone = best_zone_for_hostname(&zones, hostname).ok_or_else(|| {
        AppError::Msg(format!(
            "Hostname does not belong to a zone hosted by the current Cloudflare account: {hostname}"
        ))
    })?;
    let envelope = api_client()?
        .put(format!(
            "{CF_API}/zones/{}/tunnels/{}/routes",
            zone.id, tunnel_id
        ))
        .bearer_auth(&auth.token)
        .json(&DnsRouteRequest {
            route_type: "dns",
            user_hostname: hostname.to_string(),
            overwrite_existing: true,
        })
        .send()
        .await?
        .json::<ApiEnvelope<serde_json::Value>>()
        .await?;
    if envelope.success {
        Ok(())
    } else {
        Err(api_error(envelope))
    }
}

#[derive(Debug, Deserialize)]
struct DnsRecordResult {
    id: String,
    #[serde(default)]
    content: String,
}

fn normalize_dns_value(value: &str) -> String {
    value.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn tunnel_dns_target(tunnel_id: &str) -> String {
    format!("{}.cfargotunnel.com", normalize_dns_value(tunnel_id))
}

fn best_zone_for_hostname<'a>(
    zones: &'a [CloudflareZone],
    hostname: &str,
) -> Option<&'a CloudflareZone> {
    zones
        .iter()
        .filter(|zone| hostname_matches_zone(hostname, &zone.name))
        .max_by_key(|zone| zone.name.len())
}

async fn list_dns_records(
    client: &reqwest::Client,
    token: &str,
    zone_id: &str,
    hostname: &str,
) -> AppResult<Vec<DnsRecordResult>> {
    let mut records = Vec::new();
    let mut page = 1_u32;
    loop {
        let page_text = page.to_string();
        let envelope = client
            .get(format!("{CF_API}/zones/{zone_id}/dns_records"))
            .bearer_auth(token)
            .query(&[
                ("type", "CNAME"),
                ("name", hostname),
                ("page", page_text.as_str()),
                ("per_page", "100"),
            ])
            .send()
            .await?
            .json::<ApiEnvelope<Vec<DnsRecordResult>>>()
            .await?;
        if !envelope.success {
            return Err(api_error(envelope));
        }
        records.extend(envelope.result.unwrap_or_default());
        let Some(info) = envelope.result_info else {
            break;
        };
        if info.total_pages == 0 || info.page >= info.total_pages {
            break;
        }
        page += 1;
    }
    Ok(records)
}

fn dns_records_list_url(zone_id: &str, hostname: &str) -> AppResult<String> {
    let mut url =
        reqwest::Url::parse(&format!("{CF_API}/zones/{zone_id}/dns_records")).map_err(|error| {
            AppError::Msg(format!("Failed to build Cloudflare DNS query URL: {error}"))
        })?;
    url.query_pairs_mut()
        .append_pair("type", "CNAME")
        .append_pair("name", hostname)
        .append_pair("page", "1")
        .append_pair("per_page", "100");
    Ok(url.to_string())
}

async fn delete_dns_record(
    client: &reqwest::Client,
    token: &str,
    zone_id: &str,
    record_id: &str,
) -> AppResult<()> {
    let envelope = client
        .delete(format!("{CF_API}/zones/{zone_id}/dns_records/{record_id}"))
        .bearer_auth(token)
        .send()
        .await?
        .json::<ApiEnvelope<serde_json::Value>>()
        .await?;
    if envelope.success || api_not_found(&envelope) {
        Ok(())
    } else {
        Err(api_error(envelope))
    }
}

pub async fn delete_dns_routes_for_tunnel(
    tunnel: &CloudflareTunnel,
    tunnel_id: &str,
    hostnames: &[String],
) -> AppResult<Vec<String>> {
    let auth = api_auth_for_tunnel(tunnel)?;
    delete_dns_routes_with_auth(&auth, tunnel_id, hostnames).await
}

async fn delete_dns_routes_with_auth(
    auth: &CloudflareApiAuth,
    tunnel_id: &str,
    hostnames: &[String],
) -> AppResult<Vec<String>> {
    let tunnel_id = tunnel_id.trim();
    if tunnel_id.is_empty() || hostnames.is_empty() {
        return Ok(Vec::new());
    }
    let zones = list_zones_with_auth(auth).await?;
    let client = api_client()?;
    let target = tunnel_dns_target(tunnel_id);
    let mut deleted = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for hostname in hostnames
        .iter()
        .map(|host| host.trim())
        .filter(|host| !host.is_empty())
    {
        let hostname_key = normalize_dns_value(hostname);
        if !seen.insert(hostname_key.clone()) {
            continue;
        }
        let zone = best_zone_for_hostname(&zones, &hostname_key).ok_or_else(|| {
            AppError::Msg(format!(
                "Hostname does not belong to a zone hosted by the current Cloudflare account; cannot clean up DNS: {hostname}"
            ))
        })?;
        let records = list_dns_records(&client, &auth.token, &zone.id, &hostname_key).await?;
        let mut deleted_any = false;
        for record in records
            .into_iter()
            .filter(|record| normalize_dns_value(&record.content) == target)
        {
            delete_dns_record(&client, &auth.token, &zone.id, &record.id).await?;
            deleted_any = true;
        }
        if deleted_any {
            deleted.push(hostname_key);
        }
    }
    Ok(deleted)
}

async fn remote_dns_cleanup_actions(
    auth: &CloudflareApiAuth,
    tunnel: &CloudflareTunnel,
) -> AppResult<Vec<WatchdogCleanupAction>> {
    let tunnel_id = tunnel.tunnel_id.trim();
    if tunnel_id.is_empty() {
        return Ok(Vec::new());
    }
    let zones = list_zones_with_auth(auth).await?;
    let target = tunnel_dns_target(tunnel_id);
    let mut actions = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for hostname in tunnel
        .ingress
        .iter()
        .map(|rule| rule.hostname.trim())
        .filter(|hostname| !hostname.is_empty())
    {
        let hostname_key = normalize_dns_value(hostname);
        if !seen.insert(hostname_key.clone()) {
            continue;
        }
        let zone = best_zone_for_hostname(&zones, &hostname_key).ok_or_else(|| {
            AppError::Msg(format!(
                "Hostname does not belong to a zone hosted by the current Cloudflare account; cannot build DNS cleanup plan: {hostname}"
            ))
        })?;
        actions.push(WatchdogCleanupAction::HttpJsonDeleteMatches {
            list_request: Box::new(cloudflare_get_request(
                dns_records_list_url(&zone.id, &hostname_key)?,
                &auth.token,
            )),
            items_field: "result".into(),
            id_field: "id".into(),
            match_field: "content".into(),
            match_value: target.clone(),
            match_ignore_ascii_case: true,
            match_trim_trailing_dot: true,
            delete_request: Box::new(cloudflare_delete_request(
                format!("{CF_API}/zones/{}/dns_records/{{id}}", zone.id),
                &auth.token,
            )),
            id_placeholder: "{id}".into(),
        });
    }

    Ok(actions)
}

fn cloudflare_get_request(url: String, token: &str) -> WatchdogHttpRequest {
    WatchdogHttpRequest {
        method: "GET".into(),
        url,
        headers: cloudflare_auth_headers(token),
        body: None,
        accepted_statuses: vec![200],
        json_expectation: Some(cloudflare_success_json_expectation()),
        timeout_ms: Some(20_000),
    }
}

fn cloudflare_delete_request(url: String, token: &str) -> WatchdogHttpRequest {
    WatchdogHttpRequest {
        method: "DELETE".into(),
        url,
        headers: cloudflare_auth_headers(token),
        body: None,
        accepted_statuses: vec![200, 404],
        json_expectation: Some(cloudflare_json_expectation()),
        timeout_ms: Some(20_000),
    }
}

fn cloudflare_auth_headers(token: &str) -> Vec<WatchdogHttpHeader> {
    vec![WatchdogHttpHeader {
        name: "Authorization".into(),
        value: format!("Bearer {token}"),
    }]
}

fn cloudflare_success_json_expectation() -> WatchdogJsonExpectation {
    WatchdogJsonExpectation {
        success_field: "success".into(),
        errors_field: "errors".into(),
        error_code_field: "code".into(),
        error_message_field: "message".into(),
        ignored_error_codes: Vec::new(),
        ignored_error_message_contains: Vec::new(),
    }
}

fn cloudflare_json_expectation() -> WatchdogJsonExpectation {
    WatchdogJsonExpectation {
        success_field: "success".into(),
        errors_field: "errors".into(),
        error_code_field: "code".into(),
        error_message_field: "message".into(),
        ignored_error_codes: vec![1002, 1003, 7003],
        ignored_error_message_contains: vec![
            "not found".into(),
            "does not exist".into(),
            "does not exist or has been deleted".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    fn temp_test_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tunnelx-cloudflare-api-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn best_zone_uses_longest_matching_zone() {
        let zones = vec![
            CloudflareZone {
                id: "root".into(),
                name: "example.com".into(),
                status: "active".into(),
            },
            CloudflareZone {
                id: "sub".into(),
                name: "internal.example.com".into(),
                status: "active".into(),
            },
        ];

        let zone = best_zone_for_hostname(&zones, "api.internal.example.com").unwrap();
        assert_eq!(zone.id, "sub");
    }

    #[test]
    fn tunnel_dns_target_normalizes_input() {
        assert_eq!(
            tunnel_dns_target(" ABCD. "),
            "abcd.cfargotunnel.com".to_string()
        );
    }

    #[test]
    fn dns_records_list_url_preserves_hostname_query_value() {
        let url = dns_records_list_url("zone-1", "*.example.com").unwrap();
        let parsed = reqwest::Url::parse(&url).unwrap();
        let pairs = parsed.query_pairs().collect::<Vec<_>>();

        assert!(pairs
            .iter()
            .any(|(key, value)| key == "type" && value == "CNAME"));
        assert!(pairs
            .iter()
            .any(|(key, value)| key == "name" && value == "*.example.com"));
    }

    #[test]
    fn cloudflare_cleanup_expectation_accepts_missing_tunnel_code() {
        let expectation = cloudflare_json_expectation();

        assert!(expectation.ignored_error_codes.contains(&1002));
        assert_eq!(expectation.success_field, "success");
    }

    #[test]
    fn cleanup_tunnel_id_does_not_use_platform_connection_id() {
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.tunnel_id = tunnel.id.clone();

        assert!(configured_remote_tunnel_id(&tunnel).is_none());
    }

    #[test]
    fn cleanup_tunnel_id_can_use_credentials_file() {
        let dir = temp_test_dir();
        let credentials_file = dir.join("credentials.json");
        fs::write(
            &credentials_file,
            r#"{"TunnelID":"11111111-1111-4111-8111-111111111111"}"#,
        )
        .unwrap();
        let mut tunnel = CloudflareTunnel::new("api");
        tunnel.credentials_file = credentials_file.to_string_lossy().to_string();

        assert_eq!(
            configured_remote_tunnel_id(&tunnel).as_deref(),
            Some("11111111-1111-4111-8111-111111111111")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn api_not_found_accepts_missing_tunnel_code() {
        let envelope = ApiEnvelope::<serde_json::Value> {
            success: false,
            result: None,
            errors: vec![ApiError {
                code: Some(1002),
                message: "missing".into(),
            }],
            result_info: None,
        };

        assert!(api_not_found(&envelope));
    }
}
