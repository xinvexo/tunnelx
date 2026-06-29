<template>
  <TxTooltip class="icon-tooltip" :text="label">
    <button
      v-bind="buttonAttrs"
      class="tx-icon-btn"
      :class="{ 'tx-icon-btn-danger': danger, 'tx-icon-btn-primary': primary, loading: isLoading }"
      :aria-label="label"
      :disabled="isDisabled"
      type="button"
      @click="handleClick"
    >
      <Icon :icon="isLoading ? 'lucide:loader-circle' : icon" :class="{ spin: isLoading }" />
    </button>
  </TxTooltip>
</template>

<script setup lang="ts">
import { computed, useAttrs } from 'vue'
import { Icon } from '@iconify/vue'
import { useAsyncAction, type ActionKeyInput, type ActionMessage } from '@/composables/useAsyncAction'
import type { ToastTone } from '@/stores/ui'
import TxTooltip from './TxTooltip.vue'

defineOptions({ inheritAttrs: false })

const props = defineProps<{
  icon: string
  label: string
  danger?: boolean
  primary?: boolean
  disabled?: boolean
  loading?: boolean
  action?: (event: MouseEvent) => unknown | Promise<unknown>
  actionKey?: string
  locks?: ActionKeyInput
  success?: ActionMessage<unknown>
  successTone?: ToastTone
  error?: ActionMessage<unknown>
  errorTone?: ToastTone
}>()
const attrs = useAttrs()
const actions = useAsyncAction()
const localActionKey = `icon:${Math.random().toString(36).slice(2)}`

const runKey = computed(() => props.actionKey ?? localActionKey)
const isLoading = computed(() => props.loading || actions.isRunning(runKey.value))
const isDisabled = computed(() => props.disabled || isLoading.value || actions.isBusy(runKey.value) || actions.isBusy(props.locks))
const buttonAttrs = computed(() => {
  const { onClick: _onClick, title: _title, ...rest } = attrs
  return rest
})

function clickListeners(): Array<(event: MouseEvent) => unknown> {
  const listener = attrs.onClick
  if (Array.isArray(listener)) return listener as Array<(event: MouseEvent) => unknown>
  return typeof listener === 'function' ? [listener as (event: MouseEvent) => unknown] : []
}

function handleClick(event: MouseEvent) {
  if (isDisabled.value) {
    event.preventDefault()
    event.stopPropagation()
    return
  }
  if (props.action) {
    void actions.run(
      {
        key: runKey.value,
        locks: props.locks,
        success: props.success,
        successTone: props.successTone,
        error: props.error,
        errorTone: props.errorTone,
      },
      () => Promise.resolve(props.action!(event)),
    )
    return
  }
  const listeners = clickListeners()
  if (!listeners.length) return
  const runListeners = async () => {
    if (listeners.length === 1) return await listeners[0](event)
    return await Promise.all(listeners.map((listener) => listener(event)))
  }
  void actions.run(
    {
      key: runKey.value,
      locks: props.locks,
      success: props.success,
      successTone: props.successTone,
      error: props.error,
      errorTone: props.errorTone,
    },
    runListeners,
  )
}
</script>

<style scoped>
.icon-tooltip {
  flex: 0 0 28px;
}
.tx-icon-btn { -webkit-tap-highlight-color: transparent; }
.tx-icon-btn svg { width: 16px; height: 16px; }
.spin { animation: spin 0.8s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
