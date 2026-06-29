<template>
  <button
    v-bind="buttonAttrs"
    class="tx-button"
    :class="[tone, size, { block, loading: isLoading }]"
    :type="type"
    :disabled="isDisabled"
    @click="handleClick"
  >
    <span class="content" :class="{ hidden: isLoading }">
      <img v-if="iconAsset" class="icon-image" :src="iconAsset" alt="" aria-hidden="true" />
      <Icon v-else-if="icon" :icon="icon" />
      <span v-if="$slots.default"><slot /></span>
    </span>
    <Icon v-if="isLoading" icon="lucide:loader-circle" class="spin" />
  </button>
</template>

<script setup lang="ts">
import { computed, useAttrs } from 'vue'
import { Icon } from '@iconify/vue'
import { useAsyncAction, type ActionKeyInput, type ActionMessage } from '@/composables/useAsyncAction'
import type { ToastTone } from '@/stores/ui'

defineOptions({ inheritAttrs: false })

const props = withDefaults(
  defineProps<{
    tone?: 'default' | 'primary' | 'success' | 'danger' | 'ghost'
    size?: 'sm' | 'md'
    type?: 'button' | 'submit'
    icon?: string
    iconAsset?: string
    disabled?: boolean
    loading?: boolean
    block?: boolean
    action?: (event: MouseEvent) => unknown | Promise<unknown>
    actionKey?: string
    locks?: ActionKeyInput
    success?: ActionMessage<unknown>
    successTone?: ToastTone
    error?: ActionMessage<unknown>
    errorTone?: ToastTone
  }>(),
  { tone: 'default', size: 'md', type: 'button' },
)

const attrs = useAttrs()
const actions = useAsyncAction()
const localActionKey = `button:${Math.random().toString(36).slice(2)}`

const runKey = computed(() => props.actionKey ?? localActionKey)
const isLoading = computed(() => props.loading || actions.isRunning(runKey.value))
const isDisabled = computed(() => props.disabled || isLoading.value || actions.isBusy(runKey.value) || actions.isBusy(props.locks))
const buttonAttrs = computed(() => {
  const { onClick: _onClick, ...rest } = attrs
  return rest
})

function clickListeners(): Array<(event: MouseEvent) => unknown> {
  const listener = attrs.onClick
  if (Array.isArray(listener)) return listener as Array<(event: MouseEvent) => unknown>
  return typeof listener === 'function' ? [listener as (event: MouseEvent) => unknown] : []
}

function runWithState(task: () => Promise<unknown>) {
  void actions.run(
    {
      key: runKey.value,
      locks: props.locks,
      success: props.success,
      successTone: props.successTone,
      error: props.error,
      errorTone: props.errorTone,
    },
    task,
  )
}

function handleClick(event: MouseEvent) {
  if (isDisabled.value) {
    event.preventDefault()
    event.stopPropagation()
    return
  }
  if (props.action) {
    runWithState(() => Promise.resolve(props.action!(event)))
    return
  }
  const listeners = clickListeners()
  if (!listeners.length) return
  runWithState(async () => {
    if (listeners.length === 1) return await listeners[0](event)
    return await Promise.all(listeners.map((listener) => listener(event)))
  })
}
</script>

<style scoped>
.tx-button {
  position: relative;
  height: 32px;
  border: 1px solid var(--tx-border-strong);
  border-radius: var(--tx-radius-sm);
  background: var(--tx-bg-surface);
  color: var(--tx-text-primary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0 11px;
  cursor: pointer;
  white-space: nowrap;
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease, opacity 120ms ease;
}
.content {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
}
.content.hidden { visibility: hidden; }
.tx-button:hover:not(:disabled) {
  background: var(--tx-bg-hover);
  border-color: var(--tx-border-hover);
}
.tx-button:focus-visible {
  outline: none;
  box-shadow: 0 0 0 3px var(--tx-focus-ring);
}
.tx-button.primary,
.tx-button.success,
.tx-button.danger {
  color: #fff;
  border-color: transparent;
}
.tx-button.primary { background: var(--tx-accent); }
.tx-button.primary:hover:not(:disabled) { background: var(--tx-accent-hover); }
.tx-button.success { background: var(--tx-success); }
.tx-button.success:hover:not(:disabled) { background: var(--tx-success-hover); }
.tx-button.danger { background: var(--tx-danger); }
.tx-button.danger:hover:not(:disabled) { background: var(--tx-danger-hover); }
.tx-button.ghost {
  border-color: transparent;
  background: transparent;
  color: var(--tx-text-secondary);
}
.tx-button.ghost:hover:not(:disabled) { background: var(--tx-bg-hover); color: var(--tx-text-primary); }
.tx-button.sm {
  height: 28px;
  padding: 0 8px;
  font-size: 12px;
}
.tx-button.block { width: 100%; }
.tx-button:disabled { cursor: not-allowed; opacity: 0.48; }
.tx-button svg,
.icon-image {
  width: 15px;
  height: 15px;
}
.icon-image {
  object-fit: contain;
  border-radius: 3px;
}
.spin {
  position: absolute;
  inset: 0;
  margin: auto;
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
