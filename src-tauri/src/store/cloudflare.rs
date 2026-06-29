use crate::error::AppResult;
use crate::providers::cloudflare::domain::{
    CloudflareAccountConfig, CloudflareData, CloudflareIngressRule, CloudflarePendingRemoteCleanup,
    CloudflareTunnel,
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::util::{bool_to_i64, enum_from_db, enum_to_db, int_to_bool};

pub(crate) async fn read_cloudflare_data(pool: &SqlitePool) -> AppResult<CloudflareData> {
    let rows = sqlx::query("SELECT * FROM cloudflare_tunnels ORDER BY created_at ASC, id ASC")
        .fetch_all(pool)
        .await?;
    let mut tunnels = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id")?;
        let ingress = read_cloudflare_ingress(pool, &id).await?;
        let pending_remote_cleanup = read_cloudflare_pending_cleanup(pool, &id).await?;
        tunnels.push(CloudflareTunnel {
            id,
            name: row.try_get("name")?,
            account: CloudflareAccountConfig {
                auth_mode: enum_from_db(&row.try_get::<String, _>("auth_mode")?)?,
                api_token: row.try_get("api_token")?,
                token_account_id: row.try_get("token_account_id")?,
                token_account_name: row.try_get("token_account_name")?,
                authorization_account_id: row.try_get("authorization_account_id")?,
                authorization_zone_id: row.try_get("authorization_zone_id")?,
            },
            cert_file: row.try_get("cert_file")?,
            tunnel_id: row.try_get("tunnel_id")?,
            credentials_file: row.try_get("credentials_file")?,
            config_file: row.try_get("config_file")?,
            ingress,
            pending_remote_cleanup,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(CloudflareData { tunnels })
}

async fn read_cloudflare_ingress(
    pool: &SqlitePool,
    tunnel_id: &str,
) -> AppResult<Vec<CloudflareIngressRule>> {
    let rows =
        sqlx::query("SELECT * FROM cloudflare_ingress WHERE tunnel_id = ?1 ORDER BY position ASC")
            .bind(tunnel_id)
            .fetch_all(pool)
            .await?;
    rows.into_iter()
        .map(|row| {
            Ok(CloudflareIngressRule {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                hostname: row.try_get("hostname")?,
                service: row.try_get("service")?,
                runtime_http_host_header: String::new(),
                enabled: int_to_bool(row.try_get::<i64, _>("enabled")?),
                dns_routed: int_to_bool(row.try_get::<i64, _>("dns_routed")?),
            })
        })
        .collect()
}

async fn read_cloudflare_pending_cleanup(
    pool: &SqlitePool,
    tunnel_local_id: &str,
) -> AppResult<Vec<CloudflarePendingRemoteCleanup>> {
    let rows = sqlx::query(
        "SELECT * FROM cloudflare_pending_cleanup WHERE tunnel_local_id = ?1 ORDER BY position ASC",
    )
    .bind(tunnel_local_id)
    .fetch_all(pool)
    .await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.try_get("id")?;
        items.push(CloudflarePendingRemoteCleanup {
            kind: enum_from_db(&row.try_get::<String, _>("kind")?)?,
            tunnel_id: row.try_get("remote_tunnel_id")?,
            tunnel_name: row.try_get("tunnel_name")?,
            account: CloudflareAccountConfig {
                auth_mode: enum_from_db(&row.try_get::<String, _>("auth_mode")?)?,
                api_token: row.try_get("api_token")?,
                token_account_id: row.try_get("token_account_id")?,
                token_account_name: row.try_get("token_account_name")?,
                authorization_account_id: row.try_get("authorization_account_id")?,
                authorization_zone_id: row.try_get("authorization_zone_id")?,
            },
            cert_file: row.try_get("cert_file")?,
            credentials_file: row.try_get("credentials_file")?,
            config_file: row.try_get("config_file")?,
            dns_hostnames: read_cleanup_dns(pool, id).await?,
        });
    }
    Ok(items)
}

async fn read_cleanup_dns(pool: &SqlitePool, cleanup_id: i64) -> AppResult<Vec<String>> {
    let rows = sqlx::query(
        "SELECT hostname FROM cloudflare_pending_cleanup_dns WHERE cleanup_id = ?1 ORDER BY position ASC",
    )
    .bind(cleanup_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| row.try_get("hostname").map_err(Into::into))
        .collect()
}

pub(crate) async fn replace_cloudflare_data(
    tx: &mut Transaction<'_, Sqlite>,
    data: &CloudflareData,
) -> AppResult<()> {
    for table in [
        "cloudflare_pending_cleanup_dns",
        "cloudflare_pending_cleanup",
        "cloudflare_ingress",
        "cloudflare_tunnels",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&mut **tx)
            .await?;
    }
    for tunnel in &data.tunnels {
        insert_cloudflare_tunnel(tx, tunnel).await?;
    }
    Ok(())
}

async fn insert_cloudflare_tunnel(
    tx: &mut Transaction<'_, Sqlite>,
    tunnel: &CloudflareTunnel,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO cloudflare_tunnels (
            id, name, auth_mode, api_token, token_account_id, token_account_name,
            authorization_account_id, authorization_zone_id, cert_file, tunnel_id,
            credentials_file, config_file, created_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
        "#,
    )
    .bind(&tunnel.id)
    .bind(&tunnel.name)
    .bind(enum_to_db(&tunnel.account.auth_mode)?)
    .bind(&tunnel.account.api_token)
    .bind(&tunnel.account.token_account_id)
    .bind(&tunnel.account.token_account_name)
    .bind(&tunnel.account.authorization_account_id)
    .bind(&tunnel.account.authorization_zone_id)
    .bind(&tunnel.cert_file)
    .bind(&tunnel.tunnel_id)
    .bind(&tunnel.credentials_file)
    .bind(&tunnel.config_file)
    .bind(tunnel.created_at)
    .bind(tunnel.updated_at)
    .execute(&mut **tx)
    .await?;

    for (position, ingress) in tunnel.ingress.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO cloudflare_ingress (
                id, tunnel_id, position, name, hostname, service, enabled, dns_routed
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
        )
        .bind(&ingress.id)
        .bind(&tunnel.id)
        .bind(position as i64)
        .bind(&ingress.name)
        .bind(&ingress.hostname)
        .bind(&ingress.service)
        .bind(bool_to_i64(ingress.enabled))
        .bind(bool_to_i64(ingress.dns_routed))
        .execute(&mut **tx)
        .await?;
    }

    for (position, cleanup) in tunnel.pending_remote_cleanup.iter().enumerate() {
        let result = sqlx::query(
            r#"
            INSERT INTO cloudflare_pending_cleanup (
                tunnel_local_id, position, kind, remote_tunnel_id, tunnel_name,
                auth_mode, api_token, token_account_id, token_account_name,
                authorization_account_id, authorization_zone_id, cert_file,
                credentials_file, config_file
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
        )
        .bind(&tunnel.id)
        .bind(position as i64)
        .bind(enum_to_db(&cleanup.kind)?)
        .bind(&cleanup.tunnel_id)
        .bind(&cleanup.tunnel_name)
        .bind(enum_to_db(&cleanup.account.auth_mode)?)
        .bind(&cleanup.account.api_token)
        .bind(&cleanup.account.token_account_id)
        .bind(&cleanup.account.token_account_name)
        .bind(&cleanup.account.authorization_account_id)
        .bind(&cleanup.account.authorization_zone_id)
        .bind(&cleanup.cert_file)
        .bind(&cleanup.credentials_file)
        .bind(&cleanup.config_file)
        .execute(&mut **tx)
        .await?;
        let cleanup_id = result.last_insert_rowid();
        for (dns_position, hostname) in cleanup.dns_hostnames.iter().enumerate() {
            sqlx::query(
                "INSERT INTO cloudflare_pending_cleanup_dns (cleanup_id, position, hostname) VALUES (?1, ?2, ?3)",
            )
            .bind(cleanup_id)
            .bind(dns_position as i64)
            .bind(hostname)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
