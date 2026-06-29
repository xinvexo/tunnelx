import { markRaw } from 'vue'
import type { ProviderFrontendModule } from '@/providers/module'
import { CLOUDFLARE_PROVIDER_ID } from '@/providers/contract'
import type { TunnelResource, TunnelRuntimeInfo } from '@/providers/contract'
import EnvironmentView from './views/EnvironmentView.vue'
import SettingsView from './views/SettingsView.vue'
import TunnelsView from './views/TunnelsView.vue'
import { useCloudflareStore } from './stores'
import { publicUrlsFromHostname, serviceTag } from '@/providers/tunnelItems'

export const cloudflareFrontendProvider: ProviderFrontendModule = {
    descriptor: {
        id: CLOUDFLARE_PROVIDER_ID,
        name: 'Cloudflare Tunnel',
        summary: 'cloudflared / named tunnel',
        capabilities: {
            accountLogin: true,
            namedTunnels: true,
            credentials: false,
            dnsRoutes: true,
            ingress: true,
            localRuntime: true,
            runtimeMetrics: true,
            memoryStats: true,
            trafficStats: true,
            versionManagement: true,
        },
    },
    icon: 'simple-icons:cloudflare',
    homepageUrl: 'https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/',
    environmentPanel: markRaw(EnvironmentView),
    settingsPanel: markRaw(SettingsView),
    tunnelsPanel: markRaw(TunnelsView),
    async init() {
        await useCloudflareStore().init()
    },
    async hydrateConnection() {
        await useCloudflareStore().init()
    },
    resourceConnectionInfo(resource: TunnelResource) {
        return resource.providerTunnelId
    },
    resourceTunnelText(resource: TunnelResource) {
        return `${resource.ingress.filter((rule) => rule.enabled).length} / ${resource.ingress.length}`
    },
    resourceTunnelCount(resource: TunnelResource) {
        return resource.ingress.filter((rule) => rule.enabled).length
    },
    resourceTunnelRows(resource: TunnelResource, runtime?: TunnelRuntimeInfo) {
        const showPublicUrls = runtime?.status === 'running' || runtime?.status === 'warning'
        return resource.ingress.map((rule) => ({
            key: rule.id,
            name: rule.name || '-',
            target: rule.service || '-',
            kind: serviceTag(rule.service),
            publicUrls: showPublicUrls ? publicUrlsFromHostname(rule.hostname) : [],
        }))
    },
}
