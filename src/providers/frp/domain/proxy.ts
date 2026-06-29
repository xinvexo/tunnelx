import { i18n } from '@/i18n'
import { hydratePlugin, type PluginOptions } from './plugin'

export type ProxyType = 'tcp' | 'udp' | 'http' | 'https' | 'tcpmux' | 'stcp' | 'sudp' | 'xtcp'

export interface ProxyConfig {
    type: ProxyType
    name: string
    localIP: string
    localPort: number | null
    remotePort: number | null
    customDomains: string[]
    subdomain: string
    secretKey: string
    multiplexer: string
    httpUser: string
    httpPassword: string
    hostHeaderRewrite: string
    requestHeaders: Record<string, string>
    responseHeaders: Record<string, string>
    locations: string[]
    routeByHTTPUser: string
    allowUsers: string[]
    loadBalancer: { group: string; groupKey: string }
    healthCheck: {
        type: string
        timeoutSeconds: number
        maxFailed: number
        intervalSeconds: number
        path: string
        httpHeaders: { name: string; value: string }[]
    }
    transport: {
        useEncryption: boolean
        useCompression: boolean
        bandwidthLimit: string
        bandwidthLimitMode: string
        proxyProtocolVersion: string
    }
    plugin?: PluginOptions | null
    metadatas: Record<string, string> | null
    annotations: Record<string, string> | null
}

export interface ProfileProxy {
    id: string
    enabled: boolean
    config: ProxyConfig
}

/**
 * 中性默认值：所有「开关型」字段都取关闭/缺省（healthCheck.type=''、transport.bandwidthLimitMode=''、
 * 端口 null）。后端对这些空值做 skip_serializing_if 省略，因此 hydrate 回填它们不会改写
 * 已存储的配置，也不会触发虚假的「待重启」。新建隧道的按类型预填在 newProxyConfig 里做。
 */
export function defaultProxyConfig(type: ProxyType = 'tcp'): ProxyConfig {
    return {
        type,
        name: '',
        localIP: '127.0.0.1',
        localPort: null,
        remotePort: null,
        customDomains: [],
        subdomain: '',
        secretKey: '',
        multiplexer: '',
        httpUser: '',
        httpPassword: '',
        hostHeaderRewrite: '',
        requestHeaders: {},
        responseHeaders: {},
        locations: [],
        routeByHTTPUser: '',
        allowUsers: [],
        loadBalancer: { group: '', groupKey: '' },
        healthCheck: {
            // ''=不启用；秒数是启用后的预填值，type 为空时后端整体省略
            type: '',
            timeoutSeconds: 3,
            maxFailed: 3,
            intervalSeconds: 10,
            path: '',
            httpHeaders: [],
        },
        transport: {
            useEncryption: false,
            useCompression: false,
            bandwidthLimit: '',
            bandwidthLimitMode: '',
            proxyProtocolVersion: '',
        },
        metadatas: null,
        annotations: null,
    }
}

/** 各隧道类型的新建预填（只作用于新建/切换类型，不参与已存配置的 hydrate）。 */
const TYPE_PRESETS: Partial<Record<ProxyType, Partial<ProxyConfig>>> = {
    http: { localPort: 80 },
    https: { localPort: 443 },
    // frp 的 tcpmux 目前只支持 httpconnect，且编辑器没有该字段的输入框，必须预填。
    tcpmux: { multiplexer: 'httpconnect' },
}

export function newProxyConfig(type: ProxyType = 'tcp'): ProxyConfig {
    return { ...defaultProxyConfig(type), ...TYPE_PRESETS[type] }
}

export function newProfileProxy(type: ProxyType = 'tcp'): ProfileProxy {
    return {
        id: crypto.randomUUID(),
        enabled: true,
        config: newProxyConfig(type),
    }
}

export function hydrateProxyConfig(raw: any): ProxyConfig {
    const base = defaultProxyConfig(raw?.type)
    if (!raw) return base
    return {
        ...base,
        ...raw,
        // 0 不是合法的本地/远端端口（曾有版本把默认 0 写进存档）：归一成 null，
        // 序列化时按「未设置」省略，恢复 frp 期望的形态。
        localPort: raw.localPort || null,
        remotePort: raw.remotePort || null,
        customDomains: raw.customDomains ?? [],
        requestHeaders: raw.requestHeaders ?? {},
        responseHeaders: raw.responseHeaders ?? {},
        locations: raw.locations ?? [],
        allowUsers: raw.allowUsers ?? [],
        loadBalancer: { ...base.loadBalancer, ...raw.loadBalancer },
        healthCheck: { ...base.healthCheck, ...raw.healthCheck },
        transport: { ...base.transport, ...raw.transport },
        // 后端会省略插件里的空字段（skip_serializing_if），编辑器按「键是否存在」决定
        // 渲染哪些输入框，必须补全该插件类型的全部键，否则字段静默消失。
        plugin: raw.plugin ? hydratePlugin(raw.plugin) : raw.plugin,
        metadatas: raw.metadatas ?? null,
        annotations: raw.annotations ?? null,
    }
}

export function hydrateProfileProxy(raw: any): ProfileProxy {
    return {
        id: raw?.id ?? crypto.randomUUID(),
        enabled: raw?.enabled ?? true,
        config: hydrateProxyConfig(raw?.config),
    }
}

/**
 * 列表里展示的「远端」描述的数据结构。
 * Domain 层只返回结构化数据，UI 层负责国际化翻译。
 */
export type RemoteSummaryData =
    | { kind: 'port'; port: number }
    | { kind: 'dynamicPort' }
    | { kind: 'domains'; domains: string[] }
    | { kind: 'secret'; set: boolean }

/** 返回远端配置的结构化数据，供 UI 层格式化 */
export function remoteSummaryData(c: ProxyConfig): RemoteSummaryData {
    switch (c.type) {
        case 'tcp':
        case 'udp':
            return c.remotePort ? { kind: 'port', port: c.remotePort } : { kind: 'dynamicPort' }
        case 'http':
        case 'https':
        case 'tcpmux':
            const domains = [...c.customDomains, ...(c.subdomain ? [c.subdomain] : [])]
            return { kind: 'domains', domains }
        case 'stcp':
        case 'sudp':
        case 'xtcp':
            return { kind: 'secret', set: !!c.secretKey }
        default:
            return { kind: 'dynamicPort' }
    }
}

/** 列表里展示的「远端」描述（带 i18n）。仅在 UI 渲染时调用，保证 i18n 已初始化。 */
export function remoteSummary(c: ProxyConfig): string {
    const data = remoteSummaryData(c)
    const t = i18n.global.t

    switch (data.kind) {
        case 'port':
            return `:${data.port}`
        case 'dynamicPort':
            return t('proxySummary.dynamicPort')
        case 'domains':
            return data.domains.join(', ') || '—'
        case 'secret':
            return data.set ? t('proxySummary.secretSet') : t('proxySummary.secretUnset')
    }
}

/** 列表里展示的「本地」描述。 */
export function localSummary(c: ProxyConfig): string {
    if (c.plugin) return `plugin: ${c.plugin.type}`
    const ip = c.localIP || '127.0.0.1'
    return c.localPort ? `${ip}:${c.localPort}` : ip
}
