use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// 统一的应用错误类型。所有 #[tauri::command] 返回 `Result<T, AppError>`，
/// 序列化成 `{ code, message, data }` 传给前端：code 是稳定标识，前端据此映射
/// i18n 文案（src/api/_invoke.ts），映射不到时回退显示 message。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Msg(String),

    #[error("Connection not found: {0}")]
    TunnelNotFound(String),

    #[error("frpc is not ready: install and activate a runtime version first")]
    FrpcNotReady,

    #[error("This connection is already running")]
    AlreadyRunning,

    #[error("Watchdog communication failed, please retry")]
    WatchdogCommunicationFailed,

    #[error("Port {0} is already in use")]
    PortInUse(u16),

    #[error("The active version cannot be deleted. Activate another version first")]
    ActiveVersionInUse,

    #[error("Stop running frp connections first")]
    FrpcRuntimeInUse,

    #[error("Invalid config file format: {0}")]
    ConfigFormatError(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Sql(#[from] sqlx::Error),

    #[error(transparent)]
    SqlPlugin(#[from] tauri_plugin_sql::Error),

    #[error("Download failed: {0}")]
    Http(String),
}

impl From<reqwest::Error> for AppError {
    fn from(e: reqwest::Error) -> Self {
        AppError::Http(e.to_string())
    }
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Msg(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Msg(s.to_string())
    }
}

impl AppError {
    /// 跨语言稳定的错误码：前端用它查 `backendErrors.<code>` 的 i18n 文案。
    fn code(&self) -> &'static str {
        match self {
            AppError::Msg(_) => "msg",
            AppError::TunnelNotFound(_) => "tunnel_not_found",
            AppError::FrpcNotReady => "frpc_not_ready",
            AppError::AlreadyRunning => "already_running",
            AppError::WatchdogCommunicationFailed => "watchdog_communication_failed",
            AppError::PortInUse(_) => "port_in_use",
            AppError::ActiveVersionInUse => "active_version_in_use",
            AppError::FrpcRuntimeInUse => "frpc_runtime_in_use",
            AppError::ConfigFormatError(_) => "config_format_error",
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::Sql(_) => "sql",
            AppError::SqlPlugin(_) => "sql_plugin",
            AppError::Http(_) => "http",
        }
    }

    /// 错误码对应文案的插值参数（i18n 模板里的 {port} 等）。
    fn data(&self) -> serde_json::Value {
        match self {
            AppError::TunnelNotFound(id) => serde_json::json!({ "id": id }),
            AppError::PortInUse(port) => serde_json::json!({ "port": port }),
            AppError::ConfigFormatError(detail) => serde_json::json!({ "detail": detail }),
            _ => serde_json::Value::Null,
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.serialize_field("data", &self.data())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
