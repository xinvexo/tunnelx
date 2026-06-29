use crate::domain::{now_ms, AppSettings};
use crate::error::AppResult;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::util::{bool_to_i64, int_to_bool};

pub(crate) async fn read_settings(pool: &SqlitePool) -> AppResult<AppSettings> {
    let row = sqlx::query(
        r#"
        SELECT theme, silent_start, auto_connect, lightweight_mode, auto_update, traffic_stats_enabled
        FROM app_settings
        WHERE id = 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(AppSettings::default());
    };

    Ok(AppSettings {
        theme: row.try_get("theme")?,
        silent_start: int_to_bool(row.try_get::<i64, _>("silent_start")?),
        auto_connect: int_to_bool(row.try_get::<i64, _>("auto_connect")?),
        lightweight_mode: int_to_bool(row.try_get::<i64, _>("lightweight_mode")?),
        auto_update: int_to_bool(row.try_get::<i64, _>("auto_update")?),
        traffic_stats_enabled: int_to_bool(row.try_get::<i64, _>("traffic_stats_enabled")?),
    })
}

pub(crate) async fn write_settings(
    tx: &mut Transaction<'_, Sqlite>,
    settings: &AppSettings,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO app_settings (
            id, theme, silent_start, auto_connect, lightweight_mode,
            auto_update, traffic_stats_enabled, updated_at
        )
        VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(id) DO UPDATE SET
            theme = excluded.theme,
            silent_start = excluded.silent_start,
            auto_connect = excluded.auto_connect,
            lightweight_mode = excluded.lightweight_mode,
            auto_update = excluded.auto_update,
            traffic_stats_enabled = excluded.traffic_stats_enabled,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(settings.theme.as_deref())
    .bind(bool_to_i64(settings.silent_start))
    .bind(bool_to_i64(settings.auto_connect))
    .bind(bool_to_i64(settings.lightweight_mode))
    .bind(bool_to_i64(settings.auto_update))
    .bind(bool_to_i64(settings.traffic_stats_enabled))
    .bind(now_ms())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn read_connection_order(pool: &SqlitePool) -> AppResult<Vec<String>> {
    let rows = sqlx::query("SELECT connection_key FROM connection_order ORDER BY position ASC")
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| row.try_get("connection_key").map_err(Into::into))
        .collect()
}

pub(crate) async fn replace_connection_order(
    tx: &mut Transaction<'_, Sqlite>,
    order: &[String],
) -> AppResult<()> {
    sqlx::query("DELETE FROM connection_order")
        .execute(&mut **tx)
        .await?;
    for (position, key) in order.iter().enumerate() {
        sqlx::query("INSERT INTO connection_order (position, connection_key) VALUES (?1, ?2)")
            .bind(position as i64)
            .bind(key)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
