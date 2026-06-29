<template>
  <section class="group">
    <header v-if="title || note" class="head">
      <div>
        <h3 v-if="title">{{ title }}</h3>
        <p v-if="note">{{ note }}</p>
      </div>
      <slot name="action" />
    </header>
    <div class="body" :class="{ 'columns-2': columns === 2 }"><slot /></div>
  </section>
</template>

<script setup lang="ts">
withDefaults(defineProps<{ title?: string; note?: string; accent?: string; columns?: 1 | 2 }>(), {
  columns: 1,
})
</script>

<style scoped>
.group { margin-bottom: 20px; }
.head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 14px;
  margin-bottom: 8px;
  padding: 0 2px;
}
.head h3 { margin: 0; font-size: 13px; font-weight: 650; }
.head p { margin: 3px 0 0; color: var(--tx-text-secondary); font-size: 12px; }
.body {
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: var(--tx-bg-surface);
  overflow: hidden;
}
.body.columns-2 {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 1px;
  background: var(--tx-border-subtle);
}
.body.columns-2 :deep(.row) {
  grid-template-columns: minmax(0, 1fr) minmax(112px, 0.9fr);
  gap: 12px;
  border-top: 0;
  background: var(--tx-bg-surface);
}
.body.columns-2 :deep(.row.short) { grid-template-columns: minmax(0, 1fr) auto; }
.body.columns-2 :deep(.row.wide) { grid-column: 1 / -1; }

@media (max-width: 760px) {
  .body.columns-2 { grid-template-columns: 1fr; }
  .body.columns-2 :deep(.row.wide) { grid-column: auto; }
}
</style>
