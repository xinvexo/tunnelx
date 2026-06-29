import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import { i18n } from '@/i18n'
import {
    localizedMessageFromError,
    localizedMessageSpecFromError,
    type LocalizedMessageSpec,
} from '@/api/_invoke'
import { useUiStore } from '@/stores/ui'
import {
    connectionOrder,
    listProviders,
    providerCreateTunnel,
    providerDeleteTunnel,
    providerDuplicateTunnel,
    reorderConnections,
    providerStartTunnel,
    providerStopTunnel,
    providerTunnelStatus,
    providerTunnels,
    providerUpdateTunnel,
} from './api'
import {
    CLOUDFLARE_PROVIDER_ID,
    type ProviderDescriptor,
    type ProviderTunnelsUpdatedEvent,
    type TunnelResource,
    type TunnelRuntimeInfo,
    type TunnelRuntimeState,
    type TunnelRuntimeStatusEvent,
} from './contract'
import { cleanupProviderConnection } from './registry'

interface State {
    providers: ProviderDescriptor[]
    resources: TunnelResource[]
    orderKeys: string[]
    runtime: Record<string, TunnelRuntimeInfo>
    loaded: boolean
    loading: boolean
    listening: boolean
}

export function connectionKey(providerId: string, id: string) {
    return `${providerId}:${id}`
}

function resourceKey(resource: TunnelResource) {
    return connectionKey(resource.providerId, resource.id)
}

function normalizeConnectionOrder(keys: string[], resources: TunnelResource[]) {
    const current = new Set(resources.map(resourceKey))
    const seen = new Set<string>()
    const order: string[] = []
    for (const key of keys) {
        if (current.has(key) && !seen.has(key)) {
            order.push(key)
            seen.add(key)
        }
    }
    for (const resource of resources) {
        const key = resourceKey(resource)
        if (!seen.has(key)) {
            order.push(key)
            seen.add(key)
        }
    }
    return order
}

function sortResourcesByOrder(resources: TunnelResource[], keys: string[]) {
    const index = new Map(keys.map((key, i) => [key, i]))
    return resources
        .map((resource, fallback) => ({ resource, fallback, order: index.get(resourceKey(resource)) }))
        .sort((a, b) => {
            if (a.order == null && b.order == null) return a.fallback - b.fallback
            if (a.order == null) return 1
            if (b.order == null) return -1
            return a.order - b.order
        })
        .map((item) => item.resource)
}

function sameOrder(a: string[], b: string[]) {
    return a.length === b.length && a.every((key, index) => key === b[index])
}

function messageFromError(error: unknown) {
    return localizedMessageFromError(error)
}

function runtimeMessageSpec(text: string, provider: string): LocalizedMessageSpec {
    const message = text.trim()
    if (!message) {
        return {
            key: 'events.message.runtimeFallback',
            params: { provider },
            fallback: `${provider} runtime status changed unexpectedly.`,
        }
    }

    const runtimeMissing = message.match(/^(frpc|cloudflared|ngrok|cpolar|pinggy)(?: runtime)? (?:is )?(?:not installed|not ready)/i)
    if (runtimeMissing) {
        return {
            key: 'events.message.runtimeNotInstalled',
            params: { runtime: runtimeName(runtimeMissing[1]) },
            fallback: message,
        }
    }

    const credential = credentialName(message)
    if (credential) {
        return {
            key: 'events.message.credentialRequired',
            params: { credential },
            fallback: message,
        }
    }

    if (/at least one complete hostname ingress is required/i.test(message)) {
        return {
            key: 'events.message.hostnameIngressRequired',
            fallback: message,
        }
    }

    if (/not authenticated/i.test(message) && /expire in \d+ minutes/i.test(message)) {
        return {
            key: 'events.message.pinggyUnauthenticated',
            fallback: message,
        }
    }

    if (/start timed out while waiting for runtime status/i.test(message)) {
        return {
            key: 'events.message.startTimedOut',
            fallback: message,
        }
    }

    if (/^start confirmation timed out\b/i.test(message)) {
        return {
            key: 'events.message.startConfirmationTimedOut',
            fallback: message,
        }
    }

    if (/^frpc exited with error$/i.test(message)) {
        return {
            key: 'events.message.frpcExitedWithError',
            fallback: message,
        }
    }

    if (/^frpc running with proxy warnings$/i.test(message)) {
        return {
            key: 'events.message.frpcProxyWarnings',
            fallback: message,
        }
    }

    const exitedUnexpectedly = message.match(/^provider\s+(\S+)\s+connection\s+\S+\s+exited unexpectedly with code\s+(\d+)/i)
    if (exitedUnexpectedly) {
        return {
            key: 'events.message.providerExitedUnexpectedly',
            params: { provider: runtimeName(exitedUnexpectedly[1]), code: exitedUnexpectedly[2] },
            fallback: message,
        }
    }

    return { fallback: message }
}

