import { computed, onMounted, reactive } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
    PROVIDER_RUNTIME_INSTALL_PROGRESS_EVENT,
    type ProviderRuntimeInstallProgress,
} from '@/providers/contract'

const progressByProvider = reactive<Record<string, ProviderRuntimeInstallProgress | null>>({})
let listening = false
let listenPromise: Promise<void> | null = null
let unlisten: UnlistenFn | null = null

function ensureProgressListener() {
    if (listening || listenPromise) return
    listenPromise = listen<ProviderRuntimeInstallProgress>(
        PROVIDER_RUNTIME_INSTALL_PROGRESS_EVENT,
        (event) => {
            progressByProvider[event.payload.providerId] = event.payload
        },
    )
        .then((nextUnlisten) => {
            unlisten = nextUnlisten
            listening = true
        })
        .catch(() => {
            unlisten = null
            listening = false
        })
        .finally(() => {
            listenPromise = null
        })
}

export function useRuntimeInstallProgress(providerId: string) {
    onMounted(() => {
        ensureProgressListener()
    })

    const progress = computed(() => progressByProvider[providerId] ?? null)
    const percent = computed(() => {
        const value = progress.value
        if (!value) return null
        if (typeof value.percent === 'number' && Number.isFinite(value.percent)) {
            return clampPercent(value.percent)
        }
        if (typeof value.totalBytes === 'number' && value.totalBytes > 0) {
            return clampPercent((value.receivedBytes / value.totalBytes) * 100)
        }
        return null
    })

    function reset() {
        progressByProvider[providerId] = null
    }

    return {
        runtimeInstallProgress: progress,
        runtimeInstallProgressPercent: percent,
        resetRuntimeInstallProgress: reset,
    }
}

function clampPercent(value: number) {
    return Math.max(0, Math.min(100, value))
}

if (import.meta.hot) {
    import.meta.hot.dispose(() => {
        unlisten?.()
        unlisten = null
        listening = false
        listenPromise = null
    })
}
