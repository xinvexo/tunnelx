/** 单个 frpc 实例的运行状态（对应后端 FrpcStatus）。 */
export type FrpcStatus = 'stopped' | 'starting' | 'running' | 'warning' | 'stopping' | 'errored'

/** 已连接态（运行中，或带警告但仍在跑）。接受 undefined，便于直接传可能缺省的状态值。 */
export function isLiveStatus(status: FrpcStatus | string | undefined): boolean {
    return status === 'running' || status === 'warning'
}

/** 终止态（已停止或出错）。 */
export function isTerminalStatus(status: FrpcStatus | string | undefined): boolean {
    return status === 'stopped' || status === 'errored'
}

/** 版本列表条目：远端可下载 + 本地已安装的并集。 */
export interface RemoteVersion {
    version: string
    tag: string
    publishedAt: string
    assetName: string | null
    downloadUrl: string | null
    size: number | null
    installed: boolean
    active: boolean
}

export interface VersionList {
    versions: RemoteVersion[]
    remoteError: string | null
}

/** 下载进度事件载荷。 */
export interface DownloadProgress {
    version: string
    received: number
    total: number
}

/** frpc 状态变化事件载荷。 */
export interface FrpcStatusChange {
    profileId: string
    status: FrpcStatus
}

export interface FrpcConfigDirty {
    profileId: string
    needsRestart: boolean
}

/** 单条隧道的实时状态，来自 frpc admin API /api/status（结构化，比解析日志可靠）。 */
export interface ProxyStatusInfo {
    name: string
    /** frpc 上报的隧道状态：running / wait start / start error / check failed 等。 */
    status: string
    /** 隧道类型（tcp/udp/http/...）。 */
    kind: string
    /** GUI 标准化后的第一个公网访问地址，兼容旧调用。 */
    publicAddr: string
    /** GUI 标准化后的公网访问地址列表。 */
    publicAddrs: string[]
    /** TCP/UDP 动态端口等场景下 frps status 返回的最终远端端口。 */
    remotePort: number | null
    /** 公网映射地址（如 1.2.3.4:6000 或 http 域名）；仅 running 时通常非空。 */
    remoteAddr: string
    /** 本地目标地址（如 127.0.0.1:8080）。 */
    localAddr: string
    /** 出错时的错误描述，正常运行为空。 */
    err: string
}

export interface TrafficSample {
    timestamp: number
    proxyName: string
    downloadBytes: number
    uploadBytes: number
    downloadSpeed: number
    uploadSpeed: number
}

export interface TrafficStats {
    proxyName: string
    proxyType: string
    realLocalIP: string
    realLocalPort: number
    statsLocalIP: string
    statsLocalPort: number
    downloadBytes: number
    uploadBytes: number
    downloadSpeed: number
    uploadSpeed: number
    totalBytes: number
    lastUpdatedAt: number
    history: TrafficSample[]
}
