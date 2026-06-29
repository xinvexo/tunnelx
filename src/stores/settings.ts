import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import type { AppSettings } from '@/domain/settings'
import { getSettings, setSettings } from '@/api/settings'

interface State {
    theme: string | null
    silentStart: boolean
    autoConnect: boolean
    lightweightMode: boolean
    autoUpdate: boolean
    trafficStatsEnabled: boolean
    loaded: boolean
    listening: boolean
}

export const useSettingsStore = defineStore('settings', {
    state: (): State => ({
        theme: null,
        silentStart: false,
        autoConnect: false,
        lightweightMode: false,
        autoUpdate: false,
        trafficStatsEnabled: true,
        loaded: false,
        listening: false,
    }),
    actions: {
        async init() {
            if (!this.listening) {
                this.listening = true
                await listen<AppSettings>('settings-updated', (event) => {
                    this.apply(event.payload)
                })
            }
            if (!this.loaded) await this.load()
        },
        async load() {
            this.apply(await getSettings())
        },
        async saveCurrent() {
            await setSettings(
                this.silentStart,
                this.autoConnect,
                this.lightweightMode,
                this.autoUpdate,
                this.trafficStatsEnabled,
            )
        },
        apply(settings: AppSettings) {
            this.theme = settings.theme
            this.silentStart = settings.silentStart
            this.autoConnect = settings.autoConnect
            this.lightweightMode = settings.lightweightMode
            this.autoUpdate = settings.autoUpdate
            this.trafficStatsEnabled = settings.trafficStatsEnabled
            this.loaded = true
        },
    },
})
