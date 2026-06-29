use crate::error::AppResult;
use crate::providers::pinggy::domain::{PinggyData, PinggyEndpoint, PinggyTunnel};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::util::{bool_to_i64, int_to_bool};

pub(crate) async fn read_pinggy_data(pool: &SqlitePool) -> AppResult<PinggyData> {
    let rows = sqlx::query("SELECT * FROM pinggy_tunnels ORDER BY created_at ASC, id ASC")
        .fetch_all(pool)
        .await?;
    let mut tunnels = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id")?;
        tunnels.push(PinggyTunnel {
            id: id.clone(),
            name: row.try_get("name")?,
            token: row.try_get("token")?,
            server: row.try_get("server")?,
            server_port: row.try_get::<i64, _>("server_port")? as u16,
            debugger_port: row
                .try_get::<Option<i64>, _>("debugger_port")?
                .map(|value| value as u16),
            config_file: row.try_get("config_file")?,
            endpoints: read_pinggy_endpoints(pool, &id).await?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(PinggyData { tunnels })
}

async fn read_pinggy_endpoints(
    pool: &SqlitePool,
    tunnel_id: &str,
) -> AppResult<Vec<PinggyEndpoint>> {
    let rows =
        sqlx::query("SELECT * FROM pinggy_endpoints WHERE tunnel_id = ?1 ORDER BY position ASC")
            .bind(tunnel_id)
            .fetch_all(pool)
            .await?;
    rows.into_iter()
        .map(|row| {
            Ok(PinggyEndpoint {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                tunnel_type: row.try_get("tunnel_type")?,
                local_addr: row.try_get("local_addr")?,
                enabled: int_to_bool(row.try_get::<i64, _>("enabled")?),
            })
        })
        .collect()
}

pub(crate) async fn replace_pinggy_data(
    tx: &mut Transaction<'_, Sqlite>,
    data: &PinggyData,
) -> AppResult<()> {
    sqlx::query("DELETE FROM pinggy_endpoints")
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM pinggy_tunnels")
        .execute(&mut **tx)
        .await?;
    for tunnel in &data.tunnels {
        sqlx::query(
            r#"
            INSERT INTO pinggy_tunnels (
                id, name, token, server, server_port, debugger_port, config_file, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
        )
        .bind(&tunnel.id)
        .bind(&tunnel.name)
        .bind(&tunnel.token)
        .bind(&tunnel.server)
        .bind(i64::from(tunnel.server_port))
        .bind(tunnel.debugger_port.map(i64::from))
        .bind(&tunnel.config_file)
        .bind(tunnel.created_at)
        .bind(tunnel.updated_at)
        .execute(&mut **tx)
        .await?;
        for (position, endpoint) in tunnel.endpoints.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO pinggy_endpoints (
                    id, tunnel_id, position, name, tunnel_type, local_addr, enabled
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(&endpoint.id)
            .bind(&tunnel.id)
            .bind(position as i64)
            .bind(&endpoint.name)
            .bind(&endpoint.tunnel_type)
            .bind(&endpoint.local_addr)
            .bind(bool_to_i64(endpoint.enabled))
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
