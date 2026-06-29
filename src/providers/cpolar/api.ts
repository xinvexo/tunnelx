import { call } from '@/api/_invoke'
import type { ProviderRuntimeUpdateStatus, ProviderStatus, TunnelResource } from '@/providers/contract'

export const installCpolarRuntime = () => call<ProviderStatus>('cpolar_install_runtime')
export const uninstallCpolarRuntime = () => call<ProviderStatus>('cpolar_uninstall_runtime')
export const checkCpolarRuntimeUpdate = () => call<ProviderRuntimeUpdateStatus>('cpolar_check_runtime_update')
export const authenticateCpolarTunnel = (tunnel: TunnelResource) =>
    call<TunnelResource>('cpolar_authenticate_tunnel', { tunnel })
