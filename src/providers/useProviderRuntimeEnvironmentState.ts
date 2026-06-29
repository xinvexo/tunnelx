import { computed, reactive } from 'vue'
import type { ProviderRuntimeUpdateStatus, ProviderStatus } from '@/providers/contract'

export type ProviderRuntimeTask = 'refresh' | 'install' | 'uninstall' | 'checkUpdate' | 'update'

interface ProviderRuntimeEnvironmentState {
    status: ProviderStatus | null
    updateStatus: ProviderRuntimeUpdateStatus | null
    tasks: Record<ProviderRuntimeTask, boolean>
}

const states = reactive<Record<string, ProviderRuntimeEnvironmentState>>({})

function createState(): ProviderRuntimeEnvironmentState {
    return {
        status: null,
        updateStatus: null,
        tasks: {
            refresh: false,
            install: false,
            uninstall: false,
            checkUpdate: false,
            update: false,
        },
    }
}

function stateOf(providerId: string) {
    states[providerId] ??= createState()
    return states[providerId]
}

export function useProviderRuntimeEnvironmentState(providerId: string) {
    const state = stateOf(providerId)

    async function runRuntimeTask<T>(task: ProviderRuntimeTask, work: () => Promise<T>): Promise<T | undefined> {
        if (state.tasks[task]) return undefined
        state.tasks[task] = true
        try {
            return await work()
        } finally {
            state.tasks[task] = false
        }
    }

    return {
        status: computed({
            get: () => state.status,
            set: (value) => {
                state.status = value
            },
        }),
        updateStatus: computed({
            get: () => state.updateStatus,
            set: (value) => {
                state.updateStatus = value
            },
        }),
        refreshing: computed(() => state.tasks.refresh),
        installing: computed(() => state.tasks.install),
        uninstalling: computed(() => state.tasks.uninstall),
        checkingUpdate: computed(() => state.tasks.checkUpdate),
        updating: computed(() => state.tasks.update),
        runRuntimeTask,
    }
}
