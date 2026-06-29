use crate::error::AppResult;
use crate::providers::cpolar::domain::{CpolarData, CpolarEndpoint, CpolarTunnel};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::util::{bool_to_i64, int_to_bool};

pub(crate) async fn read_cpolar_data(pool: &SqlitePool) -> AppResult<CpolarData> {
    let rows = sqlx::query("SELECT * FROM cpolar_tunnels ORDER BY created_at ASC, id ASC")
        .fetch_all(pool)
        .await?;
    let mut tunnels = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id")?;
        tunnels.push(CpolarTunnel {
            id: id.clone(),
            name: row.try_get("name")?,
            authtoken: row.try_get("authtoken")?,
            region: row.try_get("region")?,
            config_file: row.try_get("config_file")?,
            endpoints: read_cpolar_endpoints(pool, &id).await?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(CpolarData { tunnels })
}

async fn read_cpolar_endpoints(
    pool: &SqlitePool,
    tunnel_id: &str,
) -> AppResult<Vec<CpolarEndpoint>> {
    let rows =
        sqlx::query("SELECT * FROM cpolar_endpoints WHERE tunnel_id = ?1 ORDER BY position ASC")
            .bind(tunnel_id)
            .fetch_all(pool)
            .await?;
    rows.into_iter()
        .map(|row| {
            Ok(CpolarEndpoint {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                proto: row.try_get("proto")?,
                addr: row.try_get("addr")?,
                hostname: row.try_get("hostname")?,
                remote_addr: row.try_get("remote_addr")?,
                enabled: int_to_bool(row.try_get::<i64, _>("enabled")?),
            })
        })
        .collect()
}

pub(crate) async fn replace_cpolar_data(
    tx: &mut Transaction<'_, Sqlite>,
    data: &CpolarData,
) -> AppResult<()> {
    sqlx::query("DELETE FROM cpolar_endpoints")
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM cpolar_tunnels")
        .execute(&mut **tx)
        .await?;
    for tunnel in &data.tunnels {
        sqlx::query(
            r#"
            INSERT INTO cpolar_tunnels (
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
                INSERT INTO cpolar_endpoints (
                    id, tunnel_id, position, name, proto, addr, hostname, remote_addr, enabled
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                "#,
            )
            .bind(&endpoint.id)
            .bind(&tunnel.id)
            .bind(position as i64)
            .bind(&endpoint.name)
            .bind(&endpoint.proto)
            .bind(&endpoint.addr)
            .bind(&endpoint.hostname)
            .bind(&endpoint.remote_addr)
            .bind(bool_to_i64(endpoint.enabled))
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
