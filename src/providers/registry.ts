import type { Component } from 'vue'
import { cloudflareFrontendProvider } from './cloudflare/module'
import { cpolarFrontendProvider } from './cpolar/module'
import { frpFrontendProvider } from './frp/module'
import { ngrokFrontendProvider } from './ngrok/module'
import { pinggyFrontendProvider } from './pinggy/module'
import type { ProviderFrontendModule, ProviderQuotaRow, ProviderTunnelPreviewRow } from './module'
import type { ProviderDescriptor, TunnelResource, TunnelRuntimeInfo } from './contract'

const providerModules: ProviderFrontendModule[] = [
    frpFrontendProvider,
    cloudflareFrontendProvider,
    ngrokFrontendProvider,
    cpolarFrontendProvider,
    pinggyFrontendProvider,
]

export const providerRoutes = Object.fromEntries(
    providerModules.map((provider) => [provider.descriptor.id, '/connections']),
) as Record<string, string>

export function frontendProviderModules(): ProviderFrontendModule[] {
    return providerModules
}

export function frontendProviderDescriptors(): ProviderDescriptor[] {
    return providerModules.map((provider) => provider.descriptor)
}

export function defaultProviderId(): string {
    return providerModules[0]?.descriptor.id ?? ''
}

export function frontendProvider(providerId: string): ProviderFrontendModule | undefined {
    return providerModules.find((provider) => provider.descriptor.id === providerId)
}

export async function initFrontendProviders() {
    await Promise.all(providerModules.map((provider) => provider.init?.()))
}

export async function hydrateProviderConnection(resource: TunnelResource) {
    await frontendProvider(resource.providerId)?.hydrateConnection?.(resource)
}

export async function cleanupProviderConnection(resource: TunnelResource) {
    await frontendProvider(resource.providerId)?.cleanupConnection?.(resource)
}

export function providerIcon(providerId: string): string {
    return frontendProvider(providerId)?.icon ?? 'lucide:plug'
}

export function providerIconAsset(providerId: string): string {
    return frontendProvider(providerId)?.iconAsset ?? ''
}

export function providerSettingsPanel(providerId: string): Component | undefined {
    return frontendProvider(providerId)?.settingsPanel
}

export function providerTunnelsPanel(providerId: string): Component | undefined {
    return frontendProvider(providerId)?.tunnelsPanel
}

export function providerEnvironmentPanels(): { id: string; name: string; component: Component }[] {
    return providerModules.flatMap((provider) => {
        const component = provider.environmentPanel
        return component
            ? [{
                id: provider.descriptor.id,
                name: provider.descriptor.name,
                component,
            }]
            : []
    })
}

export function providerResourceConnectionInfo(resource: TunnelResource, runtime?: TunnelRuntimeInfo): string {
    const provider = frontendProvider(resource.providerId)
    if (provider?.resourceConnectionInfo) return provider.resourceConnectionInfo(resource, runtime)
    return resource.providerTunnelId || resource.credentialsRef
}

export function providerResourceTunnelText(resource: TunnelResource, runtime?: TunnelRuntimeInfo): string {
    return frontendProvider(resource.providerId)?.resourceTunnelText?.(resource, runtime) ?? String(resource.ingress.length)
}

export function providerResourceTunnelCount(resource: TunnelResource, runtime?: TunnelRuntimeInfo): number {
    return frontendProvider(resource.providerId)?.resourceTunnelCount?.(resource, runtime) ?? resource.ingress.length
}

export function providerResourceTunnelRows(resource: TunnelResource, runtime?: TunnelRuntimeInfo, limit = 6): ProviderTunnelPreviewRow[] {
    const providerRows = frontendProvider(resource.providerId)?.resourceTunnelRows?.(resource, runtime)
    if (providerRows) return providerRows.slice(0, limit)
    return resource.ingress.slice(0, limit).map((rule) => ({
        key: rule.id,
        name: rule.name || '-',
        target: rule.service || '-',
        kind: rule.enabled ? (rule.dnsRouted ? 'DNS' : 'Ingress') : 'OFF',
    }))
}

export function providerResourceQuotaRows(resource: TunnelResource, runtime?: TunnelRuntimeInfo): ProviderQuotaRow[] {
    return frontendProvider(resource.providerId)?.resourceQuotaRows?.(resource, runtime) ?? []
}
