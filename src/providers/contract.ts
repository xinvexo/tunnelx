export const FRP_PROVIDER_ID = 'frp'
export const CLOUDFLARE_PROVIDER_ID = 'cloudflare'
export const NGROK_PROVIDER_ID = 'ngrok'
export const CPOLAR_PROVIDER_ID = 'cpolar'
export const PINGGY_PROVIDER_ID = 'pinggy'

export interface ProviderCapabilities {
    accountLogin: boolean
    namedTunnels: boolean
    credentials: boolean
    dnsRoutes: boolean
    ingress: boolean
    localRuntime: boolean
    runtimeMetrics: boolean
    memoryStats: boolean
    trafficStats: boolean
    versionManagement: boolean
}

export interface ProviderDescriptor {
    id: string
    name: string
    summary: string
    capabilities: ProviderCapabilities
}

export interface ProviderStatus {
    providerId: string
    available: boolean
    version: string | null
    message: string
    details: unknown
}

export const PROVIDER_RUNTIME_INSTALL_PROGRESS_EVENT = 'provider-runtime-install-progress'

export interface ProviderRuntimeInstallProgress {
    providerId: string
    runtime: string
    receivedBytes: number
    totalBytes: number | null
    percent: number | null
}

export interface ProviderRuntimeUpdateStatus {
    providerId: string
    runtime: string
    currentVersion: string | null
    latestVersion: string | null
    updateAvailable: boolean
    message: string
}

export interface ProviderCommandOutput {
    success: boolean
    stdout: string
    stderr: string
}

export interface CreateTunnelInput {
    name: string
}

export interface IngressRule {
    id: string
    name: string
    hostname: string
    service: string
    enabled: boolean
    dnsRouted: boolean
}

export interface TunnelResource {
    id: string
    providerId: string
    name: string
    providerTunnelId: string
    credentialsRef: string
    configFile: string
    ingress: IngressRule[]
    createdAt: number
    updatedAt: number
    metadata: Record<string, unknown>
}

export interface ProviderTunnelsUpdatedEvent {
    providerId: string
}

export type TunnelRuntimeState = 'stopped' | 'starting' | 'running' | 'warning' | 'stopping' | 'errored'

export interface TunnelRuntimeInfo {
    providerId: string
    tunnelId: string
    status: TunnelRuntimeState
    pid: number | null
    message: string
    details: Record<string, unknown>
}

export interface TunnelRuntimeStatusEvent {
    info: TunnelRuntimeInfo
}

export interface TunnelRuntimeLogEvent {
    providerId: string
    tunnelId: string
    line: string
    reset?: boolean
}

export interface WatchdogLogEvent {
    line: string
    level: 'info' | 'warning' | 'danger'
    important: boolean
    code?: string
}

export interface MetricSample {
    timestamp: number
    tunnelId: string
    tunnelName: string
    downloadBytes: number
    uploadBytes: number
    downloadSpeed: number
    uploadSpeed: number
}

export interface TunnelMetrics {
    tunnelId: string
    tunnelName: string
    tunnelType: string
    memoryBytes: number | null
    downloadBytes: number
    uploadBytes: number
    downloadSpeed: number
    uploadSpeed: number
    totalBytes: number
    lastUpdatedAt: number
    history: MetricSample[]
    metadata: Record<string, unknown>
}

export interface ProviderMetrics {
    providerId: string
    collectedAt: number
    memoryBytes: number | null
    downloadBytes: number
    uploadBytes: number
    downloadSpeed: number
    uploadSpeed: number
    totalBytes: number
    tunnels: TunnelMetrics[]
    metadata: Record<string, unknown>
}

export interface RuntimeMetrics {
    collectedAt: number
    appMemoryBytes: number | null
    downloadBytes: number
    uploadBytes: number
    downloadSpeed: number
    uploadSpeed: number
    totalBytes: number
    providers: ProviderMetrics[]
}

export interface RuntimeTrafficSummary {
    collectedAt: number
    appMemoryBytes: number | null
    hasActiveTunnels: boolean
    downloadBytes: number
    uploadBytes: number
    downloadSpeed: number
    uploadSpeed: number
    totalBytes: number
    history: MetricSample[]
}
