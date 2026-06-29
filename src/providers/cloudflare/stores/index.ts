import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import type {
    CloudflareData,
    CloudflareTunnel,
    CloudflareZone,
    CloudflaredStatus,
} from '../domain'
import * as api from '../api'
import {
    providerCreateTunnel,
    providerStatus,
    providerUpdateTunnel,
} from '@/providers/api'
import {
    CLOUDFLARE_PROVIDER_ID,
    type ProviderCommandOutput,
    type ProviderRuntimeUpdateStatus,
    type ProviderStatus,
} from '@/providers/contract'
import { toCloudflareTunnel, toTunnelResource } from '../domain/mapper'

interface State {
    tunnels: CloudflareTunnel[]
    zones: CloudflareZone[]
    status: CloudflaredStatus | null
    loaded: boolean
    listening: boolean
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === 'object' && value !== null
}

function optionalString(value: unknown): string | null {
    return typeof value === 'string' ? value : null
}

function cloudflaredStatusFromProvider(status: ProviderStatus): CloudflaredStatus {
    const details = isRecord(status.details) ? status.details : {}
    return {
        available: typeof details.available === 'boolean' ? details.available : status.available,
        managed: typeof details.managed === 'boolean' ? details.managed : false,
        version: optionalString(details.version) ?? status.version,
        path: typeof details.path === 'string' ? details.path : '',
        certPath: optionalString(details.certPath),
        homeCredentialsDir: optionalString(details.homeCredentialsDir),
        managedCredentialsDir: typeof details.managedCredentialsDir === 'string'
            ? details.managedCredentialsDir
            : '',
    }
}

export const useCloudflareStore = defineStore('cloudflare', {
    state: (): State => ({
        tunnels: [],
        zones: [],
        status: null,
        loaded: false,
        listening: false,
    }),
    actions: {
        async init() {
            if (!this.listening) {
                this.listening = true
                try {
                    await listen<CloudflareData>('cloudflare-updated', (event) => {
                        this.apply(event.payload)
                    })
                } catch {
                    // Browser previews do not have the Tauri event bridge.
                }
            }
            if (!this.loaded) await this.load()
        },
        async load() {
            const data = await api.cloudflareData()
            this.apply(data)
            this.loaded = true
        },
        apply(data: CloudflareData) {
            this.tunnels = data.tunnels ?? []
            this.loaded = true
        },
        async refreshStatus() {
            this.status = cloudflaredStatusFromProvider(await providerStatus(CLOUDFLARE_PROVIDER_ID))
            return this.status
        },
        async installCloudflared() {
            this.status = await api.installCloudflared()
            return this.status
        },
        async uninstallCloudflared() {
            this.status = await api.uninstallCloudflared()
            return this.status
        },
        async checkCloudflaredUpdate(): Promise<ProviderRuntimeUpdateStatus> {
            return api.checkCloudflaredUpdate()
        },
        async loginConnection(tunnelId: string): Promise<ProviderCommandOutput> {
            return api.loginCloudflareConnection(tunnelId)
        },
        async refreshZones(tunnelId: string) {
            this.zones = await api.listCloudflareZones(tunnelId)
            return this.zones
        },
        async createTunnel(name: string) {
            const tunnel = toCloudflareTunnel(await providerCreateTunnel(CLOUDFLARE_PROVIDER_ID, { name }))
            await this.load()
            return tunnel
        },
        async saveTunnel(tunnel: CloudflareTunnel) {
            const saved = toCloudflareTunnel(
                await providerUpdateTunnel(CLOUDFLARE_PROVIDER_ID, toTunnelResource(tunnel)),
            )
            await this.load()
            return saved
        },
    },
})
