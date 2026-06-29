import { defineStore } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import type { ProviderRuntimeUpdateStatus } from '@/providers/contract'
import type { DownloadProgress, RemoteVersion } from '@/providers/frp/domain/version'
import * as api from '@/providers/frp/api/version'

interface State {
    versions: RemoteVersion[]
    activeVersion: string | null
    mirror: string
    remoteError: string | null
    loading: boolean
    loaded: boolean
    installing: Record<string, boolean>
    progress: Record<string, { received: number; total: number }>
    listening: boolean
}

export const useVersionStore = defineStore('version', {
    state: (): State => ({
        versions: [],
        activeVersion: null,
        mirror: '',
        remoteError: null,
        loading: false,
        loaded: false,
        installing: {},
        progress: {},
        listening: false,
    }),
    getters: {
        installed: (s) => s.versions.filter((v) => v.installed),
        hasActive: (s) => !!s.activeVersion,
    },
    actions: {
        async init() {
            if (this.listening) return
            this.listening = true
            await listen<string>('versions-updated', () => {
                void this.load()
            })
            await listen<string | null>('frpc-version-activated', (e) => {
                this.markActive(e.payload)
            })
            await listen<DownloadProgress>('frpc-download-progress', (e) => {
                this.progress[e.payload.version] = {
                    received: e.payload.received,
                    total: e.payload.total,
                }
            })
            await this.loadActive()
            await this.loadMirror()
            // 启动后台预取一次，进入版本页时直接用缓存。
            void this.ensureLoaded()
        },
        /** 强制拉取完整版本列表（会请求 GitHub），刷新按钮调用。 */
        async load() {
            this.loading = true
            try {
                const list = await api.listFrpcVersions()
                this.versions = list.versions
                this.remoteError = list.remoteError
                this.markActive(await api.activeFrpcVersion())
                this.loaded = true
            } finally {
                this.loading = false
            }
        },
        /** 用缓存：已加载过则不再请求，避免每次进版本页都等待。 */
        async ensureLoaded() {
            if (this.loaded || this.loading) return
            await this.load()
        },
        /** 仅读取已激活版本（本地、无网络），启动时调用。 */
        async loadActive() {
            this.markActive(await api.activeFrpcVersion())
        },
        /** 读取已保存的下载镜像前缀。 */
        async loadMirror() {
            this.mirror = await api.getFrpcMirror()
        },
        /** 设置下载镜像前缀并持久化。镜像只影响下载链接生成，不需要重新拉取版本列表。 */
        async setMirror(mirror: string) {
            await api.setFrpcMirror(mirror)
            this.mirror = mirror
            // 注意：镜像在后端 apply_mirror() 中应用到下载链接，前端只需保存设置。
            // 已加载的版本列表的 downloadUrl 在下次 install 时会自动使用新镜像。
            // 如需立即看到新镜像的链接效果，可调用 refresh()
        },
        /** 手动刷新版本列表（查看镜像链接效果时可选调用） */
        async refresh() {
            await this.load()
        },
        async checkUpdate(): Promise<ProviderRuntimeUpdateStatus> {
            const result = await api.checkFrpcUpdate()
            if (!result.message) {
                await this.loadActive()
            }
            return result
        },
        async install(version: string) {
            this.installing[version] = true
            this.progress[version] = { received: 0, total: 0 }
            try {
                await api.installFrpcVersion(version)
            } finally {
                this.installing[version] = false
                delete this.progress[version]
            }
        },
        markActive(version: string | null) {
            this.activeVersion = version
            this.versions = this.versions.map((item) => ({
                ...item,
                active: version === item.version,
            }))
        },
        async activate(version: string): Promise<boolean> {
            if (this.activeVersion === version) return false
            await api.activateFrpcVersion(version)
            this.markActive(version)
            return true
        },
        async remove(version: string) {
            await api.removeFrpcVersion(version)
            // 后端确认删除后立即同步本地列表，不必等 versions-updated → load() 的网络往返
            // （load() 会重新请求 GitHub，慢且离线时还会触发 remoteError 警告）。
            this.markRemoved(version)
        },
        markRemoved(version: string) {
            // 与后端 list() 语义对齐：远端仍提供的版本退回「可下载」（保留该行），
            // 非远端来源（无 downloadUrl）的版本删除后整行移除。
            this.versions = this.versions.flatMap((item) => {
                if (item.version !== version) return [item]
                if (!item.downloadUrl) return []
                return [{ ...item, installed: false, active: false }]
            })
        },
        async uninstallActive() {
            const active = this.activeVersion
            await api.uninstallActiveFrpcVersion()
            this.markActive(null)
            if (active) this.markRemoved(active)
        },
    },
})
