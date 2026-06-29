use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub silent_start: bool,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default)]
    pub lightweight_mode: bool,
    #[serde(default)]
    pub auto_update: bool,
    #[serde(default = "default_true")]
    pub traffic_stats_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: None,
            silent_start: false,
            auto_connect: false,
            lightweight_mode: false,
            auto_update: false,
            traffic_stats_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    #[serde(default)]
    pub connection_order: Vec<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, Value>,
    #[serde(default)]
    pub settings: AppSettings,
}
