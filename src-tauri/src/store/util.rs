use crate::domain::AppData;
use crate::error::{AppError, AppResult};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;

pub(crate) type Map = HashMap<String, String>;

pub(crate) fn provider_data<T>(data: &AppData, provider_id: &str) -> AppResult<T>
where
    T: DeserializeOwned + Default,
{
    data.providers
        .get(provider_id)
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map(|value| value.unwrap_or_default())
        .map_err(Into::into)
}

pub(crate) async fn read_ordered_strings(
    pool: &SqlitePool,
    table: &str,
    id_column: &str,
    id: &str,
) -> AppResult<Vec<String>> {
    let rows = sqlx::query(&format!(
        "SELECT value FROM {table} WHERE {id_column} = ?1 ORDER BY position ASC"
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| row.try_get("value").map_err(Into::into))
        .collect()
}

pub(crate) async fn insert_ordered_strings(
    tx: &mut Transaction<'_, Sqlite>,
    table: &str,
    id_column: &str,
    id: &str,
    values: &[String],
) -> AppResult<()> {
    for (position, value) in values.iter().enumerate() {
        sqlx::query(&format!(
            "INSERT INTO {table} ({id_column}, position, value) VALUES (?1, ?2, ?3)"
        ))
        .bind(id)
        .bind(position as i64)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(crate) async fn read_kv(
    pool: &SqlitePool,
    table: &str,
    id_column: &str,
    id: &str,
) -> AppResult<Map> {
    let rows = sqlx::query(&format!(
        "SELECT key, value FROM {table} WHERE {id_column} = ?1 ORDER BY key ASC"
    ))
    .bind(id)
    .fetch_all(pool)
    .await?;
    let mut map = Map::new();
    for row in rows {
        map.insert(row.try_get("key")?, row.try_get("value")?);
    }
    Ok(map)
}

pub(crate) async fn insert_kv(
    tx: &mut Transaction<'_, Sqlite>,
    table: &str,
    id_column: &str,
    id: &str,
    map: Option<&Map>,
) -> AppResult<()> {
    if let Some(map) = map {
        for (key, value) in map {
            sqlx::query(&format!(
                "INSERT INTO {table} ({id_column}, key, value) VALUES (?1, ?2, ?3)"
            ))
            .bind(id)
            .bind(key)
            .bind(value)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

pub(crate) fn empty_map_to_none(map: Map) -> Option<Map> {
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

pub(crate) fn enum_to_db<T: Serialize>(value: &T) -> AppResult<String> {
    match serde_json::to_value(value)? {
        Value::String(value) => Ok(value),
        _ => Err(AppError::Msg(
            "Enum value cannot be written to the database".into(),
        )),
    }
}

pub(crate) fn enum_from_db<T: DeserializeOwned>(value: &str) -> AppResult<T> {
    serde_json::from_value(Value::String(value.to_string())).map_err(Into::into)
}

pub(crate) fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

pub(crate) fn int_to_bool(value: i64) -> bool {
    value != 0
}

pub(crate) fn opt_bool_to_i64(value: Option<bool>) -> Option<i64> {
    value.map(bool_to_i64)
}

pub(crate) fn opt_int_to_bool(value: Option<i64>) -> Option<bool> {
    value.map(int_to_bool)
}

pub(crate) fn i64_to_u16(value: i64) -> u16 {
    u16::try_from(value).unwrap_or_default()
}

pub(crate) fn opt_i64_to_u16(value: Option<i64>) -> Option<u16> {
    value.and_then(|value| u16::try_from(value).ok())
}
