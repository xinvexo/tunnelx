import { defineStore } from 'pinia'

export type ToastTone = 'info' | 'success' | 'warning' | 'danger'
export type ImportantEventTone = 'info' | 'warning' | 'danger'

export interface ToastItem {
  id: number
  message: string
  tone: ToastTone
  durationMs: number
}

export interface ImportantEventItem {
  id: number
  title: string
  titleKey?: string
  titleParams?: Record<string, unknown>
  message?: string
  messageKey?: string
  messageParams?: Record<string, unknown>
  tone: ImportantEventTone
  source?: string
  createdAt: number
}

export interface ImportantEventRequest {
  title?: string
  titleKey?: string
  titleParams?: Record<string, unknown>
  message?: string
  messageKey?: string
  messageParams?: Record<string, unknown>
  tone?: ImportantEventTone
  source?: string
}

/** 应用内确认框请求（替代原生 window.confirm，与应用视觉统一）。 */
export interface ConfirmRequest {
  title: string
  message?: string
  confirmLabel?: string
  cancelLabel?: string
  tone?: 'primary' | 'danger'
}

/** 应用内输入框请求（替代原生 window.prompt）。 */
export interface PromptRequest {
  title: string
  message?: string
  placeholder?: string
  defaultValue?: string
  confirmLabel?: string
  cancelLabel?: string
}

const IMPORTANT_EVENTS_LIMIT = 80
localStorage.removeItem('tunnelx.importantEvents')
localStorage.removeItem('tunnelx.importantEventsReadAt')
localStorage.removeItem('tunnelx.logFollow')

let nextToastId = 1
let nextImportantEventId = 1
// 确认/输入框的 resolve 回调放模块级、不进 Pinia state（与 runtime.ts 的定时器同理，避免被代理）。
let confirmResolver: ((ok: boolean) => void) | null = null
let promptResolver: ((value: string | null) => void) | null = null

export const useUiStore = defineStore('ui', {
  state: () => ({
    toasts: [] as ToastItem[],
    importantEvents: [] as ImportantEventItem[],
    importantEventsReadAt: 0,
    confirmRequest: null as ConfirmRequest | null,
    promptRequest: null as PromptRequest | null,
    logWrap: localStorage.getItem('tunnelx.logWrap') !== 'false',
    logFollow: true,
  }),
  getters: {
    unreadImportantEventCount: (state) =>
      state.importantEvents.filter((item) => item.createdAt > state.importantEventsReadAt).length,
  },
  actions: {
    notify(message: string, tone: ToastTone = 'info', durationMs = 3200) {
      const id = nextToastId++
      this.toasts.push({ id, message, tone, durationMs })
      window.setTimeout(() => this.dismiss(id), durationMs)
    },
    dismiss(id: number) {
      this.toasts = this.toasts.filter((item) => item.id !== id)
    },
    recordImportantEvent(request: ImportantEventRequest) {
      const title = request.title?.trim() ?? ''
      if (!title && !request.titleKey) return
      const now = Date.now()
      const top = this.importantEvents[0]
      if (
        top
        && top.title === title
        && top.titleKey === request.titleKey
        && JSON.stringify(top.titleParams ?? null) === JSON.stringify(request.titleParams ?? null)
        && top.message === request.message
        && top.messageKey === request.messageKey
        && JSON.stringify(top.messageParams ?? null) === JSON.stringify(request.messageParams ?? null)
        && top.source === request.source
        && top.tone === (request.tone ?? 'info')
        && now - top.createdAt < 5000
      ) {
        return
      }
      this.importantEvents = [
        {
          id: nextImportantEventId++,
          title,
          titleKey: request.titleKey,
          titleParams: request.titleParams,
          message: request.message?.trim() || undefined,
          messageKey: request.messageKey,
          messageParams: request.messageParams,
          tone: request.tone ?? 'info',
          source: request.source,
          createdAt: now,
        },
        ...this.importantEvents,
      ].slice(0, IMPORTANT_EVENTS_LIMIT)
    },
    markImportantEventsRead() {
      this.importantEventsReadAt = Date.now()
    },
    clearImportantEvents() {
      this.importantEvents = []
      this.markImportantEventsRead()
    },
    /** 弹出应用内确认框，返回用户是否确认的 Promise（替代原生 window.confirm）。 */
    confirm(request: ConfirmRequest): Promise<boolean> {
      // 已有未决确认时先按取消结算，避免上一个回调悬挂。
      confirmResolver?.(false)
      this.confirmRequest = request
      return new Promise<boolean>((resolve) => {
        confirmResolver = resolve
      })
    },
    /** 由确认框组件回调：结算当前确认并关闭。 */
    resolveConfirm(ok: boolean) {
      this.confirmRequest = null
      const resolve = confirmResolver
      confirmResolver = null
      resolve?.(ok)
    },
    /** 弹出应用内输入框，返回输入内容的 Promise；取消时为 null（替代原生 window.prompt）。 */
    prompt(request: PromptRequest): Promise<string | null> {
      promptResolver?.(null)
      this.promptRequest = request
      return new Promise<string | null>((resolve) => {
        promptResolver = resolve
      })
    },
    /** 由输入框组件回调：结算当前输入并关闭（取消传 null）。 */
    resolvePrompt(value: string | null) {
      this.promptRequest = null
      const resolve = promptResolver
      promptResolver = null
      resolve?.(value)
    },
    setLogWrap(value: boolean) {
      this.logWrap = value
      localStorage.setItem('tunnelx.logWrap', String(value))
    },
    setLogFollow(value: boolean) {
      this.logFollow = value
    },
  },
})
