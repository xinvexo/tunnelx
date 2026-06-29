<template>
  <button class="advanced-toggle" :class="{ open }" type="button" @click="emit('click')">
    <span>{{ label || t('ui.advancedOptions') }}</span>
    <Icon icon="lucide:chevron-down" />
  </button>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

withDefaults(defineProps<{ open?: boolean; label?: string }>(), {
  open: false,
  label: '',
})

const emit = defineEmits<{ click: [] }>()
</script>

<style scoped>
.advanced-toggle {
  width: 100%;
  height: 42px;
  border: 1px solid rgba(37, 99, 235, 0.18);
  border-left: 3px solid var(--tx-accent);
  border-radius: var(--tx-radius-sm);
  background: rgba(37, 99, 235, 0.045);
  color: var(--tx-text-primary);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 12px 0 13px;
  text-align: left;
  cursor: pointer;
  transition: background 120ms ease, box-shadow 120ms ease;
}
.advanced-toggle:hover,
.advanced-toggle:focus-visible { background: var(--tx-accent-soft-hover); }
.advanced-toggle:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.advanced-toggle span { font-weight: 560; }
.advanced-toggle svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  color: var(--tx-accent);
  transition: transform 140ms ease;
}
.advanced-toggle.open svg { transform: rotate(180deg); }
</style>
