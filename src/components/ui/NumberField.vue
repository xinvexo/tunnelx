<template>
  <input
    class="tx-input mono"
    type="number"
    :value="modelValue ?? ''"
    :min="min"
    :max="max"
    :placeholder="placeholder"
    @input="onInput"
  />
</template>

<script setup lang="ts">
const props = defineProps<{
  modelValue: number | null
  min?: number
  max?: number
  placeholder?: string
  zeroAsNull?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [number | null] }>()
function onInput(event: Event) {
  const value = (event.target as HTMLInputElement).value
  if (value === '') {
    emit('update:modelValue', null)
    return
  }
  const next = Number(value)
  emit('update:modelValue', !Number.isFinite(next) || (props.zeroAsNull && next === 0) ? null : next)
}
</script>
