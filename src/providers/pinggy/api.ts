import { call } from '@/api/_invoke'
import type { ProviderRuntimeUpdateStatus, ProviderStatus, TunnelResource } from '@/providers/contract'

export const installPinggyRuntime = () => call<ProviderStatus>('pinggy_install_runtime')
export const uninstallPinggyRuntime = () => call<ProviderStatus>('pinggy_uninstall_runtime')
export const checkPinggyRuntimeUpdate = () => call<ProviderRuntimeUpdateStatus>('pinggy_check_runtime_update')
export const authenticatePinggyTunnel = (tunnel: TunnelResource) =>
    call<TunnelResource>('pinggy_authenticate_tunnel', { tunnel })
