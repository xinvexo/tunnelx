use crate::error::AppResult;
use crate::providers::ngrok::domain::{NgrokData, NgrokEndpoint, NgrokTunnel};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::util::{bool_to_i64, int_to_bool};

pub(crate) async fn read_ngrok_data(pool: &SqlitePool) -> AppResult<NgrokData> {
    let rows = sqlx::query("SELECT * FROM ngrok_tunnels ORDER BY created_at ASC, id ASC")
        .fetch_all(pool)
        .await?;
    let mut tunnels = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id")?;
        tunnels.push(NgrokTunnel {
            id: id.clone(),
            name: row.try_get("name")?,
            authtoken: row.try_get("authtoken")?,
            region: row.try_get("region")?,
            config_file: row.try_get("config_file")?,
            endpoints: read_ngrok_endpoints(pool, &id).await?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(NgrokData { tunnels })
}

async fn read_ngrok_endpoints(pool: &SqlitePool, tunnel_id: &str) -> AppResult<Vec<NgrokEndpoint>> {
    let rows =
        sqlx::query("SELECT * FROM ngrok_endpoints WHERE tunnel_id = ?1 ORDER BY position ASC")
            .bind(tunnel_id)
            .fetch_all(pool)
            .await?;
    rows.into_iter()
        .map(|row| {
            Ok(NgrokEndpoint {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                proto: row.try_get("proto")?,
                addr: row.try_get("addr")?,
                domain: row.try_get("domain")?,
                enabled: int_to_bool(row.try_get::<i64, _>("enabled")?),
            })
        })
        .collect()
}

pub(crate) async fn replace_ngrok_data(
    tx: &mut Transaction<'_, Sqlite>,
    data: &NgrokData,
) -> AppResult<()> {
    sqlx::query("DELETE FROM ngrok_endpoints")
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM ngrok_tunnels")
        .execute(&mut **tx)
        .await?;
    for tunnel in &data.tunnels {
        sqlx::query(
            r#"
            INSERT INTO ngrok_tunnels (
                id, name, authtoken, region, config_file, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
        )
        .bind(&tunnel.id)
        .bind(&tunnel.name)
        .bind(&tunnel.authtoken)
        .bind(&tunnel.region)
        .bind(&tunnel.config_file)
        .bind(tunnel.created_at)
        .bind(tunnel.updated_at)
        .execute(&mut **tx)
        .await?;
        for (position, endpoint) in tunnel.endpoints.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO ngrok_endpoints (
                    id, tunnel_id, position, name, proto, addr, domain, enabled
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )
            .bind(&endpoint.id)
            .bind(&tunnel.id)
            .bind(position as i64)
            .bind(&endpoint.name)
            .bind(&endpoint.proto)
            .bind(&endpoint.addr)
            .bind(&endpoint.domain)
            .bind(bool_to_i64(endpoint.enabled))
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
