import { markRaw } from 'vue'
import type { ProviderFrontendModule } from '@/providers/module'
import { NGROK_PROVIDER_ID, type TunnelResource, type TunnelRuntimeInfo } from '@/providers/contract'
import EnvironmentView from './views/EnvironmentView.vue'
import SettingsView from './views/SettingsView.vue'
import TunnelsView from './views/TunnelsView.vue'
import { ngrokEndpointPublicUrls, ngrokMetadata } from './domain'
import { tunnelItemTag } from '@/providers/tunnelItems'

export const ngrokFrontendProvider: ProviderFrontendModule = {
    descriptor: {
        id: NGROK_PROVIDER_ID,
        name: 'Ngrok',
        summary: 'ngrok agent',
        capabilities: {
            accountLogin: false,
            namedTunnels: true,
            credentials: true,
            dnsRoutes: false,
            ingress: false,
            localRuntime: true,
            runtimeMetrics: true,
            memoryStats: true,
            trafficStats: true,
            versionManagement: true,
        },
    },
    icon: 'simple-icons:ngrok',
    homepageUrl: 'https://ngrok.com/',
    environmentPanel: markRaw(EnvironmentView),
    settingsPanel: markRaw(SettingsView),
    tunnelsPanel: markRaw(TunnelsView),
    resourceConnectionInfo(resource: TunnelResource) {
        const metadata = ngrokMetadata(resource)
        if (!metadata.authtoken.trim()) return ''
        return metadata.region || 'global'
    },
    resourceTunnelText(resource: TunnelResource) {
        const endpoints = ngrokMetadata(resource).endpoints
        return `${endpoints.filter((endpoint) => endpoint.enabled).length} / ${endpoints.length}`
    },
    resourceTunnelCount(resource: TunnelResource) {
        return ngrokMetadata(resource).endpoints.filter((endpoint) => endpoint.enabled).length
    },
    resourceTunnelRows(resource: TunnelResource, runtime?: TunnelRuntimeInfo) {
        return ngrokMetadata(resource).endpoints.map((endpoint) => ({
            key: endpoint.id,
            name: endpoint.name,
            target: endpoint.addr,
            kind: tunnelItemTag(endpoint.proto),
            publicUrls: ngrokPublicUrls(endpoint, runtime),
        }))
    },
}

function ngrokPublicUrls(
    endpoint: ReturnType<typeof ngrokMetadata>['endpoints'][number],
    runtime?: TunnelRuntimeInfo,
): string[] {
    const runtimeUrls = ngrokEndpointPublicUrls(endpoint, runtime).map((item) => item.publicUrl)
    if (runtimeUrls.length) return runtimeUrls
    return endpoint.domain ? [endpoint.domain] : []
}
