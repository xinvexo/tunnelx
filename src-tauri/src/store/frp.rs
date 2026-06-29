use crate::domain::now_ms;
use crate::error::{AppError, AppResult};
use crate::providers::frp::data::{FrpData, FrpSettings};
use crate::providers::frp::domain::{
    Auth, HeaderOps, HealthCheck, HttpHeader, HttpProxy, HttpsProxy, LoadBalancer, NatTraversal,
    Oidc, PluginOptions, Profile, ProfileProxy, ProxyBase, ProxyConfig, ProxyTransport,
    ServerConfig, StcpProxy, SudpProxy, TcpProxy, TcpmuxProxy, Tls, TlsFiles, Transport, UdpProxy,
    WebServer, XtcpProxy,
};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::util::{self, Map};

pub(crate) async fn read_frp_data(pool: &SqlitePool) -> AppResult<FrpData> {
    let settings = read_frp_settings(pool).await?;
    let profile_rows = sqlx::query("SELECT * FROM frp_profiles ORDER BY position ASC")
        .fetch_all(pool)
        .await?;
    let mut profiles = Vec::with_capacity(profile_rows.len());
    for row in profile_rows {
        let id: String = row.try_get("id")?;
        profiles.push(read_frp_profile(pool, row, &id).await?);
    }
    Ok(FrpData { profiles, settings })
}

async fn read_frp_settings(pool: &SqlitePool) -> AppResult<FrpSettings> {
    let row = sqlx::query("SELECT active_frpc_version, frpc_mirror FROM frp_settings WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    Ok(match row {
        Some(row) => FrpSettings {
            active_frpc_version: row.try_get("active_frpc_version")?,
            frpc_mirror: row.try_get("frpc_mirror")?,
        },
        None => FrpSettings::default(),
    })
}

pub(crate) fn has_settings(settings: &FrpSettings) -> bool {
    settings.active_frpc_version.is_some() || settings.frpc_mirror.is_some()
}

