//! 统一的内部诊断日志（写 stderr，面向开发者/运维）。
//!
//! 全主程序的内部诊断都走这里，保证前缀与格式一致：
//! `[tunnelx] <LEVEL> <scope>: <message>`
//! 例如 `[tunnelx] WARN db: database preflight failed: ...`。
//!
//! 约定：
//! - `scope` 用稳定的小写短标识；provider 相关统一用 `provider:<id>`
//!   （见 [`provider_scope`]），其余如 `app` / `db` / `watchdog`。
//! - 这里只负责运行诊断；面向用户、会显示在隧道日志面板里的内容走
//!   [`crate::services::provider_log`]，两者不要混用。
//! - 看门狗 sidecar 是独立进程，写自己的 stderr，沿用 `[tunnelx-watchdog]`
//!   前缀，不归这里管。

const APP_TAG: &str = "[tunnelx]";

/// 异常但可恢复/可忽略的情况：失败、被忽略的事件、进入降级模式等。
pub(crate) fn warn(scope: &str, message: impl std::fmt::Display) {
    eprintln!("{APP_TAG} WARN {scope}: {message}");
}

/// 正常运行中的关键节点：孤儿进程清理、关停进度等。
pub(crate) fn info(scope: &str, message: impl std::fmt::Display) {
    eprintln!("{APP_TAG} INFO {scope}: {message}");
}

/// provider 维度的 scope 标识，统一为 `provider:<id>`。
pub(crate) fn provider_scope(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_scope_is_prefixed() {
        assert_eq!(provider_scope("ngrok"), "provider:ngrok");
    }
}
