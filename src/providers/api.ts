import { call } from '@/api/_invoke'
import type {
    CreateTunnelInput,
    ProviderCommandOutput,
    ProviderDescriptor,
    ProviderMetrics,
    ProviderStatus,
    RuntimeMetrics,
    RuntimeTrafficSummary,
    TunnelResource,
    TunnelRuntimeInfo,
} from './contract'

export const listProviders = () => call<ProviderDescriptor[]>('list_providers')

export const connectionOrder = () => call<string[]>('connection_order')

export const reorderConnections = (keys: string[]) =>
    call<void>('reorder_connections', { keys })

export const providerStatus = (providerId: string) =>
    call<ProviderStatus>('provider_status', { providerId })

export const providerLogin = (providerId: string) =>
    call<ProviderCommandOutput>('provider_login', { providerId })

export const providerTunnels = (providerId: string) =>
    call<TunnelResource[]>('provider_tunnels', { providerId })

export const providerCreateTunnel = (providerId: string, input: CreateTunnelInput) =>
    call<TunnelResource>('provider_create_tunnel', { providerId, input })

export const providerUpdateTunnel = (providerId: string, tunnel: TunnelResource) =>
    call<TunnelResource>('provider_update_tunnel', { providerId, tunnel })

export const providerDuplicateTunnel = (providerId: string, id: string) =>
    call<TunnelResource>('provider_duplicate_tunnel', { providerId, id })

export const providerDeleteTunnel = (providerId: string, id: string, remote: boolean) =>
    call<void>('provider_delete_tunnel', { providerId, id, remote })

export const providerStartTunnel = (providerId: string, id: string) =>
    call<TunnelRuntimeInfo>('provider_start_tunnel', { providerId, id })

export const providerStopTunnel = (providerId: string, id: string) =>
    call<TunnelRuntimeInfo>('provider_stop_tunnel', { providerId, id })

export const providerTunnelStatus = (providerId: string, id: string) =>
    call<TunnelRuntimeInfo>('provider_tunnel_status', { providerId, id })

export const providerTunnelLogs = (providerId: string, id: string) =>
    call<string[]>('provider_tunnel_logs', { providerId, id })

export const watchdogLogs = () =>
    call<string[]>('watchdog_logs')

export const providerMetrics = (providerId: string) =>
    call<ProviderMetrics>('provider_metrics', { providerId })

export const runtimeMetrics = () => call<RuntimeMetrics>('runtime_metrics')

export const runtimeTrafficSummary = () =>
    call<RuntimeTrafficSummary>('runtime_traffic_summary')