async fn read_frp_profile(pool: &SqlitePool, row: SqliteRow, id: &str) -> AppResult<Profile> {
    let auth = Auth {
        method: util::enum_from_db(&row.try_get::<String, _>("auth_method")?)?,
        token: row.try_get("auth_token")?,
        additional_scopes: util::read_ordered_strings(
            pool,
            "frp_profile_auth_scopes",
            "profile_id",
            id,
        )
        .await?,
        oidc: Oidc {
            client_id: row.try_get("oidc_client_id")?,
            client_secret: row.try_get("oidc_client_secret")?,
            audience: row.try_get("oidc_audience")?,
            scope: row.try_get("oidc_scope")?,
            token_endpoint_url: row.try_get("oidc_token_endpoint_url")?,
            trusted_ca_file: row.try_get("oidc_trusted_ca_file")?,
            insecure_skip_verify: util::int_to_bool(
                row.try_get::<i64, _>("oidc_insecure_skip_verify")?,
            ),
            additional_endpoint_params: util::empty_map_to_none(
                util::read_kv(pool, "frp_profile_oidc_params", "profile_id", id).await?,
            ),
        },
    };

    let server = ServerConfig {
        user: row.try_get("user")?,
        client_id: row.try_get("client_id")?,
        server_addr: row.try_get("server_addr")?,
        server_port: util::i64_to_u16(row.try_get::<i64, _>("server_port")?),
        auth,
        transport: Transport {
            protocol: row.try_get("transport_protocol")?,
            dial_server_timeout: row.try_get("transport_dial_server_timeout")?,
            dial_server_keepalive: row.try_get("transport_dial_server_keepalive")?,
            connect_server_local_ip: row.try_get("transport_connect_server_local_ip")?,
            proxy_url: row.try_get("transport_proxy_url")?,
            pool_count: row.try_get("transport_pool_count")?,
            tcp_mux: util::opt_int_to_bool(row.try_get("transport_tcp_mux")?),
            tcp_mux_keepalive_interval: row.try_get("transport_tcp_mux_keepalive_interval")?,
            quic: crate::providers::frp::domain::Quic {
                keepalive_period: row.try_get("transport_quic_keepalive_period")?,
                max_idle_timeout: row.try_get("transport_quic_max_idle_timeout")?,
                max_incoming_streams: row.try_get("transport_quic_max_incoming_streams")?,
            },
            heartbeat_interval: row.try_get("transport_heartbeat_interval")?,
            heartbeat_timeout: row.try_get("transport_heartbeat_timeout")?,
            tls: Tls {
                enable: util::opt_int_to_bool(row.try_get("transport_tls_enable")?),
                disable_custom_tls_first_byte: util::opt_int_to_bool(
                    row.try_get("transport_tls_disable_custom_tls_first_byte")?,
                ),
                files: TlsFiles {
                    cert_file: row.try_get("transport_tls_cert_file")?,
                    key_file: row.try_get("transport_tls_key_file")?,
                    trusted_ca_file: row.try_get("transport_tls_trusted_ca_file")?,
                    server_name: row.try_get("transport_tls_server_name")?,
                },
            },
        },
        log: crate::providers::frp::domain::Log {
            to: row.try_get("log_to")?,
            level: row.try_get("log_level")?,
            max_days: row.try_get("log_max_days")?,
            disable_print_color: util::int_to_bool(
                row.try_get::<i64, _>("log_disable_print_color")?,
            ),
        },
        web_server: WebServer {
            addr: row.try_get("web_addr")?,
            port: util::i64_to_u16(row.try_get::<i64, _>("web_port")?),
            user: row.try_get("web_user")?,
            password: row.try_get("web_password")?,
            assets_dir: row.try_get("web_assets_dir")?,
            pprof_enable: util::int_to_bool(row.try_get::<i64, _>("web_pprof_enable")?),
            tls: if util::int_to_bool(row.try_get::<i64, _>("web_tls_enabled")?) {
                Some(TlsFiles {
                    cert_file: row.try_get("web_tls_cert_file")?,
                    key_file: row.try_get("web_tls_key_file")?,
                    trusted_ca_file: row.try_get("web_tls_trusted_ca_file")?,
                    server_name: row.try_get("web_tls_server_name")?,
                })
            } else {
                None
            },
        },
        dns_server: row.try_get("dns_server")?,
        nat_hole_stun_server: row.try_get("nat_hole_stun_server")?,
        login_fail_exit: util::opt_int_to_bool(row.try_get("login_fail_exit")?),
        udp_packet_size: row.try_get("udp_packet_size")?,
        metadatas: util::empty_map_to_none(
            util::read_kv(pool, "frp_profile_metadatas", "profile_id", id).await?,
        ),
    };

    let proxies = read_frp_proxies(pool, id).await?;
    Ok(Profile {
        id: id.to_string(),
        name: row.try_get("name")?,
        server,
        proxies,
        frpc_version: row.try_get("frpc_version")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

async fn read_frp_proxies(pool: &SqlitePool, profile_id: &str) -> AppResult<Vec<ProfileProxy>> {
    let rows = sqlx::query("SELECT * FROM frp_proxies WHERE profile_id = ?1 ORDER BY position ASC")
        .bind(profile_id)
        .fetch_all(pool)
        .await?;
    let mut proxies = Vec::with_capacity(rows.len());
    for row in rows {
        let proxy_id: String = row.try_get("id")?;
        proxies.push(read_frp_proxy(pool, row, &proxy_id).await?);
    }
    Ok(proxies)
}

async fn read_frp_proxy(
    pool: &SqlitePool,
    row: SqliteRow,
    proxy_id: &str,
) -> AppResult<ProfileProxy> {
    let proxy_type: String = row.try_get("proxy_type")?;
    let base = ProxyBase {
        name: row.try_get("name")?,
        transport: ProxyTransport {
            use_encryption: util::int_to_bool(row.try_get::<i64, _>("transport_use_encryption")?),
            use_compression: util::int_to_bool(row.try_get::<i64, _>("transport_use_compression")?),
            bandwidth_limit: row.try_get("transport_bandwidth_limit")?,
            bandwidth_limit_mode: row.try_get("transport_bandwidth_limit_mode")?,
            proxy_protocol_version: row.try_get("transport_proxy_protocol_version")?,
        },
        health_check: HealthCheck {
            r#type: row.try_get("health_check_type")?,
            timeout_seconds: row.try_get("health_check_timeout_seconds")?,
            max_failed: row.try_get("health_check_max_failed")?,
            interval_seconds: row.try_get("health_check_interval_seconds")?,
            path: row.try_get("health_check_path")?,
            http_headers: read_http_headers(pool, proxy_id).await?,
        },
        load_balancer: LoadBalancer {
            group: row.try_get("load_balancer_group")?,
            group_key: row.try_get("load_balancer_group_key")?,
        },
        metadatas: util::empty_map_to_none(
            util::read_kv(pool, "frp_proxy_metadatas", "proxy_id", proxy_id).await?,
        ),
        annotations: util::empty_map_to_none(
            util::read_kv(pool, "frp_proxy_annotations", "proxy_id", proxy_id).await?,
        ),
        local_ip: row.try_get("local_ip")?,
        local_port: util::opt_i64_to_u16(row.try_get("local_port")?),
        plugin: read_plugin(pool, &row, proxy_id).await?,
    };
    let config = match proxy_type.as_str() {
        "tcp" => ProxyConfig::Tcp(TcpProxy {
            base,
            remote_port: util::opt_i64_to_u16(row.try_get("remote_port")?),
        }),
        "udp" => ProxyConfig::Udp(UdpProxy {
            base,
            remote_port: util::opt_i64_to_u16(row.try_get("remote_port")?),
        }),
        "http" => ProxyConfig::Http(HttpProxy {
            base,
            custom_domains: util::read_ordered_strings(
                pool,
                "frp_proxy_custom_domains",
                "proxy_id",
                proxy_id,
            )
            .await?,
            subdomain: row.try_get("subdomain")?,
            locations: util::read_ordered_strings(
                pool,
                "frp_proxy_locations",
                "proxy_id",
                proxy_id,
            )
            .await?,
            http_user: row.try_get("http_user")?,
            http_password: row.try_get("http_password")?,
            host_header_rewrite: row.try_get("host_header_rewrite")?,
            request_headers: header_ops(
                util::read_kv(pool, "frp_proxy_request_headers", "proxy_id", proxy_id).await?,
            ),
            response_headers: header_ops(
                util::read_kv(pool, "frp_proxy_response_headers", "proxy_id", proxy_id).await?,
            ),
            route_by_http_user: row.try_get("route_by_http_user")?,
        }),
        "https" => ProxyConfig::Https(HttpsProxy {
            base,
            custom_domains: util::read_ordered_strings(
                pool,
                "frp_proxy_custom_domains",
                "proxy_id",
                proxy_id,
            )
            .await?,
            subdomain: row.try_get("subdomain")?,
        }),
        "tcpmux" => ProxyConfig::Tcpmux(TcpmuxProxy {
            base,
            custom_domains: util::read_ordered_strings(
                pool,
                "frp_proxy_custom_domains",
                "proxy_id",
                proxy_id,
            )
            .await?,
            subdomain: row.try_get("subdomain")?,
            http_user: row.try_get("http_user")?,
            http_password: row.try_get("http_password")?,
            route_by_http_user: row.try_get("route_by_http_user")?,
            multiplexer: row.try_get("multiplexer")?,
        }),
        "stcp" => ProxyConfig::Stcp(StcpProxy {
            base,
            secret_key: row.try_get("secret_key")?,
            allow_users: util::read_ordered_strings(
                pool,
                "frp_proxy_allow_users",
                "proxy_id",
                proxy_id,
            )
            .await?,
        }),
        "sudp" => ProxyConfig::Sudp(SudpProxy {
            base,
            secret_key: row.try_get("secret_key")?,
            allow_users: util::read_ordered_strings(
                pool,
                "frp_proxy_allow_users",
                "proxy_id",
                proxy_id,
            )
            .await?,
        }),
        "xtcp" => ProxyConfig::Xtcp(XtcpProxy {
            base,
            secret_key: row.try_get("secret_key")?,
            allow_users: util::read_ordered_strings(
                pool,
                "frp_proxy_allow_users",
                "proxy_id",
                proxy_id,
            )
            .await?,
            nat_traversal: if util::int_to_bool(row.try_get::<i64, _>("nat_traversal_enabled")?) {
                Some(NatTraversal {
                    disable_assisted_addrs: util::int_to_bool(
                        row.try_get::<i64, _>("nat_disable_assisted_addrs")?,
                    ),
                })
            } else {
                None
            },
        }),
        _ => {
            return Err(AppError::Msg(format!(
                "Unknown frp tunnel type: {proxy_type}"
            )))
        }
    };

    Ok(ProfileProxy {
        id: proxy_id.to_string(),
        enabled: util::int_to_bool(row.try_get::<i64, _>("enabled")?),
        config,
    })
}

async fn read_plugin(
    pool: &SqlitePool,
    row: &SqliteRow,
    proxy_id: &str,
) -> AppResult<Option<PluginOptions>> {
    let Some(plugin_type) = row.try_get::<Option<String>, _>("plugin_type")? else {
        return Ok(None);
    };
    let request_headers = header_ops(
        util::read_kv(
            pool,
            "frp_proxy_plugin_request_headers",
            "proxy_id",
            proxy_id,
        )
        .await?,
    );
    let plugin = match plugin_type.as_str() {
        "http_proxy" => PluginOptions::HttpProxy {
            http_user: row.try_get("plugin_http_user")?,
            http_password: row.try_get("plugin_http_password")?,
        },
        "socks5" => PluginOptions::Socks5 {
            username: row.try_get("plugin_username")?,
            password: row.try_get("plugin_password")?,
        },
        "static_file" => PluginOptions::StaticFile {
            local_path: row.try_get("plugin_local_path")?,
            strip_prefix: row.try_get("plugin_strip_prefix")?,
            http_user: row.try_get("plugin_http_user")?,
            http_password: row.try_get("plugin_http_password")?,
        },
        "unix_domain_socket" => PluginOptions::UnixDomainSocket {
            unix_path: row.try_get("plugin_unix_path")?,
        },
        "http2http" => PluginOptions::Http2Http {
            local_addr: row.try_get("plugin_local_addr")?,
            host_header_rewrite: row.try_get("plugin_host_header_rewrite")?,
            request_headers,
        },
        "http2https" => PluginOptions::Http2Https {
            local_addr: row.try_get("plugin_local_addr")?,
            host_header_rewrite: row.try_get("plugin_host_header_rewrite")?,
            request_headers,
        },
        "https2http" => PluginOptions::Https2Http {
            local_addr: row.try_get("plugin_local_addr")?,
            host_header_rewrite: row.try_get("plugin_host_header_rewrite")?,
            request_headers,
            enable_http2: util::opt_int_to_bool(row.try_get("plugin_enable_http2")?),
            crt_path: row.try_get("plugin_crt_path")?,
            key_path: row.try_get("plugin_key_path")?,
        },
        "https2https" => PluginOptions::Https2Https {
            local_addr: row.try_get("plugin_local_addr")?,
            host_header_rewrite: row.try_get("plugin_host_header_rewrite")?,
            request_headers,
            enable_http2: util::opt_int_to_bool(row.try_get("plugin_enable_http2")?),
            crt_path: row.try_get("plugin_crt_path")?,
            key_path: row.try_get("plugin_key_path")?,
        },
        "tls2raw" => PluginOptions::Tls2Raw {
            local_addr: row.try_get("plugin_local_addr")?,
            crt_path: row.try_get("plugin_crt_path")?,
            key_path: row.try_get("plugin_key_path")?,
        },
        "virtual_net" => PluginOptions::VirtualNet {},
        _ => {
            return Err(AppError::Msg(format!(
                "Unknown frp plugin type: {plugin_type}"
            )))
        }
    };
    Ok(Some(plugin))
}

pub(crate) async fn replace_frp_data(
    tx: &mut Transaction<'_, Sqlite>,
    data: &FrpData,
) -> AppResult<()> {
    delete_frp_tables(tx).await?;
    if has_settings(&data.settings) || !data.profiles.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO frp_settings (id, active_frpc_version, frpc_mirror, updated_at)
            VALUES (1, ?1, ?2, ?3)
            "#,
        )
        .bind(data.settings.active_frpc_version.as_deref())
        .bind(data.settings.frpc_mirror.as_deref())
        .bind(now_ms())
        .execute(&mut **tx)
        .await?;
    }
    for (position, profile) in data.profiles.iter().enumerate() {
        insert_frp_profile(tx, profile, position).await?;
    }
    Ok(())
}

async fn delete_frp_tables(tx: &mut Transaction<'_, Sqlite>) -> AppResult<()> {
    for table in [
        "frp_proxy_annotations",
        "frp_proxy_metadatas",
        "frp_proxy_health_headers",
        "frp_proxy_plugin_request_headers",
        "frp_proxy_response_headers",
        "frp_proxy_request_headers",
        "frp_proxy_allow_users",
        "frp_proxy_locations",
        "frp_proxy_custom_domains",
        "frp_proxies",
        "frp_profile_metadatas",
        "frp_profile_oidc_params",
        "frp_profile_auth_scopes",
        "frp_profiles",
        "frp_settings",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn insert_frp_profile(
    tx: &mut Transaction<'_, Sqlite>,
    profile: &Profile,
    position: usize,
) -> AppResult<()> {
    let server = &profile.server;
    sqlx::query(
        r#"
        INSERT INTO frp_profiles (
            id, position, name, frpc_version, created_at, updated_at,
            user, client_id, server_addr, server_port,
            auth_method, auth_token,
            oidc_client_id, oidc_client_secret, oidc_audience, oidc_scope,
            oidc_token_endpoint_url, oidc_trusted_ca_file, oidc_insecure_skip_verify,
            transport_protocol, transport_dial_server_timeout, transport_dial_server_keepalive,
            transport_connect_server_local_ip, transport_proxy_url, transport_pool_count,
            transport_tcp_mux, transport_tcp_mux_keepalive_interval,
            transport_quic_keepalive_period, transport_quic_max_idle_timeout,
            transport_quic_max_incoming_streams, transport_heartbeat_interval,
            transport_heartbeat_timeout, transport_tls_enable,
            transport_tls_disable_custom_tls_first_byte, transport_tls_cert_file,
            transport_tls_key_file, transport_tls_trusted_ca_file, transport_tls_server_name,
            log_to, log_level, log_max_days, log_disable_print_color,
            web_addr, web_port, web_user, web_password, web_assets_dir, web_pprof_enable,
            web_tls_enabled, web_tls_cert_file, web_tls_key_file, web_tls_trusted_ca_file,
            web_tls_server_name, dns_server, nat_hole_stun_server, login_fail_exit, udp_packet_size
        )
        VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10,
            ?11, ?12,
            ?13, ?14, ?15, ?16,
            ?17, ?18, ?19,
            ?20, ?21, ?22,
            ?23, ?24, ?25,
            ?26, ?27,
            ?28, ?29,
            ?30, ?31,
            ?32, ?33,
            ?34, ?35,
            ?36, ?37, ?38,
            ?39, ?40, ?41, ?42,
            ?43, ?44, ?45, ?46, ?47, ?48,
            ?49, ?50, ?51, ?52,
            ?53, ?54, ?55, ?56, ?57
        )
        "#,
    )
    .bind(&profile.id)
    .bind(position as i64)
    .bind(&profile.name)
    .bind(profile.frpc_version.as_deref())
    .bind(profile.created_at)
    .bind(profile.updated_at)
    .bind(&server.user)
    .bind(&server.client_id)
    .bind(&server.server_addr)
    .bind(i64::from(server.server_port))
    .bind(util::enum_to_db(&server.auth.method)?)
    .bind(&server.auth.token)
    .bind(&server.auth.oidc.client_id)
    .bind(&server.auth.oidc.client_secret)
    .bind(&server.auth.oidc.audience)
    .bind(&server.auth.oidc.scope)
    .bind(&server.auth.oidc.token_endpoint_url)
    .bind(&server.auth.oidc.trusted_ca_file)
    .bind(util::bool_to_i64(server.auth.oidc.insecure_skip_verify))
    .bind(&server.transport.protocol)
    .bind(server.transport.dial_server_timeout)
    .bind(server.transport.dial_server_keepalive)
    .bind(&server.transport.connect_server_local_ip)
    .bind(&server.transport.proxy_url)
    .bind(server.transport.pool_count)
    .bind(util::opt_bool_to_i64(server.transport.tcp_mux))
    .bind(server.transport.tcp_mux_keepalive_interval)
    .bind(server.transport.quic.keepalive_period)
    .bind(server.transport.quic.max_idle_timeout)
    .bind(server.transport.quic.max_incoming_streams)
    .bind(server.transport.heartbeat_interval)
    .bind(server.transport.heartbeat_timeout)
    .bind(util::opt_bool_to_i64(server.transport.tls.enable))
    .bind(util::opt_bool_to_i64(
        server.transport.tls.disable_custom_tls_first_byte,
    ))
    .bind(&server.transport.tls.files.cert_file)
    .bind(&server.transport.tls.files.key_file)
    .bind(&server.transport.tls.files.trusted_ca_file)
    .bind(&server.transport.tls.files.server_name)
    .bind(&server.log.to)
    .bind(&server.log.level)
    .bind(server.log.max_days)
    .bind(util::bool_to_i64(server.log.disable_print_color))
    .bind(&server.web_server.addr)
    .bind(i64::from(server.web_server.port))
    .bind(&server.web_server.user)
    .bind(&server.web_server.password)
    .bind(&server.web_server.assets_dir)
    .bind(util::bool_to_i64(server.web_server.pprof_enable))
    .bind(util::bool_to_i64(server.web_server.tls.is_some()))
    .bind(
        server
            .web_server
            .tls
            .as_ref()
            .map(|tls| tls.cert_file.as_str())
            .unwrap_or(""),
    )
    .bind(
        server
            .web_server
            .tls
            .as_ref()
            .map(|tls| tls.key_file.as_str())
            .unwrap_or(""),
    )
    .bind(
        server
            .web_server
            .tls
            .as_ref()
            .map(|tls| tls.trusted_ca_file.as_str())
            .unwrap_or(""),
    )
    .bind(
        server
            .web_server
            .tls
            .as_ref()
            .map(|tls| tls.server_name.as_str())
            .unwrap_or(""),
    )
    .bind(&server.dns_server)
    .bind(&server.nat_hole_stun_server)
    .bind(util::opt_bool_to_i64(server.login_fail_exit))
    .bind(server.udp_packet_size)
    .execute(&mut **tx)
    .await?;

    util::insert_ordered_strings(
        tx,
        "frp_profile_auth_scopes",
        "profile_id",
        &profile.id,
        &server.auth.additional_scopes,
    )
    .await?;
    util::insert_kv(
        tx,
        "frp_profile_oidc_params",
        "profile_id",
        &profile.id,
        server.auth.oidc.additional_endpoint_params.as_ref(),
    )
    .await?;
    util::insert_kv(
        tx,
        "frp_profile_metadatas",
        "profile_id",
        &profile.id,
        server.metadatas.as_ref(),
    )
    .await?;
    for (proxy_position, proxy) in profile.proxies.iter().enumerate() {
        insert_frp_proxy(tx, &profile.id, proxy, proxy_position).await?;
    }
    Ok(())
}

async fn insert_frp_proxy(
    tx: &mut Transaction<'_, Sqlite>,
    profile_id: &str,
    proxy: &ProfileProxy,
    position: usize,
) -> AppResult<()> {
    let fields = proxy_fields(proxy);
    sqlx::query(
        r#"
        INSERT INTO frp_proxies (
            id, profile_id, position, enabled, proxy_type, name, local_ip, local_port,
            remote_port, subdomain, secret_key, multiplexer, http_user, http_password,
            host_header_rewrite, route_by_http_user,
            transport_use_encryption, transport_use_compression, transport_bandwidth_limit,
            transport_bandwidth_limit_mode, transport_proxy_protocol_version,
            health_check_type, health_check_timeout_seconds, health_check_max_failed,
            health_check_interval_seconds, health_check_path,
            load_balancer_group, load_balancer_group_key,
            nat_traversal_enabled, nat_disable_assisted_addrs,
            plugin_type, plugin_http_user, plugin_http_password, plugin_username, plugin_password,
            plugin_local_path, plugin_strip_prefix, plugin_unix_path, plugin_local_addr,
            plugin_host_header_rewrite, plugin_enable_http2, plugin_crt_path, plugin_key_path
        )
        VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16,
            ?17, ?18, ?19,
            ?20, ?21,
            ?22, ?23, ?24,
            ?25, ?26,
            ?27, ?28,
            ?29, ?30,
            ?31, ?32, ?33, ?34, ?35,
            ?36, ?37, ?38, ?39,
            ?40, ?41, ?42, ?43
        )
        "#,
    )
    .bind(&proxy.id)
    .bind(profile_id)
    .bind(position as i64)
    .bind(util::bool_to_i64(proxy.enabled))
    .bind(fields.proxy_type)
    .bind(&fields.base.name)
    .bind(&fields.base.local_ip)
    .bind(fields.base.local_port.map(i64::from))
    .bind(fields.remote_port.map(i64::from))
    .bind(fields.subdomain)
    .bind(fields.secret_key)
    .bind(fields.multiplexer)
    .bind(fields.http_user)
    .bind(fields.http_password)
    .bind(fields.host_header_rewrite)
    .bind(fields.route_by_http_user)
    .bind(util::bool_to_i64(fields.base.transport.use_encryption))
    .bind(util::bool_to_i64(fields.base.transport.use_compression))
    .bind(&fields.base.transport.bandwidth_limit)
    .bind(&fields.base.transport.bandwidth_limit_mode)
    .bind(&fields.base.transport.proxy_protocol_version)
    .bind(&fields.base.health_check.r#type)
    .bind(fields.base.health_check.timeout_seconds)
    .bind(fields.base.health_check.max_failed)
    .bind(fields.base.health_check.interval_seconds)
    .bind(&fields.base.health_check.path)
    .bind(&fields.base.load_balancer.group)
    .bind(&fields.base.load_balancer.group_key)
    .bind(util::bool_to_i64(fields.nat_traversal.is_some()))
    .bind(util::bool_to_i64(
        fields
            .nat_traversal
            .as_ref()
            .map(|nat| nat.disable_assisted_addrs)
            .unwrap_or(false),
    ))
    .bind(fields.plugin.plugin_type)
    .bind(fields.plugin.http_user)
    .bind(fields.plugin.http_password)
    .bind(fields.plugin.username)
    .bind(fields.plugin.password)
    .bind(fields.plugin.local_path)
    .bind(fields.plugin.strip_prefix)
    .bind(fields.plugin.unix_path)
    .bind(fields.plugin.local_addr)
    .bind(fields.plugin.host_header_rewrite)
    .bind(util::opt_bool_to_i64(fields.plugin.enable_http2))
    .bind(fields.plugin.crt_path)
    .bind(fields.plugin.key_path)
    .execute(&mut **tx)
    .await?;

    util::insert_ordered_strings(
        tx,
        "frp_proxy_custom_domains",
        "proxy_id",
        &proxy.id,
        &fields.custom_domains,
    )
    .await?;
    util::insert_ordered_strings(
        tx,
        "frp_proxy_locations",
        "proxy_id",
        &proxy.id,
        &fields.locations,
    )
    .await?;
    util::insert_ordered_strings(
        tx,
        "frp_proxy_allow_users",
        "proxy_id",
        &proxy.id,
        &fields.allow_users,
    )
    .await?;
    util::insert_kv(
        tx,
        "frp_proxy_request_headers",
        "proxy_id",
        &proxy.id,
        fields
            .request_headers
            .as_ref()
            .and_then(|ops| ops.set.as_ref()),
    )
    .await?;
    util::insert_kv(
        tx,
        "frp_proxy_response_headers",
        "proxy_id",
        &proxy.id,
        fields
            .response_headers
            .as_ref()
            .and_then(|ops| ops.set.as_ref()),
    )
    .await?;
    util::insert_kv(
        tx,
        "frp_proxy_plugin_request_headers",
        "proxy_id",
        &proxy.id,
        fields
            .plugin
            .request_headers
            .as_ref()
            .and_then(|ops| ops.set.as_ref()),
    )
    .await?;
    insert_http_headers(tx, &proxy.id, &fields.base.health_check.http_headers).await?;
    util::insert_kv(
        tx,
        "frp_proxy_metadatas",
        "proxy_id",
        &proxy.id,
        fields.base.metadatas.as_ref(),
    )
    .await?;
    util::insert_kv(
        tx,
        "frp_proxy_annotations",
        "proxy_id",
        &proxy.id,
        fields.base.annotations.as_ref(),
    )
    .await?;
    Ok(())
}

struct ProxyFields<'a> {
    proxy_type: &'static str,
    base: &'a ProxyBase,
    remote_port: Option<u16>,
    custom_domains: Vec<String>,
    subdomain: &'a str,
    locations: Vec<String>,
    secret_key: &'a str,
    allow_users: Vec<String>,
    multiplexer: &'a str,
    http_user: &'a str,
    http_password: &'a str,
    host_header_rewrite: &'a str,
    request_headers: Option<&'a HeaderOps>,
    response_headers: Option<&'a HeaderOps>,
    route_by_http_user: &'a str,
    nat_traversal: Option<&'a NatTraversal>,
    plugin: PluginFields<'a>,
}

#[derive(Default)]
struct PluginFields<'a> {
    plugin_type: Option<&'static str>,
    http_user: &'a str,
    http_password: &'a str,
    username: &'a str,
    password: &'a str,
    local_path: &'a str,
    strip_prefix: &'a str,
    unix_path: &'a str,
    local_addr: &'a str,
    host_header_rewrite: &'a str,
    request_headers: Option<&'a HeaderOps>,
    enable_http2: Option<bool>,
    crt_path: &'a str,
    key_path: &'a str,
}

