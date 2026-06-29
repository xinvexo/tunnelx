import type { Component } from 'vue'
import type { ProviderDescriptor, TunnelResource, TunnelRuntimeInfo } from './contract'

export interface ProviderTunnelPreviewRow {
    key: string
    name: string
    target: string
    kind: string
    publicUrls?: string[]
}

export interface ProviderQuotaRow {
    key: string
    label: string
    value: string
    tone?: 'default' | 'warning' | 'danger'
}

export interface ProviderFrontendModule {
    descriptor: ProviderDescriptor
    icon: string
    homepageUrl?: string
    iconAsset?: string
    settingsPanel?: Component
    tunnelsPanel?: Component
    environmentPanel?: Component
    init?: () => Promise<void> | void
    hydrateConnection?: (resource: TunnelResource) => Promise<void> | void
    cleanupConnection?: (resource: TunnelResource) => Promise<void> | void
    resourceConnectionInfo?: (resource: TunnelResource, runtime?: TunnelRuntimeInfo) => string
    resourceTunnelText?: (resource: TunnelResource, runtime?: TunnelRuntimeInfo) => string
    resourceTunnelCount?: (resource: TunnelResource, runtime?: TunnelRuntimeInfo) => number
    resourceTunnelRows?: (resource: TunnelResource, runtime?: TunnelRuntimeInfo) => ProviderTunnelPreviewRow[]
    resourceQuotaRows?: (resource: TunnelResource, runtime?: TunnelRuntimeInfo) => ProviderQuotaRow[]
}
