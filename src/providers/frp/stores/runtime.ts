import { defineStore } from 'pinia'
import { watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import type { AppSettings } from '@/domain/settings'
import type { FrpcConfigDirty, FrpcStatus, FrpcStatusChange, ProxyStatusInfo, TrafficStats } from '@/providers/frp/domain/version'
import { isLiveStatus, isTerminalStatus } from '@/providers/frp/domain/version'
import * as api from '@/providers/frp/api/runtime'
import { i18n } from '@/i18n'
import { providerTunnelLogs } from '@/providers/api'
import { FRP_PROVIDER_ID, type TunnelRuntimeLogEvent } from '@/providers/contract'
import { useProfileStore } from '@/providers/frp/stores/profile'
import { useSettingsStore } from '@/stores/settings'
import { useUiStore } from '@/stores/ui'

/** admin API 隧道状态轮询间隔（毫秒）。 */
const PROXY_POLL_MS = 2000
const TRAFFIC_POLL_MS = 1000
/** start/stop 命令发出后，等待看门狗事件把状态推进到终态的最长时间（毫秒）。
 *  正常情况下进程一拉起/退出就有事件，这里只是兜底防止状态永远卡在 starting/stopping。 */
const START_SETTLE_MS = 20_000
const STOP_SETTLE_MS = 15_000
const TRAFFIC_HISTORY_KEEP_MS = 30_000
const ANSI_PATTERN = /\u001b(?:\[[0-?]*[ -/]*[@-~]|[@-_])/g
const ATTENTION_PATTERN = /(?:\sstart error:|\serror:|\salready exists|\[e\])/i
export type ProxyRunStatus = 'idle' | 'running' | 'warning'

function cleanLogLine(line: string): string {
    return line.replace(ANSI_PATTERN, '').replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, '')
}

function applyProxyStatusLine(statuses: Record<string, ProxyRunStatus>, line: string) {
    const added = line.match(/proxy added:\s*\[([^\]]+)\]/i)
    if (added) {
        for (const name of added[1].split(/\s+/).filter(Boolean)) statuses[name] = 'running'
    }

    const startError = line.match(/\[([^\]]+)\]\s+start error:/i)
    if (startError) statuses[startError[1]] = 'warning'

    const startSuccess = line.match(/\[([^\]]+)\]\s+start proxy success/i)
    if (startSuccess) statuses[startSuccess[1]] = 'running'

    const alreadyExists = line.match(/proxy \[([^\]]+)\] already exists/i)
    if (alreadyExists) statuses[alreadyExists[1]] = 'warning'
}

function allEnabledProxiesRunning(profileId: string, statuses: Record<string, ProxyRunStatus>) {
    const profile = useProfileStore().current
    if (!profile || profile.id !== profileId) return false
    const enabled = profile.proxies.filter((proxy) => proxy.enabled)
    return enabled.length > 0 && enabled.every((proxy) => statuses[proxy.config.name] === 'running')
}

function proxyDetailStatus(detail: ProxyStatusInfo | undefined): ProxyRunStatus | null {
    if (!detail) return null
    const status = detail.status.trim().toLowerCase()
    if (status === 'running') return 'running'
    if (detail.err.trim() || /(?:error|fail|closed)/i.test(status)) return 'warning'
    return 'idle'
}

interface State {
    status: Record<string, FrpcStatus>
    proxyStatus: Record<string, Record<string, ProxyRunStatus>>
    /** admin API 拉取的每条隧道结构化详情（含公网地址、错误），key = profileId。 */
    proxyDetail: Record<string, ProxyStatusInfo[]>
    /** 实时流量数据：只代表当前还在运行的连接，供速度数字和隧道列表使用。 */
    trafficStats: Record<string, TrafficStats[]>
    /** 图表历史数据：停止连接后继续保留短时间，让全局图自然滑成直线。 */
    trafficHistory: Record<string, TrafficStats[]>
    needsRestart: Record<string, boolean>
    attentionNotified: Record<string, boolean>
    listening: boolean
}

