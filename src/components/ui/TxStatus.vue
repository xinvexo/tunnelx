<template>
  <span class="status" :class="status">
    <span class="dot" />
    <span v-if="showLabel">{{ label }}</span>
  </span>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { TunnelRuntimeState } from '@/providers/contract'

const props = withDefaults(defineProps<{ status: TunnelRuntimeState; showLabel?: boolean }>(), {
  showLabel: true,
})

const { t } = useI18n()
const label = computed(() => t(`status.${props.status}`))
</script>

<style scoped>
.status {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--tx-text-secondary);
  white-space: nowrap;
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: #98a2b3;
  transition: background 160ms ease, box-shadow 160ms ease;
}
.running .dot {
  background: var(--tx-success);
  box-shadow: 0 0 0 3px rgba(22, 163, 74, 0.12);
}
.starting .dot,
.stopping .dot {
  background: var(--tx-warning);
  animation: pulse 1s ease-in-out infinite;
}
.warning { color: var(--tx-warning); }
.warning .dot {
  background: var(--tx-warning);
  box-shadow: 0 0 0 3px rgba(217, 119, 6, 0.13);
}
.errored { color: var(--tx-danger); }
.errored .dot { background: var(--tx-danger); }
@keyframes pulse { 50% { opacity: 0.4; } }
</style>
