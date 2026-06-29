import { call } from '@/api/_invoke'
import type { FrpcStatus, ProxyStatusInfo, TrafficStats } from '@/providers/frp/domain/version'
import {
    providerStartTunnel,
    providerStopTunnel,
    providerTunnelStatus,
} from '@/providers/api'
import { FRP_PROVIDER_ID, type TunnelRuntimeInfo } from '@/providers/contract'

const toFrpcStatus = (info: TunnelRuntimeInfo): FrpcStatus => info.status

export const startFrpc = async (profileId: string) =>
    toFrpcStatus(await providerStartTunnel(FRP_PROVIDER_ID, profileId))
export const stopFrpc = async (profileId: string) =>
    toFrpcStatus(await providerStopTunnel(FRP_PROVIDER_ID, profileId))
export const frpcStatus = async (profileId: string) =>
    toFrpcStatus(await providerTunnelStatus(FRP_PROVIDER_ID, profileId))
export const frpcNeedsRestart = (profileId: string) => call<boolean>('frpc_needs_restart', { profileId })
export const frpcProxyStatus = (profileId: string) => call<ProxyStatusInfo[] | null>('frpc_proxy_status', { profileId })
export const proxyTrafficStats = (profileId: string) => call<TrafficStats[]>('proxy_traffic_stats', { profileId })
