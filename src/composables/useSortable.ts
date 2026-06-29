import { onBeforeUnmount, ref } from 'vue'

/**
 * 指针拖拽排序。
 *
 * 不用 HTML5 原生 drag —— Tauri webview 默认开启 dragDrop 拦截，会吞掉
 * dragover/drop 事件，导致原生拖拽“看着能拖、实际不生效”。Pointer 事件不受其影响，
 * 跨平台表现一致，且能完全控制拖拽视觉（无系统残影）。
 *
 * 视觉策略：被拖项跟随指针（translateY），其余行按需让位（开一个等高的空档），
 * 数据只在松手时提交一次，避免拖拽过程中反复触发持久化。
 */
export interface UseSortableOptions {
  /** 当前渲染顺序对应的 id 列表（顺序需与 DOM 中的行一致）。 */
  ids: () => string[]
  /** 松手且顺序变化后回调，参数为新的 id 顺序。 */
  onReorder: (ids: string[]) => void
  /** 行元素在其公共父容器内的选择器，例如 '.proxy-row'。 */
  itemSelector: string
  /** 按下后移动多少像素才判定为拖拽（小于此值视为点击），默认 4。 */
  threshold?: number
  /** 从触发元素查找真正拖拽行的选择器；用于只允许行内拖拽手柄触发。 */
  handleItemSelector?: string
}

interface Row {
  id: string
  el: HTMLElement
  height: number
  center: number
}

const INTERACTIVE = 'button, input, select, textarea, label, [role="switch"]'

export function useSortable(options: UseSortableOptions) {
  const threshold = options.threshold ?? 4
  const draggingId = ref<string | null>(null)

  let pressId: string | null = null
  let pressEl: HTMLElement | null = null
  let startY = 0
  let active = false
  let rows: Row[] = []
  let fromIndex = 0
  let targetIndex = 0

  function start(id: string, event: PointerEvent) {
    if (event.button !== 0) return
    // 按到行内的交互控件（开关、按钮等）时不发起拖拽，留给它们自己处理点击。
    const hit = (event.target as HTMLElement).closest(INTERACTIVE)
    const current = event.currentTarget as HTMLElement
    const row = options.handleItemSelector
      ? current.closest<HTMLElement>(options.handleItemSelector)
      : current
    if (!row) return
    if (!options.handleItemSelector && hit && hit !== row && row.contains(hit)) return

    pressId = id
    pressEl = row
    startY = event.clientY
    active = false
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
    window.addEventListener('pointercancel', onCancel)
  }

  function activate(): boolean {
    const container = pressEl!.parentElement
    if (!container) return false
    const els = Array.from(container.querySelectorAll<HTMLElement>(options.itemSelector))
    const ids = options.ids()
    rows = els.map((el, i) => {
      const rect = el.getBoundingClientRect()
      return { id: ids[i], el, height: rect.height, center: rect.top + rect.height / 2 }
    })
    fromIndex = rows.findIndex((row) => row.id === pressId)
    if (fromIndex < 0) {
      rows = []
      return false
    }
    active = true
    draggingId.value = pressId
    targetIndex = fromIndex
    document.body.classList.add('tx-sorting')
    for (const row of rows) {
      if (row.el === pressEl) {
        row.el.style.position = 'relative'
        row.el.style.zIndex = '30'
        row.el.style.transition = 'none'
        row.el.classList.add('tx-sort-lifted')
      } else {
        row.el.style.transition = 'transform 180ms cubic-bezier(0.2, 0, 0, 1)'
        row.el.style.pointerEvents = 'none'
      }
    }
    return true
  }

  function onMove(event: PointerEvent) {
    if (pressId == null) return
    const dy = event.clientY - startY
    if (!active) {
      if (Math.abs(dy) < threshold) return
      if (!activate()) {
        cleanup()
        return
      }
    }
    event.preventDefault()
    pressEl!.style.transform = `translateY(${dy}px)`

    const draggedCenter = rows[fromIndex].center + dy
    let target = 0
    for (const row of rows) {
      if (row.id !== pressId && draggedCenter > row.center) target++
    }
    if (target !== targetIndex) {
      targetIndex = target
      layoutGap()
    }
  }

  /** 让非拖拽行平移一个行高，给被拖项在 targetIndex 处腾出空档。 */
  function layoutGap() {
    const gap = rows[fromIndex].height
    rows.forEach((row, i) => {
      if (row.id === pressId) return
      let shift = 0
      if (targetIndex > fromIndex && i > fromIndex && i <= targetIndex) shift = -gap
      else if (targetIndex < fromIndex && i >= targetIndex && i < fromIndex) shift = gap
      row.el.style.transform = shift ? `translateY(${shift}px)` : ''
    })
  }

  function onUp() {
    if (active) {
      if (targetIndex !== fromIndex) {
        const ids = options.ids().slice()
        const [moved] = ids.splice(fromIndex, 1)
        ids.splice(targetIndex, 0, moved)
        options.onReorder(ids)
      }
      // 吞掉拖拽结束时浏览器补发的那次 click，避免误触导航/按钮。
      suppressNextClick()
    }
    cleanup()
  }

  function onCancel() {
    cleanup()
  }

  function cleanup() {
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', onUp)
    window.removeEventListener('pointercancel', onCancel)
    if (active) {
      document.body.classList.remove('tx-sorting')
      for (const row of rows) {
        row.el.style.transition = ''
        row.el.style.transform = ''
        row.el.style.position = ''
        row.el.style.zIndex = ''
        row.el.style.pointerEvents = ''
        row.el.classList.remove('tx-sort-lifted')
      }
    }
    rows = []
    pressId = null
    pressEl = null
    active = false
    draggingId.value = null
  }

  onBeforeUnmount(cleanup)

  return { draggingId, start }
}

/** 在捕获阶段拦截一次 click，用于拖拽松手后抑制误触发的点击。 */
function suppressNextClick() {
  let timer = 0
  const handler = (event: MouseEvent) => {
    event.preventDefault()
    event.stopPropagation()
    window.removeEventListener('click', handler, true)
    window.clearTimeout(timer)
  }
  window.addEventListener('click', handler, true)
  timer = window.setTimeout(() => window.removeEventListener('click', handler, true), 350)
}
