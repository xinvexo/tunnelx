import { call } from '@/api/_invoke'
import type { ProviderRuntimeUpdateStatus, ProviderStatus, TunnelResource } from '@/providers/contract'

export const installNgrokRuntime = () => call<ProviderStatus>('ngrok_install_runtime')
export const uninstallNgrokRuntime = () => call<ProviderStatus>('ngrok_uninstall_runtime')
export const checkNgrokRuntimeUpdate = () => call<ProviderRuntimeUpdateStatus>('ngrok_check_runtime_update')
export const authenticateNgrokTunnel = (tunnel: TunnelResource) =>
    call<TunnelResource>('ngrok_authenticate_tunnel', { tunnel })
