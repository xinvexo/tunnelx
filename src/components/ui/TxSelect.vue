<template>
  <SelectRoot
    :model-value="rootValue"
    :disabled="disabled"
    @update:model-value="onUpdate"
  >
    <SelectTrigger class="trigger" :aria-label="ariaLabel || t('ui.selectAria')">
      <SelectValue :placeholder="placeholder || t('ui.selectPlaceholder')" />
      <Icon icon="lucide:chevron-down" class="chevron" />
    </SelectTrigger>
    <SelectPortal>
      <SelectContent class="tx-select-content" :class="{ dark: tone === 'dark' }" position="popper" :side-offset="4">
        <SelectViewport class="tx-select-viewport">
          <SelectItem v-for="option in options" :key="option.value" class="tx-select-item" :value="itemValue(option.value)">
            <SelectItemText>{{ option.label }}</SelectItemText>
            <SelectItemIndicator class="tx-select-indicator"><Icon icon="lucide:check" /></SelectItemIndicator>
          </SelectItem>
        </SelectViewport>
      </SelectContent>
    </SelectPortal>
  </SelectRoot>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import {
  SelectContent,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectTrigger,
  SelectValue,
  SelectViewport,
} from 'reka-ui'

export interface TxSelectOption {
  label: string
  value: string
}

const props = withDefaults(defineProps<{
  modelValue: string
  options: TxSelectOption[]
  placeholder?: string
  ariaLabel?: string
  disabled?: boolean
  tone?: 'light' | 'dark'
}>(), {
  placeholder: '',
  ariaLabel: '',
  tone: 'light',
})

const { t } = useI18n()
const emit = defineEmits<{ 'update:modelValue': [string] }>()
const EMPTY_VALUE = '__tunnelx_empty__'
const rootValue = computed(() => itemValue(props.modelValue))
function itemValue(value: string) { return value === '' ? EMPTY_VALUE : value }
function onUpdate(value: unknown) {
  const next = String(value ?? '')
  emit('update:modelValue', next === EMPTY_VALUE ? '' : next)
}
</script>

<style scoped>
.trigger {
  width: 100%;
  min-width: 0;
  height: 32px;
  border: 1px solid var(--tx-border-strong);
  border-radius: var(--tx-radius-sm);
  background: var(--tx-bg-surface);
  color: var(--tx-text-primary);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 0 9px;
  outline: none;
  cursor: pointer;
}
.trigger:hover:not(:disabled) {
  border-color: var(--tx-border-hover);
  background: var(--tx-bg-hover);
}
.trigger:focus-visible,
.trigger[data-state='open'] {
  border-color: var(--tx-accent);
  box-shadow: 0 0 0 3px var(--tx-focus-ring);
}
.trigger:disabled { cursor: not-allowed; opacity: 0.5; }
.chevron { width: 14px; height: 14px; color: var(--tx-text-muted); flex-shrink: 0; }
</style>

<style>
.tx-select-content {
  z-index: 150;
  min-width: var(--reka-select-trigger-width);
  max-height: min(320px, var(--reka-select-content-available-height));
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: var(--tx-bg-elevated);
  box-shadow: var(--tx-shadow-float);
  overflow: hidden;
}
.tx-select-content.dark {
  border-color: #303744;
  background: #171b22;
}
.tx-select-content.dark .tx-select-item { color: #cbd3df; }
.tx-select-content.dark .tx-select-item[data-highlighted] { background: var(--tx-dark-bg-hover); color: var(--tx-dark-text-hover); }
.tx-select-viewport { padding: 4px; }
.tx-select-item {
  position: relative;
  min-height: 30px;
  border-radius: var(--tx-radius-sm);
  color: var(--tx-text-primary);
  display: flex;
  align-items: center;
  padding: 0 30px 0 8px;
  outline: none;
  cursor: pointer;
  user-select: none;
}
.tx-select-item[data-highlighted] { background: var(--tx-bg-hover); color: var(--tx-text-primary); }
.tx-select-item[data-disabled] { opacity: 0.45; cursor: not-allowed; }
.tx-select-indicator {
  position: absolute;
  right: 8px;
  display: inline-flex;
  color: var(--tx-accent);
}
.tx-select-indicator svg { width: 14px; height: 14px; }
</style>
