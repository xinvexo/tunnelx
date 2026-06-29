import { markRaw } from 'vue'
import pinggyIcon from '@/assets/providers/pinggy.png'
import type { ProviderFrontendModule } from '@/providers/module'
import { PINGGY_PROVIDER_ID, type TunnelResource, type TunnelRuntimeInfo } from '@/providers/contract'
import EnvironmentView from './views/EnvironmentView.vue'
import SettingsView from './views/SettingsView.vue'
import TunnelsView from './views/TunnelsView.vue'
import { enabledEndpointCount, pinggyEndpointPublicUrls, pinggyMetadata } from './domain'
import { tunnelItemTag } from '@/providers/tunnelItems'

export const pinggyFrontendProvider: ProviderFrontendModule = {
    descriptor: {
        id: PINGGY_PROVIDER_ID,
        name: 'Pinggy',
        summary: 'Pinggy CLI',
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
    icon: 'lucide:route',
    iconAsset: pinggyIcon,
    homepageUrl: 'https://pinggy.io/',
    environmentPanel: markRaw(EnvironmentView),
    settingsPanel: markRaw(SettingsView),
    tunnelsPanel: markRaw(TunnelsView),
    resourceConnectionInfo(resource: TunnelResource) {
        const metadata = pinggyMetadata(resource)
        const server = metadata.server.trim()
        if (!metadata.token.trim() || !server || server === 'free.pinggy.io') return ''
        return server
    },
    resourceTunnelText(resource: TunnelResource) {
        const metadata = pinggyMetadata(resource)
        return `${enabledEndpointCount(metadata)} / ${metadata.endpoints.length}`
    },
    resourceTunnelCount(resource: TunnelResource) {
        return enabledEndpointCount(pinggyMetadata(resource))
    },
    resourceTunnelRows(resource: TunnelResource, runtime?: TunnelRuntimeInfo) {
        return pinggyMetadata(resource).endpoints.map((endpoint) => ({
            key: endpoint.id,
            name: endpoint.name,
            target: endpoint.localAddr,
            kind: tunnelItemTag(endpoint.tunnelType),
            publicUrls: pinggyEndpointPublicUrls(endpoint, runtime).map((item) => item.publicUrl),
        }))
    },
}
