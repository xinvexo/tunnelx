import { invoke } from '@tauri-apps/api/core'
import { i18n } from '@/i18n'

/** 后端 AppError 的序列化形态（src-tauri/src/error.rs）。 */
interface BackendError {
    code: string
    message: string
    data: Record<string, unknown> | null
}

export interface LocalizedMessageSpec {
    key?: string
    params?: Record<string, unknown>
    fallback: string
}

export class BackendInvokeError extends Error {
    readonly code: string
    readonly data: Record<string, unknown> | null
    readonly fallback: string

    constructor(error: BackendError) {
        super(backendErrorMessage(error))
        this.name = 'BackendInvokeError'
        this.code = error.code
        this.data = error.data
        this.fallback = error.message
    }
}

function isBackendError(e: unknown): e is BackendError {
    return !!e && typeof e === 'object'
        && typeof (e as any).code === 'string'
        && typeof (e as any).message === 'string'
}

/**
 * 把后端错误转成当前界面语言的提示文案：有 `backendErrors.<code>` 词条时用它渲染
 * （带 data 插值），否则回退到后端自带的 message（Msg/Io 等动态文本暂未本地化）。
 */
function backendErrorMessage(e: unknown): string {
    if (isBackendError(e)) {
        const key = `backendErrors.${e.code}`
        if (i18n.global.te(key)) return i18n.global.t(key, e.data ?? {})
        return e.message
    }
    if (typeof e === 'string') return e
    return (e as any)?.message ?? String(e)
}

export function localizedMessageFromError(error: unknown): string {
    if (error instanceof BackendInvokeError) {
        const key = `backendErrors.${error.code}`
        if (i18n.global.te(key)) return i18n.global.t(key, error.data ?? {})
        return error.fallback
    }
    return backendErrorMessage(error)
}

export function localizedMessageSpecFromError(error: unknown): LocalizedMessageSpec {
    if (error instanceof BackendInvokeError) {
        const key = `backendErrors.${error.code}`
        if (i18n.global.te(key)) {
            return { key, params: error.data ?? {}, fallback: error.fallback }
        }
        return { fallback: error.fallback }
    }
    return { fallback: backendErrorMessage(error) }
}

export async function call<T>(cmd: string, payload?: any): Promise<T> {
    try {
        return await invoke<T>(cmd, payload)
    } catch (e: unknown) {
        if (isBackendError(e)) throw new BackendInvokeError(e)
        throw new Error(backendErrorMessage(e))
    }
}
