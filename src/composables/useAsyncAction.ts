import { computed, reactive } from 'vue'
import { useUiStore, type ToastTone } from '@/stores/ui'

export type ActionKey = string | null | undefined
export type ActionKeyInput = ActionKey | ActionKey[]
export type ActionMessage<T> =
  | string
  | false
  | null
  | undefined
  | ((value: T) => string | false | null | undefined)

export interface AsyncActionOptions<T> {
  key: string
  locks?: ActionKeyInput
  success?: ActionMessage<T>
  successTone?: ToastTone
  error?: ActionMessage<unknown>
  errorTone?: ToastTone
  rethrow?: boolean
}

interface ActiveAction {
  owner: string
}

const active = reactive<Record<string, ActiveAction>>({})

function keysOf(input: ActionKeyInput): string[] {
  if (Array.isArray(input)) return input.filter((key): key is string => !!key)
  return input ? [input] : []
}

function actionKeys(key: string, locks: ActionKeyInput): string[] {
  return Array.from(new Set([key, ...keysOf(locks)]))
}

function messageOf<T>(message: ActionMessage<T>, value: T): string | null {
  const resolved = typeof message === 'function' ? message(value) : message
  return resolved || null
}

function start(key: string, locks?: ActionKeyInput): boolean {
  const keys = actionKeys(key, locks)
  if (keys.some((item) => active[item])) return false
  for (const item of keys) active[item] = { owner: key }
  return true
}

function finish(key: string) {
  for (const [item, state] of Object.entries(active)) {
    if (state.owner === key) delete active[item]
  }
}

export function useAsyncAction() {
  const ui = useUiStore()

  function isBusy(input: ActionKeyInput): boolean {
    return keysOf(input).some((key) => !!active[key])
  }

  function isRunning(key: ActionKey): boolean {
    return !!key && active[key]?.owner === key
  }

  async function run<T>(options: AsyncActionOptions<T>, task: () => Promise<T>): Promise<T | undefined> {
    if (!start(options.key, options.locks)) return undefined
    try {
      const result = await task()
      const success = messageOf(options.success, result)
      if (success) ui.notify(success, options.successTone ?? 'success')
      return result
    } catch (error) {
      const failure = messageOf(options.error ?? ((reason) => String(reason)), error)
      if (failure) ui.notify(failure, options.errorTone ?? 'danger')
      if (options.rethrow) throw error
      return undefined
    } finally {
      finish(options.key)
    }
  }

  function track<T>(key: string, promise: Promise<T>, locks?: ActionKeyInput): Promise<T> {
    if (!start(key, locks)) return promise
    return promise.finally(() => finish(key))
  }

  return {
    active: computed(() => ({ ...active })),
    isBusy,
    isRunning,
    run,
    track,
  }
}
