// UI 下拉/分段选择用的选项常量。

export interface Opt<T = string> {
    label: string
    value: T
    desc?: string
    labelKey?: string
    descKey?: string
}

export const PROXY_TYPES: Opt[] = [
    { label: 'TCP', value: 'tcp', descKey: 'frpOptions.proxyTypes.tcp' },
    { label: 'UDP', value: 'udp', descKey: 'frpOptions.proxyTypes.udp' },
    { label: 'HTTP', value: 'http', descKey: 'frpOptions.proxyTypes.http' },
    { label: 'HTTPS', value: 'https', descKey: 'frpOptions.proxyTypes.https' },
    { label: 'TCPMUX', value: 'tcpmux', descKey: 'frpOptions.proxyTypes.tcpmux' },
    { label: 'STCP', value: 'stcp', descKey: 'frpOptions.proxyTypes.stcp' },
    { label: 'SUDP', value: 'sudp', descKey: 'frpOptions.proxyTypes.sudp' },
    { label: 'XTCP', value: 'xtcp', descKey: 'frpOptions.proxyTypes.xtcp' },
]

export const PROTOCOLS: Opt[] = [
    { label: 'TCP', value: 'tcp' },
    { label: 'KCP', value: 'kcp' },
    { label: 'QUIC', value: 'quic' },
    { label: 'WebSocket', value: 'websocket' },
    { label: 'WSS', value: 'wss' },
]

export const AUTH_METHODS: Opt[] = [
    { label: 'Token', value: 'token' },
    { label: 'OIDC', value: 'oidc' },
]

export const LOG_LEVELS: Opt[] = [
    { label: 'trace', value: 'trace' },
    { label: 'debug', value: 'debug' },
    { label: 'info', value: 'info' },
    { label: 'warn', value: 'warn' },
    { label: 'error', value: 'error' },
]

export const BANDWIDTH_MODES: Opt[] = [
    { label: 'Client', value: 'client', labelKey: 'proxyEditor.bandwidthMode.client' },
    { label: 'Server', value: 'server', labelKey: 'proxyEditor.bandwidthMode.server' },
]

export const PROXY_PROTOCOL_VERSIONS: Opt[] = [
    { label: 'Disabled', value: '', labelKey: 'proxyEditor.proxyProtocol.none' },
    { label: 'v1', value: 'v1' },
    { label: 'v2', value: 'v2' },
]

export const HEALTH_CHECK_TYPES: Opt[] = [
    { label: 'Off', value: '', labelKey: 'proxyEditor.health.off' },
    { label: 'TCP', value: 'tcp' },
    { label: 'HTTP', value: 'http' },
]

export const ADDITIONAL_SCOPES: Opt[] = [
    { label: 'HeartBeats', value: 'HeartBeats' },
    { label: 'NewWorkConns', value: 'NewWorkConns' },
]

export const MULTIPLEXERS: Opt[] = [{ label: 'httpconnect', value: 'httpconnect' }]

export const PLUGIN_TYPES: Opt[] = [
    { label: 'No plugin', value: '', labelKey: 'pluginTypes.none' },
    { label: 'HTTP proxy', value: 'http_proxy', labelKey: 'pluginTypes.httpProxy' },
    { label: 'SOCKS5', value: 'socks5' },
    { label: 'Static file', value: 'static_file', labelKey: 'pluginTypes.staticFile' },
    { label: 'Unix Socket', value: 'unix_domain_socket' },
    { label: 'http2http', value: 'http2http' },
    { label: 'http2https', value: 'http2https' },
    { label: 'https2http', value: 'https2http' },
    { label: 'https2https', value: 'https2https' },
    { label: 'tls2raw', value: 'tls2raw' },
    { label: 'virtual_net', value: 'virtual_net' },
]

export function localizeOptions<T = string>(
    options: Opt<T>[],
    t: (key: string) => string,
): Opt<T>[] {
    return options.map((item) => ({
        ...item,
        label: item.labelKey ? t(item.labelKey) : item.label,
        desc: item.descKey ? t(item.descKey) : item.desc,
    }))
}