/** 运行中各连接的 admin 状态轮询定时器（不进 state，避免被 Pinia 代理/持久化）。 */
const pollTimers = new Map<string, ReturnType<typeof setInterval>>()
const trafficTimers = new Map<string, ReturnType<typeof setInterval>>()
const proxyPollInFlight = new Map<string, Promise<void>>()
const trafficPollInFlight = new Map<string, Promise<void>>()
const proxyPollVersions = new Map<string, number>()
const trafficPollVersions = new Map<string, number>()

function pollingPausedByVisibility(): boolean {
    return typeof document !== 'undefined' && document.hidden
}

function currentPollVersion(versions: Map<string, number>, id: string): number {
    return versions.get(id) ?? 0
}

function bumpPollVersion(versions: Map<string, number>, id: string): number {
    const next = currentPollVersion(versions, id) + 1
    versions.set(id, next)
    return next
}

/** 隧道详情逐字段比较：内容没变就不替换数组，避免每 2s 用新数组身份白白触发整表重渲染。 */
function proxyDetailsEqual(a: ProxyStatusInfo[] | undefined, b: ProxyStatusInfo[]): boolean {
    if (!a || a.length !== b.length) return false
    return a.every((x, i) => {
        const y = b[i]
        return x.name === y.name && x.status === y.status && x.kind === y.kind
            && x.publicAddr === y.publicAddr
            && (x.publicAddrs ?? []).join('\0') === (y.publicAddrs ?? []).join('\0')
            && x.remotePort === y.remotePort && x.remoteAddr === y.remoteAddr
            && x.localAddr === y.localAddr && x.err === y.err
    })
}

function latestTrafficSample(stat: TrafficStats) {
    return stat.history[stat.history.length - 1]
}

/** 流量逐项比较：字节/速率不变时仍要看 history 的尾部采样，否则侧边栏流量图会收不到空闲采样。 */
function trafficStatsEqual(a: TrafficStats[] | undefined, b: TrafficStats[]): boolean {
    if ((!a || a.length === 0) && b.length === 0) return true
    if (!a || a.length !== b.length) return false
    return a.every((x, i) => {
        const y = b[i]
        const xLast = latestTrafficSample(x)
        const yLast = latestTrafficSample(y)
        return x.proxyName === y.proxyName
            && x.downloadBytes === y.downloadBytes && x.uploadBytes === y.uploadBytes
            && x.downloadSpeed === y.downloadSpeed && x.uploadSpeed === y.uploadSpeed
            && x.history.length === y.history.length
            && (xLast?.timestamp ?? 0) === (yLast?.timestamp ?? 0)
            && (xLast?.downloadSpeed ?? 0) === (yLast?.downloadSpeed ?? 0)
            && (xLast?.uploadSpeed ?? 0) === (yLast?.uploadSpeed ?? 0)
    })
}

function mergeTrafficHistory(previous: TrafficStats | undefined, next: TrafficStats, now: number): TrafficStats {
    const cutoff = now - TRAFFIC_HISTORY_KEEP_MS
    const merged = new Map<number, TrafficStats['history'][number]>()
    for (const sample of previous?.history ?? []) {
        if (sample.timestamp >= cutoff) merged.set(sample.timestamp, sample)
    }
    for (const sample of next.history) {
        if (sample.timestamp >= cutoff) merged.set(sample.timestamp, sample)
    }
    return {
        ...next,
        history: Array.from(merged.values()).sort((a, b) => a.timestamp - b.timestamp),
    }
}

function mergeTrafficStats(previous: TrafficStats[] | undefined, next: TrafficStats[]): TrafficStats[] {
    const now = Date.now()
    const previousByName = new Map((previous ?? []).map((stat) => [stat.proxyName, stat]))
    const nextNames = new Set(next.map((stat) => stat.proxyName))
    const merged = next
        .map((stat) => mergeTrafficHistory(previousByName.get(stat.proxyName), stat, now))
        .filter((stat) => stat.history.length > 0)

    for (const stat of previous ?? []) {
        if (nextNames.has(stat.proxyName)) continue
        const history = stat.history.filter((sample) => sample.timestamp >= now - TRAFFIC_HISTORY_KEEP_MS)
        if (!history.length) continue
        merged.push({
            ...stat,
            downloadSpeed: 0,
            uploadSpeed: 0,
            lastUpdatedAt: now,
            history,
        })
    }

    return merged
}