fn proxy_fields(proxy: &ProfileProxy) -> ProxyFields<'_> {
    let empty = "";
    let (
        proxy_type,
        base,
        remote_port,
        custom_domains,
        subdomain,
        locations,
        secret_key,
        allow_users,
        multiplexer,
        http_user,
        http_password,
        host_header_rewrite,
        request_headers,
        response_headers,
        route_by_http_user,
        nat_traversal,
    ) = match &proxy.config {
        ProxyConfig::Tcp(item) => (
            "tcp",
            &item.base,
            item.remote_port,
            Vec::new(),
            empty,
            Vec::new(),
            empty,
            Vec::new(),
            empty,
            empty,
            empty,
            empty,
            None,
            None,
            empty,
            None,
        ),
        ProxyConfig::Udp(item) => (
            "udp",
            &item.base,
            item.remote_port,
            Vec::new(),
            empty,
            Vec::new(),
            empty,
            Vec::new(),
            empty,
            empty,
            empty,
            empty,
            None,
            None,
            empty,
            None,
        ),
        ProxyConfig::Http(item) => (
            "http",
            &item.base,
            None,
            item.custom_domains.clone(),
            item.subdomain.as_str(),
            item.locations.clone(),
            empty,
            Vec::new(),
            empty,
            item.http_user.as_str(),
            item.http_password.as_str(),
            item.host_header_rewrite.as_str(),
            item.request_headers.as_ref(),
            item.response_headers.as_ref(),
            item.route_by_http_user.as_str(),
            None,
        ),
        ProxyConfig::Https(item) => (
            "https",
            &item.base,
            None,
            item.custom_domains.clone(),
            item.subdomain.as_str(),
            Vec::new(),
            empty,
            Vec::new(),
            empty,
            empty,
            empty,
            empty,
            None,
            None,
            empty,
            None,
        ),
        ProxyConfig::Tcpmux(item) => (
            "tcpmux",
            &item.base,
            None,
            item.custom_domains.clone(),
            item.subdomain.as_str(),
            Vec::new(),
            empty,
            Vec::new(),
            item.multiplexer.as_str(),
            item.http_user.as_str(),
            item.http_password.as_str(),
            empty,
            None,
            None,
            item.route_by_http_user.as_str(),
            None,
        ),
        ProxyConfig::Stcp(item) => (
            "stcp",
            &item.base,
            None,
            Vec::new(),
            empty,
            Vec::new(),
            item.secret_key.as_str(),
            item.allow_users.clone(),
            empty,
            empty,
            empty,
            empty,
            None,
            None,
            empty,
            None,
        ),
        ProxyConfig::Sudp(item) => (
            "sudp",
            &item.base,
            None,
            Vec::new(),
            empty,
            Vec::new(),
            item.secret_key.as_str(),
            item.allow_users.clone(),
            empty,
            empty,
            empty,
            empty,
            None,
            None,
            empty,
            None,
        ),
        ProxyConfig::Xtcp(item) => (
            "xtcp",
            &item.base,
            None,
            Vec::new(),
            empty,
            Vec::new(),
            item.secret_key.as_str(),
            item.allow_users.clone(),
            empty,
            empty,
            empty,
            empty,
            None,
            None,
            empty,
            item.nat_traversal.as_ref(),
        ),
    };

    ProxyFields {
        proxy_type,
        base,
        remote_port,
        custom_domains,
        subdomain,
        locations,
        secret_key,
        allow_users,
        multiplexer,
        http_user,
        http_password,
        host_header_rewrite,
        request_headers,
        response_headers,
        route_by_http_user,
        nat_traversal,
        plugin: plugin_fields(base.plugin.as_ref()),
    }
}