function eventMessageSpecFromError(error: unknown): LocalizedMessageSpec {
    const spec = localizedMessageSpecFromError(error)
    if (spec.key) return spec
    return runtimeMessageSpec(spec.fallback, '')
}

function runtimeName(value: string) {
    const lower = value.toLowerCase()
    if (lower === 'pinggy') return 'Pinggy'
    if (lower === 'cloudflared') return 'cloudflared'
    if (lower === 'frpc') return 'frpc'
    return lower
}

function credentialName(message: string) {
    if (/pinggy token is required/i.test(message)) return 'Pinggy Token'
    if (/ngrok authtoken is required/i.test(message)) return 'ngrok Authtoken'
    if (/cpolar authtoken is required/i.test(message)) return 'cpolar Authtoken'
    if (/cloudflare api token/i.test(message)) return 'Cloudflare API Token'
    if (/cloudflare account id/i.test(message)) return 'Cloudflare Account ID'
    return ''
}

function runtimeNeedsAttention(info: TunnelRuntimeInfo) {
    return info.status === 'warning' || info.status === 'errored'
}

function runtimeStartSettled(status: TunnelRuntimeState) {
    return status === 'running' || status === 'warning' || status === 'errored' || status === 'stopped'
}

function startConfirmationTimedOut(message: string) {
    return /^start confirmation timed out\b/i.test(message.trim())
}

function staleRuntimeUpdate(previous: TunnelRuntimeInfo | undefined, next: TunnelRuntimeInfo) {
    if (!previous) return false
    if (next.status === 'starting') {
        return previous.status === 'running' || previous.status === 'warning' || previous.status === 'errored'
    }
    if (next.status === 'stopping') {
        return previous.status === 'stopped' || previous.status === 'errored'
    }
    return false
}

function startTimeoutMs(resource: TunnelResource) {
    return resource.providerId === CLOUDFLARE_PROVIDER_ID ? 180_000 : 35_000
}

function providerOrder(providers: ProviderDescriptor[], resources: TunnelResource[], nextProviderId?: string) {
    const order: string[] = []
    const seen = new Set<string>()
    for (const provider of providers) {
        order.push(provider.id)
        seen.add(provider.id)
    }
    for (const resource of resources) {
        if (seen.has(resource.providerId)) continue
        order.push(resource.providerId)
        seen.add(resource.providerId)
    }
    if (nextProviderId && !seen.has(nextProviderId)) order.push(nextProviderId)
    return order
}

function mergeProviderResources(
    providers: ProviderDescriptor[],
    current: TunnelResource[],
    providerId: string,
    refreshed: TunnelResource[],
) {
    return providerOrder(providers, current, providerId).flatMap((id) =>
        id === providerId ? refreshed : current.filter((resource) => resource.providerId === id),
    )
}

