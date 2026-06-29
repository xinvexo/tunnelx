<template>
  <span
    ref="triggerRef"
    v-bind="$attrs"
    class="tx-tooltip group"
    :class="[`placement-${placement}`, `align-${align}`, { 'tx-tooltip-block': block }]"
    :tabindex="focusable ? 0 : undefined"
    :aria-label="focusable ? text : undefined"
    @mouseenter="show"
    @mouseleave="hide"
    @focusin="show"
    @focusout="hide"
  >
    <slot />
  </span>
  <Teleport to="body">
    <span
      v-if="visible"
      ref="bubbleRef"
      class="tx-tooltip-bubble tooltip-bubble floating-tooltip"
      :style="bubbleStyle"
      role="tooltip"
    >
      {{ text }}
    </span>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'

defineOptions({ inheritAttrs: false })

const props = withDefaults(
  defineProps<{
    text: string
    placement?: 'top' | 'bottom'
    align?: 'start' | 'center' | 'end'
    focusable?: boolean
    block?: boolean
  }>(),
  { placement: 'top', align: 'center' },
)

const triggerRef = ref<HTMLElement | null>(null)
const bubbleRef = ref<HTMLElement | null>(null)
const visible = ref(false)
const position = ref({ left: 0, top: 0 })
const bubbleStyle = computed(() => ({
  left: `${position.value.left}px`,
  top: `${position.value.top}px`,
}))
let frame = 0

function show() {
  if (visible.value) return
  visible.value = true
  window.addEventListener('scroll', schedulePosition, true)
  window.addEventListener('resize', schedulePosition)
  nextTick(schedulePosition)
}

function hide() {
  visible.value = false
  window.removeEventListener('scroll', schedulePosition, true)
  window.removeEventListener('resize', schedulePosition)
  if (frame) {
    cancelAnimationFrame(frame)
    frame = 0
  }
}

function schedulePosition() {
  if (frame) cancelAnimationFrame(frame)
  frame = requestAnimationFrame(updatePosition)
}

function updatePosition() {
  frame = 0
  const trigger = triggerRef.value
  const bubble = bubbleRef.value
  if (!trigger || !bubble) return

  const gap = 6
  const margin = 8
  const triggerRect = trigger.getBoundingClientRect()
  const bubbleRect = bubble.getBoundingClientRect()
  const viewportWidth = window.innerWidth
  const viewportHeight = window.innerHeight

  let top = props.placement === 'bottom'
    ? triggerRect.bottom + gap
    : triggerRect.top - bubbleRect.height - gap
  if (top < margin) top = triggerRect.bottom + gap
  if (top + bubbleRect.height > viewportHeight - margin) {
    top = Math.max(margin, triggerRect.top - bubbleRect.height - gap)
  }

  let left = triggerRect.left
  if (props.align === 'center') left = triggerRect.left + triggerRect.width / 2 - bubbleRect.width / 2
  if (props.align === 'end') left = triggerRect.right - bubbleRect.width
  left = Math.min(Math.max(margin, left), Math.max(margin, viewportWidth - bubbleRect.width - margin))

  position.value = { left, top }
}

onBeforeUnmount(hide)
</script>

<style scoped>
.tx-tooltip { --tooltip-x: 0; --tooltip-y: 2px; }
.tx-tooltip:focus-visible {
  outline: none;
}
.tooltip-bubble {
  position: fixed;
  transform: translate(var(--tooltip-x), var(--tooltip-y));
}
.floating-tooltip {
  opacity: 1;
  transform: none;
}
</style>
