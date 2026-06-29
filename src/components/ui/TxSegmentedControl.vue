<template>
  <ToggleGroupRoot
    class="inline-flex border border-tx-border rounded-[var(--tx-radius-sm)] bg-[#f2f4f7] p-2px"
    type="single"
    :model-value="modelValue"
    :disabled="disabled"
    @update:model-value="onUpdate"
  >
    <ToggleGroupItem
      v-for="option in options"
      :key="option.value"
      :value="option.value"
      :disabled="option.disabled"
      class="h-24px min-w-62px cursor-pointer border-0 rounded-4px bg-transparent px-10px text-12px text-[var(--tx-text-secondary)] transition-colors transition-shadow duration-120 hover:bg-[var(--tx-bg-hover)] hover:text-[var(--tx-text-primary)] focus-visible:outline-none focus-visible:shadow-[inset_0_0_0_1px_var(--tx-border-hover)] data-[state=on]:bg-tx-surface data-[state=on]:text-[var(--tx-text-primary)] data-[state=on]:shadow-[0_1px_2px_rgba(15,23,42,0.12)] disabled:cursor-not-allowed disabled:opacity-45"
    >
      {{ option.label }}
    </ToggleGroupItem>
  </ToggleGroupRoot>
</template>

<script setup lang="ts">
import { ToggleGroupItem, ToggleGroupRoot } from 'reka-ui'

export interface TxSegmentedOption {
  label: string
  value: string
  disabled?: boolean
}

defineProps<{
  modelValue: string
  options: TxSegmentedOption[]
  disabled?: boolean
}>()

const emit = defineEmits<{ 'update:modelValue': [string] }>()

function onUpdate(value: unknown) {
  const next = String(value ?? '')
  if (next) emit('update:modelValue', next)
}
</script>
