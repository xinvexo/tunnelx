import { call } from '@/api/_invoke'
import type {
    CloudflareAccount,
    CloudflareData,
    CloudflareTunnel,
    CloudflareZone,
    CloudflaredStatus,
} from '../domain'
import type { ProviderRuntimeUpdateStatus } from '@/providers/contract'

export const cloudflareData = () => call<CloudflareData>('cloudflare_data')
export const loginCloudflareConnection = (tunnelId: string) =>
    call<{ success: boolean; stdout: string; stderr: string }>('cloudflare_login_connection', { tunnelId })
export const installCloudflared = () => call<CloudflaredStatus>('cloudflare_install_cloudflared')
export const uninstallCloudflared = () => call<CloudflaredStatus>('cloudflare_uninstall_cloudflared')
export const checkCloudflaredUpdate = () =>
    call<ProviderRuntimeUpdateStatus>('cloudflare_check_cloudflared_update')
export const verifyCloudflareToken = (tunnelId: string) =>
    call<boolean>('cloudflare_verify_token', { tunnelId })
export const listCloudflareAccounts = (tunnelId: string) =>
    call<CloudflareAccount[]>('cloudflare_list_accounts', { tunnelId })
export const listCloudflareZones = (tunnelId: string) =>
    call<CloudflareZone[]>('cloudflare_list_zones', { tunnelId })
export const listRemoteCloudflareTunnels = (tunnelId: string) =>
    call<CloudflareTunnel[]>('cloudflare_list_remote_tunnels', { tunnelId })
