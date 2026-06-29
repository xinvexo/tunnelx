use crate::error::{AppError, AppResult};
use crate::state::AppState;
use sqlx::Row;
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tauri::plugin::{Builder as PluginBuilder, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_sql::{DbInstances, DbPool};

use super::schema::{DB_FILE, DB_URL};

const RECOVERY_MARKER_FILE: &str = "tunnelx-data.sqlite3.recovery-required";

pub fn database_guard_plugin<R: Runtime>() -> TauriPlugin<R> {
    PluginBuilder::new("tunnelx-database-guard")
        .setup(|app, _api| {
            guard_database(app)?;
            Ok(())
        })
        .build()
}

pub(crate) fn with_pool<R, F, Fut, T>(app: &AppHandle<R>, work: F) -> AppResult<T>
where
    R: Runtime,
    F: FnOnce(SqlitePool) -> Fut,
    Fut: std::future::Future<Output = AppResult<T>>,
{
    let pool = sql_pool(app)?;
    block_on_store(work(pool))
}

fn sql_pool<R: Runtime>(app: &AppHandle<R>) -> AppResult<SqlitePool> {
    let instances = app.state::<DbInstances>();
    block_on_store(async {
        let instances = instances.0.read().await;
        match instances.get(DB_URL) {
            Some(DbPool::Sqlite(pool)) => Ok(pool.clone()),
            None => Err(AppError::Msg(
                "TunnelX SQLite database is not loaded".into(),
            )),
        }
    })
}

fn block_on_store<F: std::future::Future>(future: F) -> F::Output {
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(future))
    } else {
        tauri::async_runtime::block_on(future)
    }
}

fn guard_database<R: Runtime>(app: &tauri::AppHandle<R>) -> AppResult<()> {
    let path = database_path(app)?;
    let marker = recovery_marker_path(&path);
    if marker.exists() {
        if recovered_database_ready(&path) {
            let _ = std::fs::remove_file(&marker);
        } else {
            if let Some(state) = app.try_state::<AppState>() {
                state.session.lock().save_blocked = true;
            }
            crate::diag::warn(
                "db",
                format!(
                    "database recovery marker still exists; starting in read-only protection mode: {}",
                    marker.display()
                ),
            );
            return Ok(());
        }
    }

    if !path.exists() {
        return Ok(());
    }

    let url = absolute_db_url(&path)?;
    let result = block_on_store(async {
        let pool = SqlitePool::connect(&url).await?;
        let check: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&pool)
            .await?;
        pool.close().await;
        Ok::<_, AppError>(check)
    });

    match result {
        Ok(check) if check.eq_ignore_ascii_case("ok") => {
            crate::paths::harden_permissions(&path);
            Ok(())
        }
        Ok(check) => {
            backup_and_remove_database(app, &path, &format!("quick_check failed: {check}"));
            if let Some(state) = app.try_state::<AppState>() {
                state.session.lock().save_blocked = true;
            }
            crate::diag::warn(
                "db",
                format!("database integrity check failed; backed up and entered read-only protection: {check}"),
            );
            Ok(())
        }
        Err(error) => {
            backup_and_remove_database(app, &path, &error.to_string());
            if let Some(state) = app.try_state::<AppState>() {
                state.session.lock().save_blocked = true;
            }
            crate::diag::warn(
                "db",
                format!("database preflight failed; backed up and entered read-only protection: {error}"),
            );
            Ok(())
        }
    }
}

fn database_path<R: Runtime>(app: &AppHandle<R>) -> AppResult<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Msg(format!("Failed to locate database directory: {e}")))?;
    crate::paths::ensure_private_dir(&dir)?;
    Ok(dir.join(DB_FILE))
}

fn absolute_db_url(path: &Path) -> AppResult<String> {
    path.to_str()
        .map(|path| format!("sqlite:{path}"))
        .ok_or_else(|| AppError::Msg("Database path is not valid UTF-8".into()))
}

pub(crate) fn harden_database_file<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(path) = database_path(app) {
        if let Some(parent) = path.parent() {
            crate::paths::harden_dir_permissions(parent);
        }
        crate::paths::harden_permissions(&path);
    }
}

