import { markRaw } from 'vue'
import frpIcon from '@/assets/providers/frp.svg'
import type { ProviderFrontendModule } from '@/providers/module'
import { FRP_PROVIDER_ID } from '@/providers/contract'
import type { TunnelResource, TunnelRuntimeInfo } from '@/providers/contract'
import EnvironmentView from './views/EnvironmentView.vue'
import SettingsView from './views/SettingsView.vue'
import TunnelsView from './views/TunnelsView.vue'
import { useProfileStore } from './stores/profile'
import { useRuntimeStore } from './stores/runtime'
import { useVersionStore } from './stores/version'
import { endpointPublicUrls } from '@/providers/runtimeDetails'

export const frpFrontendProvider: ProviderFrontendModule = {
    descriptor: {
        id: FRP_PROVIDER_ID,
        name: 'Frp',
        summary: 'frpc / frps',
        capabilities: {
            accountLogin: false,
            namedTunnels: true,
            credentials: false,
            dnsRoutes: false,
            ingress: false,
            localRuntime: true,
            runtimeMetrics: true,
            memoryStats: true,
            trafficStats: true,
            versionManagement: true,
        },
    },
    icon: 'lucide:server',
    iconAsset: frpIcon,
    homepageUrl: 'https://github.com/fatedier/frp',
    settingsPanel: markRaw(SettingsView),
    tunnelsPanel: markRaw(TunnelsView),
    environmentPanel: markRaw(EnvironmentView),
    async init() {
        await Promise.all([
            useProfileStore().init(),
            useRuntimeStore().init(),
            useVersionStore().init(),
        ])
    },
    async hydrateConnection(resource: TunnelResource) {
        const runtime = useRuntimeStore()
        await Promise.all([
            useProfileStore().loadCurrent(resource.id),
            runtime.refresh(resource.id),
            runtime.refreshLogDerivedStatus(resource.id),
        ])
    },
    cleanupConnection(resource: TunnelResource) {
        useProfileStore().forgetDeleted(resource.id)
    },
    resourceConnectionInfo(resource: TunnelResource) {
        const host = String(resource.metadata.serverAddr ?? '')
        const port = Number(resource.metadata.serverPort ?? 0)
        return host ? `${host}:${port}` : ''
    },
    resourceTunnelText(resource: TunnelResource, runtime?: TunnelRuntimeInfo) {
        const enabled = Number(resource.metadata.enabledProxyCount ?? 0)
        const total = Number(resource.metadata.proxyCount ?? 0)
        // 未运行：显示配置态（已启用 / 总数）。
        if (runtime?.status !== 'running' && runtime?.status !== 'warning') {
            return `${enabled} / ${total}`
        }
        // 运行中：显示实际在线代理数 / 已启用数，部分隧道失败时一眼可见（如 2 / 3）。
        const runtimeStore = useRuntimeStore()
        const proxies = Array.isArray(resource.metadata.proxies) ? resource.metadata.proxies : []
        const running = proxies.reduce((count, proxy) => {
            const name = String((proxy as Record<string, unknown>).name ?? '')
            return name && runtimeStore.proxyStatusOf(resource.id, name) === 'running'
                ? count + 1
                : count
        }, 0)
        return `${running} / ${enabled}`
    },
    resourceTunnelCount(resource: TunnelResource) {
        return Number(resource.metadata.enabledProxyCount ?? resource.metadata.proxyCount ?? 0)
    },
    resourceTunnelRows(resource: TunnelResource, runtime?: TunnelRuntimeInfo) {
        const proxies = Array.isArray(resource.metadata.proxies) ? resource.metadata.proxies : []
        return proxies.map((proxy) => {
            const record = proxy as Record<string, unknown>
            const name = String(record.name ?? '')
            return {
                key: String(record.id ?? record.name),
                name,
                target: `${record.localIP ?? ''}:${record.localPort ?? ''}`,
                kind: String(record.proxyType ?? '').toUpperCase(),
                publicUrls: endpointPublicUrls(name, runtime).map((item) => item.publicUrl),
            }
        })
    },
}
