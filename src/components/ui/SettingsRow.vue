<template>
  <div
    class="row"
    :class="{ vertical, clickable, wide, short, compact, 'copy-only': !$slots.default }"
    :role="clickable ? 'button' : undefined"
    :tabindex="clickable ? 0 : undefined"
    @click="activate"
    @keydown.enter.prevent="activate"
    @keydown.space.prevent="activate"
  >
    <div class="copy">
      <div v-if="label || $slots.label" class="label"><slot name="label">{{ label }}</slot></div>
      <div v-if="desc || $slots.desc" class="desc"><slot name="desc">{{ desc }}</slot></div>
    </div>
    <div v-if="$slots.default" class="control"><slot /></div>
  </div>
</template>

<script setup lang="ts">
const props = defineProps<{
  label?: string
  desc?: string
  vertical?: boolean
  clickable?: boolean
  wide?: boolean
  short?: boolean
  compact?: boolean
}>()
const emit = defineEmits<{ click: [] }>()

function activate() {
  if (props.clickable) emit('click')
}
</script>

<style scoped>
.row {
  min-height: 48px;
  display: grid;
  grid-template-columns: minmax(160px, 0.8fr) minmax(220px, 1.2fr);
  align-items: center;
  gap: 20px;
  padding: 9px 14px;
}
.row + .row { border-top: 1px solid var(--tx-border-subtle); }
.row.clickable {
  cursor: pointer;
  transition: background 120ms ease, box-shadow 120ms ease;
}
.row.clickable:hover,
.row.clickable:focus-visible { background: var(--tx-bg-hover); }
.row.clickable:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.copy { min-width: 0; }
.label { color: var(--tx-text-primary); }
.desc { margin-top: 3px; color: var(--tx-text-secondary); font-size: 11px; line-height: 1.4; }
.control { min-width: 0; display: flex; align-items: center; justify-content: flex-end; gap: 8px; }
.vertical { grid-template-columns: 1fr; gap: 8px; align-items: stretch; }
.vertical .control { justify-content: stretch; }
.copy-only { grid-template-columns: minmax(0, 1fr); }
.copy-only .copy { max-width: none; }
.compact { grid-template-columns: minmax(0, 1fr) auto; }
.compact .control { min-width: 140px; }
</style>
