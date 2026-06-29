import { CLOUDFLARE_PROVIDER_ID, type IngressRule, type TunnelResource } from '@/providers/contract'
import type {
    CloudflareAccountConfig,
    CloudflareAuthMode,
    CloudflareIngressRule,
    CloudflareTunnel,
} from '.'

const toIngressRule = (rule: CloudflareIngressRule): IngressRule => ({
    id: rule.id,
    name: rule.name.trim(),
    hostname: rule.hostname,
    service: rule.service,
    enabled: rule.enabled,
    dnsRouted: rule.dnsRouted,
})

const toCloudflareIngressRule = (rule: IngressRule): CloudflareIngressRule => ({
    id: rule.id,
    name: rule.name.trim(),
    hostname: rule.hostname,
    service: rule.service,
    enabled: rule.enabled,
    dnsRouted: rule.dnsRouted,
})

function authMode(value: unknown): CloudflareAuthMode {
    return value === 'apiToken' ? 'apiToken' : 'authorization'
}

function accountConfig(value: unknown): CloudflareAccountConfig {
    const account = typeof value === 'object' && value ? value as Record<string, unknown> : {}
    return {
        authMode: authMode(account.authMode),
        apiToken: String(account.apiToken ?? ''),
        tokenAccountId: String(account.tokenAccountId ?? ''),
        tokenAccountName: String(account.tokenAccountName ?? ''),
        authorizationAccountId: String(account.authorizationAccountId ?? ''),
        authorizationZoneId: String(account.authorizationZoneId ?? ''),
    }
}

export const toTunnelResource = (tunnel: CloudflareTunnel): TunnelResource => ({
    id: tunnel.id,
    providerId: CLOUDFLARE_PROVIDER_ID,
    name: tunnel.name,
    providerTunnelId: tunnel.tunnelId,
    credentialsRef: tunnel.credentialsFile,
    configFile: tunnel.configFile,
    ingress: tunnel.ingress.map(toIngressRule),
    createdAt: tunnel.createdAt,
    updatedAt: tunnel.updatedAt,
    metadata: {
        account: tunnel.account,
        certFile: tunnel.certFile,
    },
})

export const toCloudflareTunnel = (resource: TunnelResource): CloudflareTunnel => ({
    id: resource.id,
    name: resource.name,
    account: accountConfig(resource.metadata.account),
    certFile: typeof resource.metadata.certFile === 'string' ? resource.metadata.certFile : '',
    tunnelId: resource.providerTunnelId,
    credentialsFile: resource.credentialsRef,
    configFile: resource.configFile,
    ingress: resource.ingress.map(toCloudflareIngressRule),
    createdAt: resource.createdAt,
    updatedAt: resource.updatedAt,
})