fn plugin_fields(plugin: Option<&PluginOptions>) -> PluginFields<'_> {
    let empty = "";
    match plugin {
        Some(PluginOptions::HttpProxy {
            http_user,
            http_password,
        }) => PluginFields {
            plugin_type: Some("http_proxy"),
            http_user,
            http_password,
            ..Default::default()
        },
        Some(PluginOptions::Socks5 { username, password }) => PluginFields {
            plugin_type: Some("socks5"),
            username,
            password,
            ..Default::default()
        },
        Some(PluginOptions::StaticFile {
            local_path,
            strip_prefix,
            http_user,
            http_password,
        }) => PluginFields {
            plugin_type: Some("static_file"),
            local_path,
            strip_prefix,
            http_user,
            http_password,
            ..Default::default()
        },
        Some(PluginOptions::UnixDomainSocket { unix_path }) => PluginFields {
            plugin_type: Some("unix_domain_socket"),
            unix_path,
            ..Default::default()
        },
        Some(PluginOptions::Http2Http {
            local_addr,
            host_header_rewrite,
            request_headers,
        }) => PluginFields {
            plugin_type: Some("http2http"),
            local_addr,
            host_header_rewrite,
            request_headers: request_headers.as_ref(),
            ..Default::default()
        },
        Some(PluginOptions::Http2Https {
            local_addr,
            host_header_rewrite,
            request_headers,
        }) => PluginFields {
            plugin_type: Some("http2https"),
            local_addr,
            host_header_rewrite,
            request_headers: request_headers.as_ref(),
            ..Default::default()
        },
        Some(PluginOptions::Https2Http {
            local_addr,
            host_header_rewrite,
            request_headers,
            enable_http2,
            crt_path,
            key_path,
        }) => PluginFields {
            plugin_type: Some("https2http"),
            local_addr,
            host_header_rewrite,
            request_headers: request_headers.as_ref(),
            enable_http2: *enable_http2,
            crt_path,
            key_path,
            ..Default::default()
        },
        Some(PluginOptions::Https2Https {
            local_addr,
            host_header_rewrite,
            request_headers,
            enable_http2,
            crt_path,
            key_path,
        }) => PluginFields {
            plugin_type: Some("https2https"),
            local_addr,
            host_header_rewrite,
            request_headers: request_headers.as_ref(),
            enable_http2: *enable_http2,
            crt_path,
            key_path,
            ..Default::default()
        },
        Some(PluginOptions::Tls2Raw {
            local_addr,
            crt_path,
            key_path,
        }) => PluginFields {
            plugin_type: Some("tls2raw"),
            local_addr,
            crt_path,
            key_path,
            ..Default::default()
        },
        Some(PluginOptions::VirtualNet {}) => PluginFields {
            plugin_type: Some("virtual_net"),
            ..Default::default()
        },
        None => PluginFields {
            http_user: empty,
            http_password: empty,
            username: empty,
            password: empty,
            local_path: empty,
            strip_prefix: empty,
            unix_path: empty,
            local_addr: empty,
            host_header_rewrite: empty,
            crt_path: empty,
            key_path: empty,
            ..Default::default()
        },
    }
}

async fn read_http_headers(pool: &SqlitePool, proxy_id: &str) -> AppResult<Vec<HttpHeader>> {
    let rows = sqlx::query(
        "SELECT name, value FROM frp_proxy_health_headers WHERE proxy_id = ?1 ORDER BY position ASC",
    )
    .bind(proxy_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(HttpHeader {
                name: row.try_get("name")?,
                value: row.try_get("value")?,
            })
        })
        .collect()
}

async fn insert_http_headers(
    tx: &mut Transaction<'_, Sqlite>,
    proxy_id: &str,
    headers: &[HttpHeader],
) -> AppResult<()> {
    for (position, header) in headers.iter().enumerate() {
        sqlx::query(
            "INSERT INTO frp_proxy_health_headers (proxy_id, position, name, value) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(proxy_id)
        .bind(position as i64)
        .bind(&header.name)
        .bind(&header.value)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn header_ops(map: Map) -> Option<HeaderOps> {
    if map.is_empty() {
        None
    } else {
        Some(HeaderOps { set: Some(map) })
    }
}
