use crate::domain::AppData;
use crate::error::{AppError, AppResult};
use crate::providers::cloudflare::domain::CloudflareData;
use crate::providers::contract::{
    CLOUDFLARE_PROVIDER_ID, CPOLAR_PROVIDER_ID, FRP_PROVIDER_ID, NGROK_PROVIDER_ID,
    PINGGY_PROVIDER_ID,
};
use crate::providers::cpolar::domain::CpolarData;
use crate::providers::frp::data::FrpData;
use crate::providers::ngrok::domain::NgrokData;
use crate::providers::pinggy::domain::PinggyData;
use crate::state::AppState;
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use tauri::{AppHandle, Runtime};

use super::{cloudflare, cpolar, database, frp, ngrok, pinggy, platform, schema, util};

pub fn load_app_data<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> AppResult<()> {
    {
        let mut session = state.session.lock();
        if session.loaded {
            return Ok(());
        }
        session.loaded = true;
    }

    match database::with_pool(app, read_app_data) {
        Ok(data) => {
            state.config.replace(data);
            database::harden_database_file(app);
            if state.session.lock().save_blocked {
                return Err(AppError::Msg(schema::SAVE_BLOCKED_MSG.into()));
            }
            Ok(())
        }
        Err(error) => {
            database::backup_database(app);
            state.session.lock().save_blocked = true;
            Err(AppError::Msg(format!(
                "Database load failed; TunnelX entered read-only protection mode: {error}"
            )))
        }
    }
}

pub fn save_app_data<R: Runtime>(app: &AppHandle<R>, state: &AppState) -> AppResult<()> {
    ensure_app_data_writable(state)?;
    let data = state.config.snapshot()?;
    database::with_pool(app, |pool| write_app_data(pool, data))?;
    database::harden_database_file(app);
    Ok(())
}

pub fn ensure_app_data_writable(state: &AppState) -> AppResult<()> {
    if state.session.lock().save_blocked {
        return Err(AppError::Msg(schema::SAVE_BLOCKED_MSG.into()));
    }
    Ok(())
}

async fn read_app_data(pool: SqlitePool) -> AppResult<AppData> {
    let mut providers = BTreeMap::new();
    let frp = frp::read_frp_data(&pool).await?;
    if !frp.profiles.is_empty() || frp::has_settings(&frp.settings) {
        providers.insert(FRP_PROVIDER_ID.into(), serde_json::to_value(frp)?);
    }
    let cloudflare = cloudflare::read_cloudflare_data(&pool).await?;
    if !cloudflare.tunnels.is_empty() {
        providers.insert(
            CLOUDFLARE_PROVIDER_ID.into(),
            serde_json::to_value(cloudflare)?,
        );
    }
    let ngrok = ngrok::read_ngrok_data(&pool).await?;
    if !ngrok.tunnels.is_empty() {
        providers.insert(NGROK_PROVIDER_ID.into(), serde_json::to_value(ngrok)?);
    }
    let cpolar = cpolar::read_cpolar_data(&pool).await?;
    if !cpolar.tunnels.is_empty() {
        providers.insert(CPOLAR_PROVIDER_ID.into(), serde_json::to_value(cpolar)?);
    }
    let pinggy = pinggy::read_pinggy_data(&pool).await?;
    if !pinggy.tunnels.is_empty() {
        providers.insert(PINGGY_PROVIDER_ID.into(), serde_json::to_value(pinggy)?);
    }

    Ok(AppData {
        connection_order: platform::read_connection_order(&pool).await?,
        providers,
        settings: platform::read_settings(&pool).await?,
    })
}