export const useConnectionStore = defineStore('connections', {
    state: (): State => ({
        providers: [],
        resources: [],
        orderKeys: [],
        runtime: {},
        loaded: false,
        loading: false,
        listening: false,
    }),
    getters: {
        count: (state) => state.resources.length,
        byProvider: (state) => (providerId: string) =>
            state.resources.filter((resource) => resource.providerId === providerId),
        providerName: (state) => (providerId: string) =>
            state.providers.find((provider) => provider.id === providerId)?.name ?? providerId,
        providerOf: (state) => (providerId: string): ProviderDescriptor | undefined =>
            state.providers.find((provider) => provider.id === providerId),
        resourceOf: (state) => (providerId: string, id: string): TunnelResource | undefined =>
            state.resources.find((resource) => resource.providerId === providerId && resource.id === id),
        runtimeOf: (state) => (resource: TunnelResource): TunnelRuntimeInfo =>
            state.runtime[connectionKey(resource.providerId, resource.id)] ?? {
                providerId: resource.providerId,
                tunnelId: resource.id,
                status: 'stopped',
                pid: null,
                message: '',
                details: {},
            },
        has: (state) => (providerId: string, id: string) =>
            state.resources.some((resource) => resource.providerId === providerId && resource.id === id),
    },
    actions: {
        async init() {
            if (!this.listening) {
                this.listening = true
                try {
                    await listen<TunnelRuntimeStatusEvent>('provider-tunnel-status-changed', (event) => {
                        this.applyRuntime(event.payload.info)
                    })
                    await listen<ProviderTunnelsUpdatedEvent>('provider-tunnels-updated', (event) => {
                        void this.refreshProvider(event.payload.providerId)
                    })
                } catch {
                    // Browser previews do not have the Tauri event bridge.
                }
            }
            if (!this.loaded) await this.refresh()
        },
        async refresh() {
            if (this.loading) return
            this.loading = true
            try {
                const [providers, order] = await Promise.all([
                    listProviders(),
                    connectionOrder().catch(() => [] as string[]),
                ])
                this.providers = providers
                const lists = await Promise.all(
                    this.providers.map((provider) =>
                        providerTunnels(provider.id).catch(() => [] as TunnelResource[]),
                    ),
                )
                const resources = lists.flat()
                this.applyResourceOrder(resources, order)
                this.pruneRuntime(this.resources)
                this.loaded = true
                await this.refreshRuntime()
            } finally {
                this.loading = false
            }
        },
        async refreshProvider(providerId: string) {
            const resources = await providerTunnels(providerId).catch(() => [] as TunnelResource[])
            this.applyResourceOrder(mergeProviderResources(this.providers, this.resources, providerId, resources))
            this.pruneRuntime(this.resources)
            await Promise.all(resources.map((resource) => this.refreshResourceRuntime(resource).catch(() => undefined)))
        },
        applyResourceOrder(resources: TunnelResource[], keys?: string[]) {
            const sourceKeys = keys ?? this.orderKeys
            const order = normalizeConnectionOrder(sourceKeys, resources)
            this.orderKeys = order
            this.resources = sortResourcesByOrder(resources, order)
        },
        async persistOrder() {
            await reorderConnections(this.orderKeys)
        },
        async persistOrderSilently() {
            try {
                await this.persistOrder()
            } catch {
                // 连接本身已经创建/删除成功，排序持久化失败不阻塞主流程。
            }
        },
        async reorder(keys: string[]) {
            const order = normalizeConnectionOrder(keys, this.resources)
            if (sameOrder(order, this.orderKeys)) return
            const previousOrder = this.orderKeys
            const previousResources = this.resources
            this.orderKeys = order
            this.resources = sortResourcesByOrder(this.resources, order)
            try {
                await this.persistOrder()
            } catch (error) {
                this.orderKeys = previousOrder
                this.resources = previousResources
                useUiStore().notify(messageFromError(error), 'danger')
            }
        },
        async refreshRuntime() {
            await Promise.all(this.resources.map((resource) => this.refreshResourceRuntime(resource).catch(() => undefined)))
        },
        async refreshResourceRuntime(resource: TunnelResource) {
            this.applyRuntime(await providerTunnelStatus(resource.providerId, resource.id))
        },
        async create(providerId: string, name: string) {
            const resource = await providerCreateTunnel(providerId, { name })
            await this.refreshProvider(providerId)
            await this.persistOrderSilently()
            return this.resourceOf(providerId, resource.id) ?? resource
        },
        async update(resource: TunnelResource) {
            const saved = await providerUpdateTunnel(resource.providerId, resource)
            await this.refreshProvider(resource.providerId)
            return this.resourceOf(resource.providerId, resource.id) ?? saved
        },
        async duplicate(resource: TunnelResource) {
            const duplicated = await providerDuplicateTunnel(resource.providerId, resource.id)
            await this.refreshProvider(resource.providerId)
            await this.persistOrderSilently()
            return this.resourceOf(resource.providerId, duplicated.id) ?? duplicated
        },
        async delete(resource: TunnelResource, remote = true) {
            await providerDeleteTunnel(resource.providerId, resource.id, remote)
            await cleanupProviderConnection(resource)
            const key = resourceKey(resource)
            this.resources = this.resources.filter((item) => resourceKey(item) !== key)
            this.pruneRuntime(this.resources)
            await this.refreshProvider(resource.providerId)
            await this.persistOrderSilently()
        },
        async start(resource: TunnelResource) {
            try {
                this.applyRuntime(await providerStartTunnel(resource.providerId, resource.id))
                await this.waitForStartResult(resource)
                await this.refreshResourceRuntime(resource)
            } catch (error) {
                const ui = useUiStore()
                const message = eventMessageSpecFromError(error)
                ui.recordImportantEvent({
                    title: i18n.global.t('events.startFailedTitle', { name: resource.name }),
                    titleKey: 'events.startFailedTitle',
                    titleParams: { name: resource.name },
                    message: message.fallback,
                    messageKey: message.key,
                    messageParams: message.params,
                    tone: 'danger',
                    source: `start:${resourceKey(resource)}`,
                })
                throw error
            }
        },
        async waitForStartResult(resource: TunnelResource) {
            const key = resourceKey(resource)
            const deadline = Date.now() + startTimeoutMs(resource)
            while (Date.now() < deadline) {
                const info = this.runtime[key]
                if (info && runtimeStartSettled(info.status)) {
                    if (info.status === 'errored' || info.status === 'stopped') {
                        throw new Error(info.message || 'Connection failed to start')
                    }
                    if (info.status === 'warning' && startConfirmationTimedOut(info.message)) {
                        throw new Error(info.message)
                    }
                    return info
                }
                await new Promise((resolve) => window.setTimeout(resolve, 250))
            }
            throw new Error('Connection start timed out while waiting for runtime status')
        },
        async stop(resource: TunnelResource) {
            this.applyRuntime(await providerStopTunnel(resource.providerId, resource.id))
        },
        applyRuntime(info: TunnelRuntimeInfo) {
            if (this.loaded && !this.has(info.providerId, info.tunnelId)) return
            const key = connectionKey(info.providerId, info.tunnelId)
            const previous = this.runtime[key]
            if (staleRuntimeUpdate(previous, info)) return
            this.runtime = {
                ...this.runtime,
                [key]: info,
            }
            this.recordRuntimeAttention(info, previous)
        },
        recordRuntimeAttention(info: TunnelRuntimeInfo, previous?: TunnelRuntimeInfo) {
            if (!runtimeNeedsAttention(info)) return
            if (previous?.status === info.status && previous.message === info.message) return

            const resource = this.resourceOf(info.providerId, info.tunnelId)
            const provider = this.providerName(info.providerId)
            const name = resource?.name ?? info.tunnelId
            const titleKey = info.status === 'errored' ? 'events.runtimeErroredTitle' : 'events.runtimeWarningTitle'
            const message = runtimeMessageSpec(info.message, provider)
            const ui = useUiStore()
            ui.recordImportantEvent({
                title: i18n.global.t(titleKey, { name }),
                titleKey,
                titleParams: { name },
                message: message.fallback,
                messageKey: message.key,
                messageParams: message.params,
                tone: info.status === 'errored' ? 'danger' : 'warning',
                source: `runtime:${info.providerId}:${info.tunnelId}`,
            })
        },
        pruneRuntime(resources?: TunnelResource[]) {
            const keys = new Set((resources ?? this.resources).map(resourceKey))
            const runtime = Object.fromEntries(
                Object.entries(this.runtime).filter(([key]) => keys.has(key)),
            )
            this.runtime = runtime
        },
    },
})
