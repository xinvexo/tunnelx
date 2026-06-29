import { useUiStore } from '@/stores/ui'

/**
 * 统一的"复制到剪贴板"。把各处重复的 `writeText + try/catch + notify`
 * 收口成一处：成功提示 successMessage，失败提示错误。返回是否成功。
 */
export function useClipboard() {
    const ui = useUiStore()

    async function copyText(text: string, successMessage: string): Promise<boolean> {
        try {
            await navigator.clipboard.writeText(text)
            ui.notify(successMessage, 'success')
            return true
        } catch (error) {
            ui.notify(String(error), 'danger')
            return false
        }
    }

    return { copyText }
}
