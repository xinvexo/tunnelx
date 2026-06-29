<template>
  <SwitchRoot
    class="switch"
    :model-value="modelValue"
    :disabled="disabled"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <SwitchThumb class="thumb" />
  </SwitchRoot>
</template>

<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from 'reka-ui'

defineProps<{ modelValue: boolean; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [boolean] }>()
</script>

<style scoped>
.switch {
  width: 30px;
  height: 18px;
  padding: 2px;
  border: 0;
  border-radius: 999px;
  background: #c7cdd6;
  cursor: pointer;
  transition: background 140ms ease, box-shadow 140ms ease;
}
.switch[data-state='checked'] { background: var(--tx-accent); }
.switch:hover:not(:disabled) { background: #b7c0cc; }
.switch[data-state='checked']:hover:not(:disabled) { background: var(--tx-accent-hover); }
.switch:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.switch:disabled { opacity: 0.45; cursor: not-allowed; }
.thumb {
  display: block;
  width: 14px;
  height: 14px;
  border-radius: 999px;
  background: #fff;
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.28);
  transition: transform 140ms ease;
}
.thumb[data-state='checked'] { transform: translateX(12px); }
</style>
