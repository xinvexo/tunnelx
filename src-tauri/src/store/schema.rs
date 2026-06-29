use tauri::plugin::TauriPlugin;
use tauri::Runtime;
use tauri_plugin_sql::{Migration, MigrationKind};

pub(crate) const DB_FILE: &str = "tunnelx-data.sqlite3";
pub(crate) const DB_URL: &str = "sqlite:tunnelx-data.sqlite3";
pub(crate) const SAVE_BLOCKED_MSG: &str =
    "Data storage is in read-only protection mode because the database looked damaged at startup and was backed up. Changes in this session will not be saved. Restore the database from backup, or delete tunnelx-data.sqlite3.recovery-required after confirming recovery is not needed, then restart TunnelX";

pub(crate) const INIT_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS app_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    theme TEXT,
    silent_start INTEGER NOT NULL DEFAULT 0,
    auto_connect INTEGER NOT NULL DEFAULT 0,
    lightweight_mode INTEGER NOT NULL DEFAULT 0,
    auto_update INTEGER NOT NULL DEFAULT 0,
    traffic_stats_enabled INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS connection_order (
    position INTEGER PRIMARY KEY,
    connection_key TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS frp_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    active_frpc_version TEXT,
    frpc_mirror TEXT,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS frp_profiles (
    id TEXT PRIMARY KEY,
    position INTEGER NOT NULL,
    name TEXT NOT NULL,
    frpc_version TEXT,
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0,
    user TEXT NOT NULL DEFAULT '',
    client_id TEXT NOT NULL DEFAULT '',
    server_addr TEXT NOT NULL DEFAULT '',
    server_port INTEGER NOT NULL DEFAULT 0,
    auth_method TEXT NOT NULL DEFAULT 'token',
    auth_token TEXT NOT NULL DEFAULT '',
    oidc_client_id TEXT NOT NULL DEFAULT '',
    oidc_client_secret TEXT NOT NULL DEFAULT '',
    oidc_audience TEXT NOT NULL DEFAULT '',
    oidc_scope TEXT NOT NULL DEFAULT '',
    oidc_token_endpoint_url TEXT NOT NULL DEFAULT '',
    oidc_trusted_ca_file TEXT NOT NULL DEFAULT '',
    oidc_insecure_skip_verify INTEGER NOT NULL DEFAULT 0,
    transport_protocol TEXT NOT NULL DEFAULT '',
    transport_dial_server_timeout INTEGER,
    transport_dial_server_keepalive INTEGER,
    transport_connect_server_local_ip TEXT NOT NULL DEFAULT '',
    transport_proxy_url TEXT NOT NULL DEFAULT '',
    transport_pool_count INTEGER,
    transport_tcp_mux INTEGER,
    transport_tcp_mux_keepalive_interval INTEGER,
    transport_quic_keepalive_period INTEGER,
    transport_quic_max_idle_timeout INTEGER,
    transport_quic_max_incoming_streams INTEGER,
    transport_heartbeat_interval INTEGER,
    transport_heartbeat_timeout INTEGER,
    transport_tls_enable INTEGER,
    transport_tls_disable_custom_tls_first_byte INTEGER,
    transport_tls_cert_file TEXT NOT NULL DEFAULT '',
    transport_tls_key_file TEXT NOT NULL DEFAULT '',
    transport_tls_trusted_ca_file TEXT NOT NULL DEFAULT '',
    transport_tls_server_name TEXT NOT NULL DEFAULT '',
    log_to TEXT NOT NULL DEFAULT '',
    log_level TEXT NOT NULL DEFAULT '',
    log_max_days INTEGER,
    log_disable_print_color INTEGER NOT NULL DEFAULT 0,
    web_addr TEXT NOT NULL DEFAULT '',
    web_port INTEGER NOT NULL DEFAULT 0,
    web_user TEXT NOT NULL DEFAULT '',
    web_password TEXT NOT NULL DEFAULT '',
    web_assets_dir TEXT NOT NULL DEFAULT '',
    web_pprof_enable INTEGER NOT NULL DEFAULT 0,
    web_tls_enabled INTEGER NOT NULL DEFAULT 0,
    web_tls_cert_file TEXT NOT NULL DEFAULT '',
    web_tls_key_file TEXT NOT NULL DEFAULT '',
    web_tls_trusted_ca_file TEXT NOT NULL DEFAULT '',
    web_tls_server_name TEXT NOT NULL DEFAULT '',
    dns_server TEXT NOT NULL DEFAULT '',
    nat_hole_stun_server TEXT NOT NULL DEFAULT '',
    login_fail_exit INTEGER,
    udp_packet_size INTEGER
);

CREATE TABLE IF NOT EXISTS frp_profile_auth_scopes (
    profile_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (profile_id, position),
    FOREIGN KEY (profile_id) REFERENCES frp_profiles(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_profile_oidc_params (
    profile_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (profile_id, key),
    FOREIGN KEY (profile_id) REFERENCES frp_profiles(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_profile_metadatas (
    profile_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (profile_id, key),
    FOREIGN KEY (profile_id) REFERENCES frp_profiles(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxies (
    id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    proxy_type TEXT NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    local_ip TEXT NOT NULL DEFAULT '',
    local_port INTEGER,
    remote_port INTEGER,
    subdomain TEXT NOT NULL DEFAULT '',
    secret_key TEXT NOT NULL DEFAULT '',
    multiplexer TEXT NOT NULL DEFAULT '',
    http_user TEXT NOT NULL DEFAULT '',
    http_password TEXT NOT NULL DEFAULT '',
    host_header_rewrite TEXT NOT NULL DEFAULT '',
    route_by_http_user TEXT NOT NULL DEFAULT '',
    transport_use_encryption INTEGER NOT NULL DEFAULT 0,
    transport_use_compression INTEGER NOT NULL DEFAULT 0,
    transport_bandwidth_limit TEXT NOT NULL DEFAULT '',
    transport_bandwidth_limit_mode TEXT NOT NULL DEFAULT '',
    transport_proxy_protocol_version TEXT NOT NULL DEFAULT '',
    health_check_type TEXT NOT NULL DEFAULT '',
    health_check_timeout_seconds INTEGER,
    health_check_max_failed INTEGER,
    health_check_interval_seconds INTEGER,
    health_check_path TEXT NOT NULL DEFAULT '',
    load_balancer_group TEXT NOT NULL DEFAULT '',
    load_balancer_group_key TEXT NOT NULL DEFAULT '',
    nat_traversal_enabled INTEGER NOT NULL DEFAULT 0,
    nat_disable_assisted_addrs INTEGER NOT NULL DEFAULT 0,
    plugin_type TEXT,
    plugin_http_user TEXT NOT NULL DEFAULT '',
    plugin_http_password TEXT NOT NULL DEFAULT '',
    plugin_username TEXT NOT NULL DEFAULT '',
    plugin_password TEXT NOT NULL DEFAULT '',
    plugin_local_path TEXT NOT NULL DEFAULT '',
    plugin_strip_prefix TEXT NOT NULL DEFAULT '',
    plugin_unix_path TEXT NOT NULL DEFAULT '',
    plugin_local_addr TEXT NOT NULL DEFAULT '',
    plugin_host_header_rewrite TEXT NOT NULL DEFAULT '',
    plugin_enable_http2 INTEGER,
    plugin_crt_path TEXT NOT NULL DEFAULT '',
    plugin_key_path TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (profile_id) REFERENCES frp_profiles(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_frp_proxies_profile ON frp_proxies(profile_id, position);
CREATE INDEX IF NOT EXISTS idx_frp_proxies_name ON frp_proxies(name);
CREATE INDEX IF NOT EXISTS idx_frp_proxies_local ON frp_proxies(local_ip, local_port);

CREATE TABLE IF NOT EXISTS frp_proxy_custom_domains (
    proxy_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, position),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_locations (
    proxy_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, position),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_allow_users (
    proxy_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, position),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_request_headers (
    proxy_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, key),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_response_headers (
    proxy_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, key),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_plugin_request_headers (
    proxy_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, key),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_health_headers (
    proxy_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    name TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, position),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_metadatas (
    proxy_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, key),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS frp_proxy_annotations (
    proxy_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (proxy_id, key),
    FOREIGN KEY (proxy_id) REFERENCES frp_proxies(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cloudflare_tunnels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    auth_mode TEXT NOT NULL DEFAULT 'authorization',
    api_token TEXT NOT NULL DEFAULT '',
    token_account_id TEXT NOT NULL DEFAULT '',
    token_account_name TEXT NOT NULL DEFAULT '',
    authorization_account_id TEXT NOT NULL DEFAULT '',
    authorization_zone_id TEXT NOT NULL DEFAULT '',
    cert_file TEXT NOT NULL DEFAULT '',
    tunnel_id TEXT NOT NULL DEFAULT '',
    credentials_file TEXT NOT NULL DEFAULT '',
    config_file TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_cloudflare_tunnels_name ON cloudflare_tunnels(name);
CREATE INDEX IF NOT EXISTS idx_cloudflare_tunnels_remote_id ON cloudflare_tunnels(tunnel_id);

CREATE TABLE IF NOT EXISTS cloudflare_ingress (
    id TEXT PRIMARY KEY,
    tunnel_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    hostname TEXT NOT NULL DEFAULT '',
    service TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1,
    dns_routed INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (tunnel_id) REFERENCES cloudflare_tunnels(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_cloudflare_ingress_hostname ON cloudflare_ingress(hostname);

CREATE TABLE IF NOT EXISTS cloudflare_pending_cleanup (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tunnel_local_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    kind TEXT NOT NULL,
    remote_tunnel_id TEXT NOT NULL DEFAULT '',
    tunnel_name TEXT NOT NULL DEFAULT '',
    auth_mode TEXT NOT NULL DEFAULT 'authorization',
    api_token TEXT NOT NULL DEFAULT '',
    token_account_id TEXT NOT NULL DEFAULT '',
    token_account_name TEXT NOT NULL DEFAULT '',
    authorization_account_id TEXT NOT NULL DEFAULT '',
    authorization_zone_id TEXT NOT NULL DEFAULT '',
    cert_file TEXT NOT NULL DEFAULT '',
    credentials_file TEXT NOT NULL DEFAULT '',
    config_file TEXT NOT NULL DEFAULT '',
    FOREIGN KEY (tunnel_local_id) REFERENCES cloudflare_tunnels(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS cloudflare_pending_cleanup_dns (
    cleanup_id INTEGER NOT NULL,
    position INTEGER NOT NULL,
    hostname TEXT NOT NULL,
    PRIMARY KEY (cleanup_id, position),
    FOREIGN KEY (cleanup_id) REFERENCES cloudflare_pending_cleanup(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_cloudflare_pending_dns_hostname ON cloudflare_pending_cleanup_dns(hostname);

CREATE TABLE IF NOT EXISTS ngrok_tunnels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    authtoken TEXT NOT NULL DEFAULT '',
    region TEXT NOT NULL DEFAULT '',
    config_file TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_ngrok_tunnels_name ON ngrok_tunnels(name);

CREATE TABLE IF NOT EXISTS ngrok_endpoints (
    id TEXT PRIMARY KEY,
    tunnel_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    proto TEXT NOT NULL DEFAULT 'http',
    addr TEXT NOT NULL DEFAULT '',
    domain TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (tunnel_id) REFERENCES ngrok_tunnels(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_ngrok_endpoints_domain ON ngrok_endpoints(domain);
"#;

pub(crate) const PROVIDER_EXTENSION_SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS cpolar_tunnels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    authtoken TEXT NOT NULL DEFAULT '',
    region TEXT NOT NULL DEFAULT '',
    config_file TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_cpolar_tunnels_name ON cpolar_tunnels(name);

CREATE TABLE IF NOT EXISTS cpolar_endpoints (
    id TEXT PRIMARY KEY,
    tunnel_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    proto TEXT NOT NULL DEFAULT 'http',
    addr TEXT NOT NULL DEFAULT '',
    hostname TEXT NOT NULL DEFAULT '',
    remote_addr TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (tunnel_id) REFERENCES cpolar_tunnels(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_cpolar_endpoints_hostname ON cpolar_endpoints(hostname);
CREATE INDEX IF NOT EXISTS idx_cpolar_endpoints_remote_addr ON cpolar_endpoints(remote_addr);

CREATE TABLE IF NOT EXISTS pinggy_tunnels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    token TEXT NOT NULL DEFAULT '',
    server TEXT NOT NULL DEFAULT 'free.pinggy.io',
    server_port INTEGER NOT NULL DEFAULT 443,
    debugger_port INTEGER,
    config_file TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_pinggy_tunnels_name ON pinggy_tunnels(name);

CREATE TABLE IF NOT EXISTS pinggy_endpoints (
    id TEXT PRIMARY KEY,
    tunnel_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    name TEXT NOT NULL DEFAULT '',
    tunnel_type TEXT NOT NULL DEFAULT 'http',
    local_addr TEXT NOT NULL DEFAULT '',
    enabled INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (tunnel_id) REFERENCES pinggy_tunnels(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pinggy_endpoints_local_addr ON pinggy_endpoints(local_addr);
"#;

pub(crate) const INGRESS_NAME_SCHEMA_SQL: &str = r#"
ALTER TABLE cloudflare_ingress ADD COLUMN name TEXT NOT NULL DEFAULT '';
"#;

pub fn sql_plugin<R: Runtime>() -> TauriPlugin<R, Option<tauri_plugin_sql::PluginConfig>> {
    tauri_plugin_sql::Builder::new()
        .add_migrations(
            DB_URL,
            vec![
                Migration {
                    version: 1,
                    description: "init_tunnelx_store",
                    sql: INIT_SCHEMA_SQL,
                    kind: MigrationKind::Up,
                },
                Migration {
                    version: 2,
                    description: "add_cpolar_pinggy_provider_store",
                    sql: PROVIDER_EXTENSION_SCHEMA_SQL,
                    kind: MigrationKind::Up,
                },
                Migration {
                    version: 3,
                    description: "add_ingress_item_names",
                    sql: INGRESS_NAME_SCHEMA_SQL,
                    kind: MigrationKind::Up,
                },
            ],
        )
        .build()
}
