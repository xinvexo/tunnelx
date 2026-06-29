import { createI18n } from 'vue-i18n'
import zhCN from './locales/zh-CN'
import enUS from './locales/en-US'

export type Locale = 'zh-CN' | 'en-US'

/// 语言选择持久化到 localStorage 的 key。
const STORAGE_KEY = 'tunnelx.locale'

export const SUPPORTED_LOCALES: Locale[] = ['zh-CN', 'en-US']

/// 兜底语言：未命中浏览器语言时使用。
const FALLBACK_LOCALE: Locale = 'zh-CN'

/// 语言下拉选项（供设置页 TxSelect 使用）。
export const LOCALE_OPTIONS: { label: string; value: Locale }[] = [
  { label: '简体中文', value: 'zh-CN' },
  { label: 'English', value: 'en-US' },
]

/// 把任意 BCP-47 语言标签归一到受支持的 locale，无法识别时返回 null。
function normalizeLocale(input: string): Locale | null {
  const lower = input.toLowerCase()
  if (lower.startsWith('zh')) return 'zh-CN'
  if (lower.startsWith('en')) return 'en-US'
  return null
}

/// 启动时决定初始语言：已保存的偏好 > 浏览器语言 > 兜底。
function detectLocale(): Locale {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored && (SUPPORTED_LOCALES as string[]).includes(stored)) {
    return stored as Locale
  }
  for (const lang of navigator.languages ?? [navigator.language]) {
    const normalized = normalizeLocale(lang)
    if (normalized) return normalized
  }
  return FALLBACK_LOCALE
}

export const i18n = createI18n({
  legacy: false,
  locale: detectLocale(),
  fallbackLocale: FALLBACK_LOCALE,
  messages: {
    'zh-CN': zhCN,
    'en-US': enUS,
  },
})

/// 当前语言（函数形式，供模板外的逻辑读取，如同步托盘语言）。
export function currentLocale(): Locale {
  return i18n.global.locale.value as Locale
}

/// 切换语言：更新 i18n、持久化、并同步 <html lang>。
export function setLocale(locale: Locale): void {
  i18n.global.locale.value = locale
  localStorage.setItem(STORAGE_KEY, locale)
  document.documentElement.lang = locale
}