async fn write_app_data(pool: SqlitePool, data: AppData) -> AppResult<()> {
    let frp = util::provider_data::<FrpData>(&data, FRP_PROVIDER_ID)?;
    let cloudflare = util::provider_data::<CloudflareData>(&data, CLOUDFLARE_PROVIDER_ID)?;
    let ngrok = util::provider_data::<NgrokData>(&data, NGROK_PROVIDER_ID)?;
    let cpolar = util::provider_data::<CpolarData>(&data, CPOLAR_PROVIDER_ID)?;
    let pinggy = util::provider_data::<PinggyData>(&data, PINGGY_PROVIDER_ID)?;

    let mut tx = pool.begin().await?;
    platform::write_settings(&mut tx, &data.settings).await?;
    platform::replace_connection_order(&mut tx, &data.connection_order).await?;
    frp::replace_frp_data(&mut tx, &frp).await?;
    cloudflare::replace_cloudflare_data(&mut tx, &cloudflare).await?;
    ngrok::replace_ngrok_data(&mut tx, &ngrok).await?;
    cpolar::replace_cpolar_data(&mut tx, &cpolar).await?;
    pinggy::replace_pinggy_data(&mut tx, &pinggy).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::AppSettings;

    use crate::providers::cloudflare::domain::{CloudflareIngressRule, CloudflareTunnel};

    use crate::providers::frp::data::FrpSettings;

    use crate::providers::frp::domain::{HttpProxy, Profile, ProfileProxy, ProxyBase, ProxyConfig};

    use crate::providers::ngrok::domain::NgrokTunnel;

    use sqlx::Executor;

    async fn memory_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(schema::INIT_SCHEMA_SQL).await.unwrap();
        pool.execute(schema::PROVIDER_EXTENSION_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(schema::INGRESS_NAME_SCHEMA_SQL).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn sqlite_empty_database_uses_defaults() {
        let pool = memory_pool().await;

        let data = read_app_data(pool).await.unwrap();

        assert!(data.connection_order.is_empty());
        assert!(data.providers.is_empty());
        assert!(!data.settings.auto_connect);
        assert!(data.settings.traffic_stats_enabled);
    }

    #[tokio::test]
    async fn sqlite_round_trips_platform_and_provider_data_without_json_blob() {
        let pool = memory_pool().await;
        let mut data = AppData {
            connection_order: vec!["frp:profile-1".into(), "cloudflare:tunnel-1".into()],
            settings: AppSettings {
                theme: Some("dark".into()),
                auto_connect: true,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut frp = FrpData::default();
        frp.settings.active_frpc_version = Some("0.64.0".into());
        let mut profile = Profile::new("home");
        profile.id = "profile-1".into();
        profile.proxies.push(ProfileProxy {
            id: "proxy-1".into(),
            enabled: true,
            config: ProxyConfig::Http(HttpProxy {
                base: ProxyBase {
                    name: "web".into(),
                    local_ip: "127.0.0.1".into(),
                    local_port: Some(8080),
                    ..Default::default()
                },
                custom_domains: vec!["app.example.com".into()],
                ..Default::default()
            }),
        });
        frp.profiles.push(profile);
        data.providers
            .insert(FRP_PROVIDER_ID.into(), serde_json::to_value(frp).unwrap());

        let mut cf = CloudflareData::default();
        let mut tunnel = CloudflareTunnel::new("edge");
        tunnel.id = "tunnel-1".into();
        tunnel.ingress.push(CloudflareIngressRule {
            id: "ingress-1".into(),
            hostname: "edge.example.com".into(),
            service: "http://localhost:8080".into(),
            enabled: true,
            dns_routed: true,
            ..Default::default()
        });
        cf.tunnels.push(tunnel);
        data.providers.insert(
            CLOUDFLARE_PROVIDER_ID.into(),
            serde_json::to_value(cf).unwrap(),
        );

        write_app_data(pool.clone(), data.clone()).await.unwrap();
        let saved = read_app_data(pool.clone()).await.unwrap();

        assert_eq!(saved.connection_order, data.connection_order);
        assert_eq!(saved.settings.theme.as_deref(), Some("dark"));
        assert!(saved.settings.auto_connect);
        assert!(saved.providers.contains_key(FRP_PROVIDER_ID));
        assert!(saved.providers.contains_key(CLOUDFLARE_PROVIDER_ID));

        let blob_table_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'provider_data'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(blob_table_count, 0);
    }

    #[tokio::test]
    async fn sqlite_save_replaces_removed_provider_data() {
        let pool = memory_pool().await;
        let mut first = AppData::default();
        first.providers.insert(
            FRP_PROVIDER_ID.into(),
            serde_json::to_value(FrpData {
                profiles: vec![Profile::new("one")],
                settings: FrpSettings::default(),
            })
            .unwrap(),
        );
        first.providers.insert(
            NGROK_PROVIDER_ID.into(),
            serde_json::to_value(NgrokData {
                tunnels: vec![NgrokTunnel::new("ngrok")],
            })
            .unwrap(),
        );
        write_app_data(pool.clone(), first).await.unwrap();

        let mut second = AppData::default();
        second.providers.insert(
            FRP_PROVIDER_ID.into(),
            serde_json::to_value(FrpData {
                profiles: vec![Profile::new("one")],
                settings: FrpSettings::default(),
            })
            .unwrap(),
        );
        write_app_data(pool.clone(), second).await.unwrap();

        let saved = read_app_data(pool).await.unwrap();
        assert!(saved.providers.contains_key(FRP_PROVIDER_ID));
        assert!(!saved.providers.contains_key(NGROK_PROVIDER_ID));
    }
}
