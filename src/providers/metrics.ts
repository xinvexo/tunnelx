import { defineStore } from 'pinia'
import type { RuntimeTrafficSummary } from './contract'
import * as api from './api'

interface State {
    runtime: RuntimeTrafficSummary | null
    loading: boolean
}

export const useProviderMetricsStore = defineStore('providerMetrics', {
    state: (): State => ({
        runtime: null,
        loading: false,
    }),
    getters: {
        appMemoryBytes: (state): number | null => state.runtime?.appMemoryBytes ?? null,
        downloadSpeed: (state): number => state.runtime?.downloadSpeed ?? 0,
        uploadSpeed: (state): number => state.runtime?.uploadSpeed ?? 0,
        hasActiveTunnels: (state): boolean => !!state.runtime?.hasActiveTunnels,
        trafficHistory: (state) => state.runtime?.history ?? [],
    },
    actions: {
        async refresh() {
            if (this.loading) return this.runtime
            this.loading = true
            try {
                this.runtime = await api.runtimeTrafficSummary()
                return this.runtime
            } finally {
                this.loading = false
            }
        },
    },
})
