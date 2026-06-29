import { markRaw } from 'vue'
import cpolarIcon from '@/assets/providers/cpolar.png'
import type { ProviderFrontendModule } from '@/providers/module'
import { CPOLAR_PROVIDER_ID, type TunnelResource, type TunnelRuntimeInfo } from '@/providers/contract'
import EnvironmentView from './views/EnvironmentView.vue'
import SettingsView from './views/SettingsView.vue'
import TunnelsView from './views/TunnelsView.vue'
import { cpolarEndpointPublicUrls, cpolarMetadata, cpolarRuntimePublicUrls } from './domain'
import { tunnelItemTag } from '@/providers/tunnelItems'

export const cpolarFrontendProvider: ProviderFrontendModule = {
    descriptor: {
        id: CPOLAR_PROVIDER_ID,
        name: 'Cpolar',
        summary: 'cpolar agent',
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
    icon: 'lucide:radio-tower',
    iconAsset: cpolarIcon,
    homepageUrl: 'https://www.cpolar.com/',
    environmentPanel: markRaw(EnvironmentView),
    settingsPanel: markRaw(SettingsView),
    tunnelsPanel: markRaw(TunnelsView),
    resourceConnectionInfo(resource: TunnelResource) {
        const metadata = cpolarMetadata(resource)
        if (!metadata.authtoken.trim()) return ''
        return metadata.region || 'default'
    },
    resourceTunnelText(resource: TunnelResource) {
        const endpoints = cpolarMetadata(resource).endpoints
        return `${endpoints.filter((endpoint) => endpoint.enabled).length} / ${endpoints.length}`
    },
    resourceTunnelCount(resource: TunnelResource) {
        return cpolarMetadata(resource).endpoints.filter((endpoint) => endpoint.enabled).length
    },
    resourceTunnelRows(resource: TunnelResource, runtime?: TunnelRuntimeInfo) {
        const metadata = cpolarMetadata(resource)
        const enabledCount = metadata.endpoints.filter((endpoint) => endpoint.enabled).length
        return metadata.endpoints.map((endpoint) => ({
            key: endpoint.id,
            name: endpoint.name,
            target: endpoint.addr,
            kind: tunnelItemTag(endpoint.proto),
            publicUrls: cpolarPublicUrls(endpoint, runtime, enabledCount),
        }))
    },
}

function cpolarPublicUrls(
    endpoint: ReturnType<typeof cpolarMetadata>['endpoints'][number],
    runtime?: TunnelRuntimeInfo,
    enabledCount = 0,
): string[] {
    const runtimeUrls = (enabledCount === 1
        ? cpolarRuntimePublicUrls(runtime)
        : cpolarEndpointPublicUrls(endpoint, runtime)
    ).map((item) => item.publicUrl)
    if (runtimeUrls.length) return runtimeUrls
    const configured = endpoint.proto === 'tcp' ? endpoint.remoteAddr : endpoint.hostname
    return configured ? [configured] : []
}