function pruneTrafficStatsHistory(list: TrafficStats[] | undefined, now = Date.now()): TrafficStats[] {
    if (!list?.length) return []
    const cutoff = now - TRAFFIC_HISTORY_KEEP_MS
    return list
        .map((stat) => ({ ...stat, history: stat.history.filter((sample) => sample.timestamp >= cutoff) }))
        .filter((stat) => stat.history.length > 0)
}

export const useRuntimeStore = defineStore('runtime', {
    state: (): State => ({
        status: {},
        proxyStatus: {},
        proxyDetail: {},
        trafficStats: {},
        trafficHistory: {},
        needsRestart: {},
        attentionNotified: {},
        listening: false,
    }),
    getters: {
        statusOf: (s) => (id: string): FrpcStatus => s.status[id] ?? 'stopped',
        isRunning: (s) => (id: string): boolean => isLiveStatus(s.status[id]),
        proxyStatusOf: (s) => (profileId: string, name: string): ProxyRunStatus => {
            const detail = s.proxyDetail[profileId]?.find((item) => item.name === name)
            return proxyDetailStatus(detail) ?? s.proxyStatus[profileId]?.[name] ?? 'idle'
        },
        /** admin API 拉取的某连接全部隧道详情（含公网地址）；未就绪时为空数组。 */
        proxyDetailsOf: (s) => (profileId: string): ProxyStatusInfo[] =>
            s.proxyDetail[profileId] ?? [],
        /** 某条隧道的 admin 详情（按名字匹配）；找不到返回 undefined。 */
        proxyDetailOf: (s) => (profileId: string, name: string): ProxyStatusInfo | undefined =>
            s.proxyDetail[profileId]?.find((p) => p.name === name),
        trafficStatsOf: (s) => (profileId: string): TrafficStats[] =>
            s.trafficStats[profileId] ?? [],
        trafficHistoryOf: (s) => (profileId: string): TrafficStats[] =>
            s.trafficHistory[profileId] ?? [],
        trafficStatOf: (s) => (profileId: string, name: string): TrafficStats | undefined =>
            s.trafficStats[profileId]?.find((item) => item.proxyName === name),
        needsRestartOf: (s) => (id: string): boolean => s.needsRestart[id] ?? false,
    },
    actions: {
        async init() {
            if (this.listening) return
            try {
                await useSettingsStore().init()
            } catch {
                // Browser previews do not have a Tauri bridge; runtime polling will fail softly too.
            }
            this.listening = true
            // 窗口隐藏到托盘/最小化时暂停所有轮询，回前台再恢复，避免后台常驻时每秒空跑
            // admin 状态(2s)+流量(1s)查询。事件不触发时仍按原轮询节奏恢复。
            if (typeof document !== 'undefined') {
                document.addEventListener('visibilitychange', () => {
                    if (document.hidden) this.pauseAllPolling()
                    else this.resumeAllPolling()
                })
            }
            await listen<FrpcStatusChange>('frpc-status-changed', (e) => {
                const pid = e.payload.profileId
                this.status[pid] = e.payload.status
                if (e.payload.status === 'stopped') this.attentionNotified[pid] = false
                // 进程自行退出（stopped/errored）时停掉轮询，避免对已死的 admin 端口空转。
                if (isTerminalStatus(e.payload.status)) {
                    this.stopProxyPolling(pid)
                    this.stopTrafficPolling(pid)
                    this.proxyDetail[pid] = []
                    this.clearLiveTrafficStats(pid)
                } else if (isLiveStatus(e.payload.status)) {
                    this.startProxyPolling(pid)
                    this.startTrafficPolling(pid)
                }
            })
            await listen<FrpcConfigDirty>('frpc-config-dirty', (e) => {
                this.needsRestart[e.payload.profileId] = e.payload.needsRestart
            })
            await listen<AppSettings>('settings-updated', (e) => {
                this.applyTrafficStatsEnabled(e.payload.trafficStatsEnabled)
            })
            await listen<TunnelRuntimeLogEvent>('provider-tunnel-log', (e) => {
                if (e.payload.providerId !== FRP_PROVIDER_ID) return
                if (e.payload.reset) {
                    this.proxyStatus[e.payload.tunnelId] = {}
                    this.attentionNotified[e.payload.tunnelId] = false
                    return
                }
                this.applyLogDerivedStatus(e.payload.tunnelId, e.payload.line)
            })
        },
        async refresh(id: string) {
            const [status, needsRestart] = await Promise.all([
                api.frpcStatus(id),
                api.frpcNeedsRestart(id),
            ])
            this.status[id] = status
            this.needsRestart[id] = needsRestart
            // 恢复场景（如刷新页面时连接已在跑）：补开轮询，否则已运行的连接拿不到隧道详情。
            if (isLiveStatus(status)) {
                this.startProxyPolling(id)
                this.startTrafficPolling(id)
            } else {
                this.stopProxyPolling(id)
                this.stopTrafficPolling(id)
                this.clearLiveTrafficStats(id)
            }
        },
        async start(id: string) {
            const previous = this.statusOf(id)
            this.proxyStatus[id] = {}
            this.proxyDetail[id] = []
            this.clearLiveTrafficStats(id)
            // 先乐观置 'starting'（后端也会同步 emit 同样的 starting）。立即置位、不在 await
            // 之后再写回，避免抢先到达的 running/errored 事件被过期的返回值盖掉。
            this.status[id] = 'starting'
            this.needsRestart[id] = false
            this.attentionNotified[id] = false
            this.startProxyPolling(id)
            this.startTrafficPolling(id)
            try {
                await api.startFrpc(id)
            } catch (error) {
                // 命令本身被拒（如已在运行/配置错误）：停掉轮询，把乐观置的 starting 回滚。
                // 真正的失败态（errored）由后端 mark_start_failed 的事件纠正。
                this.stopProxyPolling(id)
                this.stopTrafficPolling(id)
                if (this.status[id] === 'starting') this.status[id] = previous
                throw error
            }
            // 命令只代表「已让看门狗去拉起 frpc」，进程是否真的起来由看门狗事件推进状态。
            // 等状态离开 starting：running/warning（进程已起）才算启动成功，errored/stopped 视为失败。
            let settled: FrpcStatus
            try {
                settled = await this.waitForStatus(id, (s) => s !== 'starting', START_SETTLE_MS)
            } catch (error) {
                if (this.status[id] === 'starting') {
                    this.stopProxyPolling(id)
                    this.stopTrafficPolling(id)
                    this.clearLiveTrafficStats(id)
                    this.status[id] = previous
                }
                throw error
            }
            if (!isLiveStatus(settled)) {
                throw new Error(i18n.global.t('runtime.startFailed'))
            }
        },
        async stop(id: string) {
            // 先停轮询再发停止命令：避免停止期间又发出新一轮查询。
            this.stopProxyPolling(id)
            this.stopTrafficPolling(id)
            this.clearLiveTrafficStats(id)
            const result = await api.stopFrpc(id)
            // 看门狗驱动的 'stopped'/'errored' 事件可能先于命令返回值到达；事件才是终态，
            // 不能用过期的 'stopping' 盖掉，否则下面的 waitForStatus 会等不到新事件而超时。
            if (!isTerminalStatus(this.status[id])) {
                this.status[id] = result
            }
            this.needsRestart[id] = false
            this.proxyStatus[id] = {}
            this.proxyDetail[id] = []
            this.clearLiveTrafficStats(id)
            // 命令返回 'stopping' 只表示已发出停止；等看门狗 exited 事件把状态推进到终态
            // (stopped/errored) 再返回，按钮的 loading/toast 才对应进程真正退出。
            if (!isTerminalStatus(this.statusOf(id))) {
                await this.waitForStatus(id, (s) => isTerminalStatus(s), STOP_SETTLE_MS)
            }
        },
        /** 事件驱动地等待某连接状态满足条件（如离开 starting / 进入终态）。
         *  立即命中则同步返回；否则监听 status 变化，超时 reject。不依赖轮询，靠后端事件推进。 */
        waitForStatus(
            id: string,
            predicate: (status: FrpcStatus) => boolean,
            timeoutMs: number,
        ): Promise<FrpcStatus> {
            const current = this.statusOf(id)
            if (predicate(current)) return Promise.resolve(current)
            return new Promise<FrpcStatus>((resolve, reject) => {
                const stopWatch = watch(
                    () => this.statusOf(id),
                    (status) => {
                        if (predicate(status)) {
                            clearTimeout(timer)
                            stopWatch()
                            resolve(status)
                        }
                    },
                )
                const timer = setTimeout(() => {
                    stopWatch()
                    reject(new Error(i18n.global.t('runtime.waitTimeout')))
                }, timeoutMs)
            })
        },
        /** 拉一次 admin API 隧道详情。后端返回 null 表示查询失败/admin 未就绪（保留既有数据），
         *  返回数组（即使为空）表示查询成功，直接覆盖——空数组会清掉已失效的过期地址。
         *  invoke 本身可能 reject（窗口销毁、命令未注册等），吞掉以免每 2s 抛未捕获 rejection。 */
        async pollProxyStatus(id: string) {
            const pending = proxyPollInFlight.get(id)
            if (pending) return pending
            const version = currentPollVersion(proxyPollVersions, id)
            let task: Promise<void> | undefined
            task = (async () => {
                try {
                    const list = await api.frpcProxyStatus(id)
                    if (list === null) return
                    // 等待响应期间连接可能已被 stop()/forgetProfile 清场：轮询已注销的就是过期
                    // 响应，写回去会让已停止的连接重新显示「运行中 + 公网地址」。
                    if (!pollTimers.has(id)) return
                    if (currentPollVersion(proxyPollVersions, id) !== version) return
                    if (!proxyDetailsEqual(this.proxyDetail[id], list)) {
                        this.proxyDetail[id] = list
                    }
                } catch {
                    // 查询失败等同于 null：保留既有数据，不清空。
                } finally {
                    if (task && proxyPollInFlight.get(id) === task) proxyPollInFlight.delete(id)
                }
            })()
            proxyPollInFlight.set(id, task)
            return task
        },
        /** 为某连接开启 admin 状态轮询（幂等：已在轮询则跳过）。立即拉一次再定时。 */
        startProxyPolling(id: string) {
            if (pollingPausedByVisibility()) return
            if (pollTimers.has(id)) return
            bumpPollVersion(proxyPollVersions, id)
            const timer = setInterval(() => void this.pollProxyStatus(id), PROXY_POLL_MS)
            pollTimers.set(id, timer)
            void this.pollProxyStatus(id)
        },
        /** 停止某连接的轮询并清掉定时器。 */
        stopProxyPolling(id: string) {
            const timer = pollTimers.get(id)
            if (timer) {
                clearInterval(timer)
                pollTimers.delete(id)
            }
            proxyPollInFlight.delete(id)
            bumpPollVersion(proxyPollVersions, id)
        },
        async pollTrafficStats(id: string) {
            const pending = trafficPollInFlight.get(id)
            if (pending) return pending
            if (!useSettingsStore().trafficStatsEnabled) {
                this.resetTrafficStats(id)
                return
            }
            const version = currentPollVersion(trafficPollVersions, id)
            let task: Promise<void> | undefined
            task = (async () => {
                try {
                    const incoming = await api.proxyTrafficStats(id)
                    if (!trafficTimers.has(id)) return
                    if (currentPollVersion(trafficPollVersions, id) !== version) return
                    const history = mergeTrafficStats(this.trafficHistory[id], incoming)
                    if (!trafficStatsEqual(this.trafficHistory[id], history)) {
                        if (history.length) this.trafficHistory[id] = history
                        else delete this.trafficHistory[id]
                    }
                    if (!trafficStatsEqual(this.trafficStats[id], incoming)) {
                        if (incoming.length) this.trafficStats[id] = incoming
                        else delete this.trafficStats[id]
                    }
                } catch {
                    // 普通浏览器预览或 Tauri bridge 不可用时忽略。
                } finally {
                    if (task && trafficPollInFlight.get(id) === task) trafficPollInFlight.delete(id)
                }
            })()
            trafficPollInFlight.set(id, task)
            return task
        },
        startTrafficPolling(id: string) {
            if (!useSettingsStore().trafficStatsEnabled) {
                this.resetTrafficStats(id)
                return
            }
            if (pollingPausedByVisibility()) return
            if (trafficTimers.has(id)) return
            bumpPollVersion(trafficPollVersions, id)
            const timer = setInterval(() => void this.pollTrafficStats(id), TRAFFIC_POLL_MS)
            trafficTimers.set(id, timer)
            void this.pollTrafficStats(id)
        },
        stopTrafficPolling(id: string) {
            const timer = trafficTimers.get(id)
            if (timer) {
                clearInterval(timer)
                trafficTimers.delete(id)
            }
            trafficPollInFlight.delete(id)
            bumpPollVersion(trafficPollVersions, id)
        },
        resetTrafficStats(id: string) {
            trafficPollInFlight.delete(id)
            bumpPollVersion(trafficPollVersions, id)
            delete this.trafficStats[id]
            delete this.trafficHistory[id]
        },
        clearLiveTrafficStats(id: string) {
            delete this.trafficStats[id]
            this.pruneTrafficHistory(id)
        },
        pruneTrafficHistory(id?: string) {
            if (id) {
                const history = pruneTrafficStatsHistory(this.trafficHistory[id])
                if (history.length) this.trafficHistory[id] = history
                else delete this.trafficHistory[id]
                return
            }
            for (const profileId of Object.keys(this.trafficHistory)) {
                const history = pruneTrafficStatsHistory(this.trafficHistory[profileId])
                if (history.length) this.trafficHistory[profileId] = history
                else delete this.trafficHistory[profileId]
            }
        },
        /** 窗口隐藏时暂停所有连接的轮询（保留 status，恢复时据此重开）。 */
        pauseAllPolling() {
            for (const id of [...pollTimers.keys()]) this.stopProxyPolling(id)
            for (const id of [...trafficTimers.keys()]) this.stopTrafficPolling(id)
        },
        /** 回到前台时，为仍处于运行/警告态的连接重开轮询。 */
        resumeAllPolling() {
            for (const id of Object.keys(this.status)) {
                if (isLiveStatus(this.status[id])) {
                    this.startProxyPolling(id)
                    this.startTrafficPolling(id)
                }
            }
        },
        applyTrafficStatsEnabled(enabled: boolean) {
            if (!enabled) {
                for (const id of [...trafficTimers.keys()]) this.stopTrafficPolling(id)
                const ids = new Set([...Object.keys(this.trafficStats), ...Object.keys(this.trafficHistory)])
                for (const id of ids) this.resetTrafficStats(id)
                return
            }
            for (const id of Object.keys(this.status)) {
                if (isLiveStatus(this.status[id])) {
                    this.clearLiveTrafficStats(id)
                    this.startTrafficPolling(id)
                }
            }
        },
        /** 连接被删除时调用：停轮询并清掉其运行态残留，避免定时器对已不存在的 id 空转。 */
        forgetProfile(id: string) {
            this.stopProxyPolling(id)
            this.stopTrafficPolling(id)
            proxyPollVersions.delete(id)
            trafficPollVersions.delete(id)
            delete this.proxyDetail[id]
            delete this.trafficStats[id]
            delete this.trafficHistory[id]
            delete this.status[id]
            delete this.proxyStatus[id]
            delete this.needsRestart[id]
            delete this.attentionNotified[id]
        },
        async refreshLogDerivedStatus(id: string) {
            const lines = (await providerTunnelLogs(FRP_PROVIDER_ID, id)).map(cleanLogLine)
            const statuses: Record<string, ProxyRunStatus> = {}
            for (const line of lines) applyProxyStatusLine(statuses, line)
            this.proxyStatus[id] = statuses
        },
        applyLogDerivedStatus(id: string, rawLine: string) {
            const line = cleanLogLine(rawLine)
            const statuses = this.proxyStatus[id] ?? (this.proxyStatus[id] = {})
            applyProxyStatusLine(statuses, line)
            if (this.status[id] === 'warning' && allEnabledProxiesRunning(id, statuses)) {
                this.status[id] = 'running'
            }
            if (ATTENTION_PATTERN.test(line) && !this.attentionNotified[id]) {
                this.attentionNotified[id] = true
                useUiStore().notify(i18n.global.t('runtime.logError'), 'warning')
            }
        },
    },
})
