//! frp provider 的本地路径约定。
//!
//! `<app_data>/frpc/<version>/frpc[.exe]` —— 各版本 frpc 可执行文件
//! `<app_data>/runtime/<profile_id>.json` —— 运行时生成的 frpc 配置
use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use tauri::{AppHandle, Runtime};

pub fn frpc_root<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(crate::paths::data_dir(app)?.join("frpc"))
}

pub(crate) fn sanitize_filename(name: &str) -> String {
    let cleaned = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect::<String>();
    let trimmed = cleaned.trim_matches(['.', '-']).to_string();
    if trimmed.is_empty() {
        "frp".into()
    } else {
        trimmed
    }
}

/// 创建新版本目录时的严格校验：只许 [A-Za-z0-9._-]，杜绝路径遍历与怪异目录名。
/// 用于安装等「新建」入口（版本号来自前端命令参数）。
pub fn validate_version(version: &str) -> AppResult<()> {
    if version.is_empty() || version.len() > 64 {
        return Err(AppError::Msg("Invalid version".into()));
    }
    let ok = version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    // 必须含至少一个字母或数字：否则 "."、".." 之外的 "..."、"-" 等纯符号名仍可能
    // 指向当前目录/父目录或制造怪异目录。"." 会让 version_dir 解析为 frpc_root 本身，
    // remove_dir_all 会删空整个版本根目录，必须拒绝。
    let has_alnum = version.chars().any(|c| c.is_ascii_alphanumeric());
    if !ok || !has_alnum || version.contains("..") {
        return Err(AppError::Msg("Invalid version".into()));
    }
    Ok(())
}

pub fn version_dir<R: Runtime>(app: &AppHandle<R>, version: &str) -> AppResult<PathBuf> {
    validate_version(version)?;
    Ok(frpc_root(app)?.join(version))
}

pub fn exe_name() -> &'static str {
    if cfg!(windows) {
        "frpc.exe"
    } else {
        "frpc"
    }
}

pub fn frpc_exe<R: Runtime>(app: &AppHandle<R>, version: &str) -> AppResult<PathBuf> {
    Ok(version_dir(app, version)?.join(exe_name()))
}

pub fn runtime_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    Ok(crate::paths::data_dir(app)?.join("runtime"))
}

pub fn runtime_config<R: Runtime>(app: &AppHandle<R>, profile_id: &str) -> AppResult<PathBuf> {
    Ok(runtime_dir(app)?.join(format!("{}.json", sanitize_filename(profile_id))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_version_accepts_normal_versions() {
        assert!(validate_version("0.52.0").is_ok());
        assert!(validate_version("1.2.3").is_ok());
        assert!(validate_version("v0.58.1").is_ok());
        assert!(validate_version("0.61.0-rc.1").is_ok());
    }

    #[test]
    fn validate_version_rejects_traversal_and_junk() {
        assert!(validate_version("").is_err());
        assert!(validate_version(".").is_err());
        assert!(validate_version("..").is_err());
        assert!(validate_version("...").is_err());
        assert!(validate_version("-").is_err());
        assert!(validate_version("../etc").is_err());
        assert!(validate_version("a/b").is_err());
        assert!(validate_version("a\\b").is_err());
        assert!(validate_version(&"x".repeat(65)).is_err());
    }

    #[test]
    fn version_dir_uses_standard_version_validation() {
        assert!(validate_version("0.52.0").is_ok());
        assert!(validate_version("frp 0.58 测试").is_err());
        assert!(validate_version("c:thing").is_err());
    }

    #[test]
    fn runtime_config_filename_is_sanitized() {
        assert_eq!(sanitize_filename("profile-1"), "profile-1");
        assert_eq!(sanitize_filename("../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_filename("..."), "frp");
        assert_eq!(sanitize_filename("中文 连接"), "frp");
    }
}
