import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import type { Profile, ProfileSummary } from '@/providers/frp/domain/profile'
import { hydrateProfile } from '@/providers/frp/domain/profile'
import type { ProfileProxy } from '@/providers/frp/domain/proxy'
import * as api from '@/providers/frp/api/profile'
import { updateProxies } from '@/providers/frp/api/proxy'
import { useRuntimeStore } from '@/providers/frp/stores/runtime'

interface State {
    summaries: ProfileSummary[]
    current: Profile | null
    loaded: boolean
    listening: boolean
}

export const useProfileStore = defineStore('profile', {
    state: (): State => ({
        summaries: [],
        current: null,
        loaded: false,
        listening: false,
    }),
    getters: {
        has: (s) => (id: string) => s.summaries.some((p) => p.id === id),
        count: (s) => s.summaries.length,
    },
    actions: {
        /** 注册事件监听并首次加载，进程内只跑一次。 */
        async init() {
            if (!this.listening) {
                this.listening = true
                try {
                    await listen<ProfileSummary[]>('profiles-updated', (e) => {
                        this.summaries = e.payload
                    })
                    await listen<Profile>('profile-updated', (e) => {
                        if (this.current && this.current.id === e.payload.id)
                            this.current = hydrateProfile(e.payload)
                    })
                } catch {
                    // Browser previews do not have the Tauri event bridge.
                }
            }
            if (!this.loaded) await this.load()
        },
        async load() {
            this.summaries = await api.listProfiles()
            this.loaded = true
        },
        async loadCurrent(id: string) {
            this.current = hydrateProfile(await api.getProfile(id))
            return this.current
        },
        forgetDeleted(id: string) {
            useRuntimeStore().forgetProfile(id)
            this.summaries = this.summaries.filter((item) => item.id !== id)
            if (this.current?.id === id) this.current = null
        },
        async reorder(ids: string[]) {
            const byId = new Map(this.summaries.map((item) => [item.id, item]))
            this.summaries = ids.map((id) => byId.get(id)).filter((item): item is ProfileSummary => !!item)
            await api.reorderProfiles(ids)
            await this.load()
        },
        /** 保存整个 Profile（服务器设置变更后调用）。 */
        async save(profile: Profile) {
            this.current = hydrateProfile(await api.updateProfile(profile))
            await this.load()
            return this.current
        },
        /** 仅保存隧道列表（增删改/启停/拖拽排序时调用）。先乐观更新 UI，失败再回滚。 */
        async saveProxies(proxies: ProfileProxy[]) {
            const current = this.current
            if (!current) return
            const previous = current.proxies
            this.current = { ...current, proxies }
            try {
                await updateProxies(current.id, proxies)
            } catch (error) {
                this.current = { ...current, proxies: previous }
                throw error
            }
            await this.load()
            // update_proxies 会让运行中的 frpc 热重载；立即同步一次 admin 状态，
            // 避免新增、重命名、删除或启停隧道后继续显示热重载前的状态。
            const runtime = useRuntimeStore()
            if (runtime.isRunning(current.id)) {
                runtime.startProxyPolling(current.id)
                runtime.startTrafficPolling(current.id)
                await runtime.pollProxyStatus(current.id)
                await runtime.pollTrafficStats(current.id)
            }
        },
    },
})
