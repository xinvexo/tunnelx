<template>
  <section class="page">
    <header v-if="title || $slots.actions" class="head">
      <div>
        <h2 v-if="title">{{ title }}</h2>
        <p v-if="description">{{ description }}</p>
      </div>
      <div class="actions"><slot name="actions" /></div>
    </header>
    <div class="scroll" :class="{ fill }">
      <div class="inner" :class="{ wide, fill }"><slot /></div>
    </div>
  </section>
</template>

<script setup lang="ts">
withDefaults(defineProps<{ title?: string; description?: string; wide?: boolean; fill?: boolean }>(), {
  wide: false,
  fill: false,
})
</script>

<style scoped>
.page { height: 100%; min-height: 0; display: flex; flex-direction: column; }
.head {
  min-height: 62px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--tx-border-subtle);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 11px 24px;
}
.head h2 { margin: 0; font-size: 15px; font-weight: 650; line-height: 1.3; }
.head p { margin: 2px 0 0; color: var(--tx-text-secondary); font-size: 12px; line-height: 1.3; }
.actions { display: flex; align-items: center; gap: 8px; }
.scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-y: overlay;
  scrollbar-gutter: auto;
}
.scroll.fill { overflow: hidden; }
.inner { width: 100%; max-width: 880px; padding: 20px 24px 24px; }
.inner.wide { max-width: none; }
.inner.fill { height: 100%; min-height: 0; display: flex; flex-direction: column; padding-bottom: 20px; }

@media (max-width: 760px) {
  .head { padding-left: 16px; padding-right: 16px; }
  .inner { padding-left: 16px; padding-right: 16px; }
}
</style>
