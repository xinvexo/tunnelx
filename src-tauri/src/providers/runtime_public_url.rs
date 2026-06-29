use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimePublicUrl {
    pub(crate) name: String,
    pub(crate) proto: String,
    pub(crate) public_url: String,
}

impl RuntimePublicUrl {
    pub(crate) fn new(
        name: impl AsRef<str>,
        proto: impl AsRef<str>,
        public_url: impl AsRef<str>,
    ) -> Option<Self> {
        let public_url = public_url.as_ref().trim();
        if public_url.is_empty() {
            return None;
        }
        Some(Self {
            name: name.as_ref().trim().to_string(),
            proto: proto.as_ref().trim().to_string(),
            public_url: public_url.to_string(),
        })
    }
}

#[derive(Debug, Deserialize)]
struct AgentTunnelsResponse {
    #[serde(default)]
    tunnels: Vec<AgentTunnel>,
}

#[derive(Debug, Deserialize)]
struct AgentTunnel {
    #[serde(default)]
    name: String,
    #[serde(default)]
    proto: String,
    #[serde(default)]
    public_url: String,
}

pub(crate) fn allocate_local_agent_addr() -> AppResult<String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    Ok(format!("127.0.0.1:{port}"))
}

pub(crate) fn agent_runtime_details(
    inspect_addr_or_url: &str,
    public_urls: Vec<RuntimePublicUrl>,
) -> Value {
    json!({
        "inspectUrl": normalize_inspect_url(inspect_addr_or_url),
        "publicUrls": public_urls,
    })
}

pub(crate) fn public_url_details(public_urls: Vec<RuntimePublicUrl>) -> Value {
    json!({
        "publicUrls": public_urls,
    })
}

pub(crate) fn public_urls_from_details(details: &Value) -> Vec<RuntimePublicUrl> {
    details
        .get("publicUrls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let raw = item.as_object()?;
            let public_url = raw
                .get("publicUrl")
                .or_else(|| raw.get("public_url"))
                .and_then(Value::as_str)?;
            RuntimePublicUrl::new(
                raw.get("name").and_then(Value::as_str).unwrap_or_default(),
                raw.get("proto").and_then(Value::as_str).unwrap_or_default(),
                public_url,
            )
        })
        .collect()
}

pub(crate) fn inspect_url_from_details(details: &Value) -> Option<String> {
    details
        .get("inspectUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
}

pub(crate) async fn fetch_agent_public_urls(
    api_url: &str,
    timeout: Duration,
) -> AppResult<Vec<RuntimePublicUrl>> {
    let client = reqwest::Client::builder()
        .connect_timeout(timeout)
        .timeout(timeout)
        .build()?;
    let response = client.get(api_url).send().await?.error_for_status()?;
    let body = response.json::<AgentTunnelsResponse>().await?;
    Ok(public_urls_from_agent_response(body))
}

fn normalize_inspect_url(inspect_addr_or_url: &str) -> String {
    let value = inspect_addr_or_url.trim();
    if value.starts_with("http://") || value.starts_with("https://") {
        value.to_string()
    } else {
        format!("http://{value}")
    }
}

fn public_urls_from_agent_response(response: AgentTunnelsResponse) -> Vec<RuntimePublicUrl> {
    response
        .tunnels
        .into_iter()
        .filter_map(|item| RuntimePublicUrl::new(item.name, item.proto, item.public_url))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_response_extracts_public_urls() {
        let response = AgentTunnelsResponse {
            tunnels: vec![
                AgentTunnel {
                    name: "web".into(),
                    proto: "https".into(),
                    public_url: "https://demo.example.com".into(),
                },
                AgentTunnel {
                    name: "pending".into(),
                    proto: "http".into(),
                    public_url: String::new(),
                },
            ],
        };

        let urls = public_urls_from_agent_response(response);

        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].name, "web");
        assert_eq!(urls[0].public_url, "https://demo.example.com");
    }

    #[test]
    fn details_accept_addr_or_url() {
        let from_addr = agent_runtime_details("127.0.0.1:4040", Vec::new());
        let from_url = agent_runtime_details("http://127.0.0.1:4040", Vec::new());

        assert_eq!(
            inspect_url_from_details(&from_addr).as_deref(),
            Some("http://127.0.0.1:4040")
        );
        assert_eq!(
            inspect_url_from_details(&from_url).as_deref(),
            Some("http://127.0.0.1:4040")
        );
    }
}
