//! 应用数据目录下的路径约定：
//! provider 专属目录结构由各 provider 模块维护；这里仅提供平台级目录与安全写入工具。
use crate::error::{AppError, AppResult};
#[cfg(test)]
mod test_support;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};
#[cfg(test)]
pub(crate) use test_support::{data_dir_test_guard, set_test_data_dir};

pub fn data_dir<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_support::data_dir_override() {
        ensure_private_dir(&path)?;
        return Ok(path);
    }

    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Msg(format!("Failed to locate app data directory: {e}")))?;
    ensure_private_dir(&path)?;
    Ok(path)
}

pub fn ensure_private_dir(path: &Path) -> AppResult<()> {
    std::fs::create_dir_all(path)
        .map_err(|e| AppError::Msg(format!("Failed to create data directory: {e}")))?;
    harden_dir_permissions(path);
    Ok(())
}

/// 将应用私有目录权限收紧到 0700。目录里会放 token、凭据、运行时配置和 SQLite 数据库。
pub fn harden_dir_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.is_dir() {
                let mut perm = meta.permissions();
                perm.set_mode(0o700);
                let _ = std::fs::set_permissions(path, perm);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// 将已存在的文件权限收紧到 0600（仅当前用户可读写）。Unix 专用，其他平台 no-op。
/// 用于含明文密钥的文件（数据库、运行时配置、损坏备份），避免同机其他用户读取。
/// 尽力而为：文件不存在或 chmod 失败都忽略（调用方已写入数据，权限收紧是额外加固）。
pub fn harden_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perm = meta.permissions();
            perm.set_mode(0o600);
            let _ = std::fs::set_permissions(path, perm);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

pub fn remove_file_if_matches(path: &Path, expected: &Path) {
    if path_matches_expected(path, expected) {
        let _ = std::fs::remove_file(path);
    }
}

pub fn remove_file_if_under(path: &Path, root: &Path) {
    if path_is_under(path, root) {
        let _ = std::fs::remove_file(path);
    }
}

fn path_matches_expected(path: &Path, expected: &Path) -> bool {
    if path.as_os_str().is_empty() || expected.as_os_str().is_empty() {
        return false;
    }
    if path == expected {
        return true;
    }
    match (path.canonicalize(), expected.canonicalize()) {
        (Ok(path), Ok(expected)) => path == expected,
        _ => false,
    }
}

pub(crate) fn path_is_under(path: &Path, root: &Path) -> bool {
    if path.as_os_str().is_empty() || root.as_os_str().is_empty() {
        return false;
    }
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(path), Ok(root)) => path.starts_with(root),
        _ => false,
    }
}

/// 写入含敏感信息（明文 token/密码）的文件。Unix 上以 0600 创建，
/// 确保从一开始就只有当前用户可读写，不存在“先 0644 创建、再 chmod”的可读窗口。
pub fn write_secret_file(path: &std::path::Path, contents: &[u8]) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|e| AppError::Msg(format!("Failed to write file: {e}")))?;
        file.write_all(contents)
            .map_err(|e| AppError::Msg(format!("Failed to write file: {e}")))?;
        // 已存在的旧文件 mode 不会被 open 修改，显式收紧一次兜底。
        harden_permissions(path);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)
            .map_err(|e| AppError::Msg(format!("Failed to write file: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_test_dir(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        std::env::temp_dir()
            .join("tunnelx-tests")
            .join(name)
            .join(suffix)
    }

    #[test]
    #[cfg(unix)]
    fn ensure_private_dir_tightens_existing_directory() {
        use std::os::unix::fs::PermissionsExt;

        let dir = unique_test_dir("private-dir");
        std::fs::create_dir_all(&dir).unwrap();
        let mut permissions = std::fs::metadata(&dir).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&dir, permissions).unwrap();

        ensure_private_dir(&dir).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn remove_file_if_matches_keeps_unexpected_path() {
        let dir = unique_test_dir("remove-match");
        std::fs::create_dir_all(&dir).unwrap();
        let expected = dir.join("expected.yml");
        let external = dir.join("external.yml");
        std::fs::write(&expected, b"expected").unwrap();
        std::fs::write(&external, b"external").unwrap();

        remove_file_if_matches(&external, &expected);

        assert!(external.exists());
        assert!(expected.exists());

        remove_file_if_matches(&expected, &expected);

        assert!(!expected.exists());
        assert!(external.exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn remove_file_if_under_keeps_files_outside_root() {
        let dir = unique_test_dir("remove-under");
        let root = dir.join("root");
        let inside = root.join("inside.txt");
        let outside = dir.join("outside.txt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&inside, b"inside").unwrap();
        std::fs::write(&outside, b"outside").unwrap();

        remove_file_if_under(&outside, &root);

        assert!(outside.exists());
        assert!(inside.exists());

        remove_file_if_under(&inside, &root);

        assert!(!inside.exists());
        assert!(outside.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