pub(crate) fn backup_database<R: Runtime>(app: &AppHandle<R>) {
    let Ok(path) = database_path(app) else {
        return;
    };
    if path.exists() {
        let backup = backup_database_file(&path);
        write_recovery_marker(&path, backup.as_deref(), "database load failed");
    }
}

fn backup_and_remove_database<R: Runtime>(app: &AppHandle<R>, path: &Path, reason: &str) {
    let backup = backup_database_file(path);
    write_recovery_marker(path, backup.as_deref(), reason);
    let _ = std::fs::remove_file(path);
    harden_database_file(app);
}

pub(crate) fn backup_database_file(path: &Path) -> Option<PathBuf> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = path.with_file_name(format!("{DB_FILE}.corrupt-{ts}"));
    if copy_hardened_file(path, &backup).is_ok() {
        Some(backup)
    } else {
        None
    }
}

fn copy_hardened_file(source: &Path, target: &Path) -> std::io::Result<u64> {
    #[cfg(unix)]
    {
        use std::io;
        use std::os::unix::fs::OpenOptionsExt;

        let mut source_file = std::fs::File::open(source)?;
        let mut target_file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(target)?;
        let copied = io::copy(&mut source_file, &mut target_file)?;
        crate::paths::harden_permissions(target);
        Ok(copied)
    }
    #[cfg(not(unix))]
    {
        let copied = std::fs::copy(source, target)?;
        crate::paths::harden_permissions(target);
        Ok(copied)
    }
}

fn recovery_marker_path(path: &Path) -> PathBuf {
    path.with_file_name(RECOVERY_MARKER_FILE)
}

fn write_recovery_marker(path: &Path, backup: Option<&Path>, reason: &str) {
    let marker = recovery_marker_path(path);
    let text = format!(
        "TunnelX database recovery is required.\nreason={}\ndatabase={}\nbackup={}\n",
        reason,
        path.display(),
        backup
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<backup failed>".into())
    );
    let _ = std::fs::write(&marker, text);
    crate::paths::harden_permissions(&marker);
}

fn recovered_database_ready(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let Ok(url) = absolute_db_url(path) else {
        return false;
    };
    block_on_store(async {
        let pool = SqlitePool::connect(&url).await?;
        let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
            .fetch_one(&pool)
            .await?;
        if !quick_check.eq_ignore_ascii_case("ok") {
            pool.close().await;
            return Ok::<_, AppError>(false);
        }
        let has_data = database_has_user_data(&pool).await?;
        pool.close().await;
        Ok::<_, AppError>(has_data)
    })
    .unwrap_or(false)
}

async fn database_has_user_data(pool: &SqlitePool) -> AppResult<bool> {
    for table in [
        "app_settings",
        "connection_order",
        "frp_settings",
        "frp_profiles",
        "cloudflare_tunnels",
        "ngrok_tunnels",
        "cpolar_tunnels",
        "pinggy_tunnels",
    ] {
        if table_row_count(pool, table).await? > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn table_row_count(pool: &SqlitePool, table: &str) -> AppResult<i64> {
    let exists: i64 =
        sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")
            .bind(table)
            .fetch_one(pool)
            .await?;
    if exists == 0 {
        return Ok(0);
    }
    let row = sqlx::query(&format!("SELECT count(*) AS count FROM {table}"))
        .fetch_one(pool)
        .await?;
    row.try_get("count").map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    fn unique_test_dir(name: &str) -> PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let path = std::env::temp_dir()
            .join("tunnelx-tests")
            .join(name)
            .join(suffix);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    #[cfg(unix)]
    fn copy_hardened_file_keeps_backup_private_from_creation() {
        use std::os::unix::fs::PermissionsExt;

        let dir = unique_test_dir("database-copy-hardened");
        let source = dir.join("source.sqlite3");
        let target = dir.join("backup.sqlite3");
        fs::write(&source, b"database").unwrap();
        fs::write(&target, b"old").unwrap();

        let mut permissions = fs::metadata(&target).unwrap().permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(&target, permissions).unwrap();

        let copied = copy_hardened_file(&source, &target).unwrap();

        assert_eq!(copied, b"database".len() as u64);
        assert_eq!(fs::read(&target).unwrap(), b"database");
        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        let _ = fs::remove_dir_all(dir);
    }
}
