import { call } from '@/api/_invoke'
import type { ProviderRuntimeUpdateStatus } from '@/providers/contract'
import type { VersionList } from '@/providers/frp/domain/version'

export const listFrpcVersions = () => call<VersionList>('list_frpc_versions')
export const checkFrpcUpdate = () => call<ProviderRuntimeUpdateStatus>('check_frpc_update')
export const installFrpcVersion = (version: string) => call<void>('install_frpc_version', { version })
export const activateFrpcVersion = (version: string) => call<void>('activate_frpc_version', { version })
export const removeFrpcVersion = (version: string) => call<void>('remove_frpc_version', { version })
export const uninstallActiveFrpcVersion = () => call<void>('uninstall_active_frpc_version')
export const activeFrpcVersion = () => call<string | null>('active_frpc_version')
export const getFrpcMirror = () => call<string>('get_frpc_mirror')
export const setFrpcMirror = (mirror: string) => call<void>('set_frpc_mirror', { mirror })
