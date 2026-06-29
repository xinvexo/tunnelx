<template>
  <DialogRoot :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <DialogPortal>
      <DialogOverlay class="overlay" />
      <DialogContent class="content" :class="{ wide }">
        <header class="head">
          <div>
            <DialogTitle class="title">{{ title }}</DialogTitle>
            <DialogDescription v-if="description" class="description">{{ description }}</DialogDescription>
          </div>
          <TxIconButton icon="lucide:x" :label="t('ui.dialogClose')" @click="emit('update:modelValue', false)" />
        </header>
        <div class="body"><slot /></div>
        <footer v-if="$slots.footer" class="footer"><slot name="footer" /></footer>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<script setup lang="ts">
import {
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui'
import { useI18n } from 'vue-i18n'
import TxIconButton from './TxIconButton.vue'

const { t } = useI18n()
defineProps<{ modelValue: boolean; title: string; description?: string; wide?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [boolean] }>()
</script>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 80;
  background: rgba(15, 23, 42, 0.28);
  backdrop-filter: blur(2px);
}
.content {
  position: fixed;
  z-index: 81;
  top: 50%;
  left: 50%;
  width: min(440px, calc(100vw - 32px));
  max-height: calc(100vh - 56px);
  transform: translate(-50%, -50%);
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-lg);
  background: var(--tx-bg-elevated);
  box-shadow: var(--tx-shadow-float);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.content.wide { width: min(760px, calc(100vw - 48px)); }
.head,
.footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 14px 16px;
}
.head { border-bottom: 1px solid var(--tx-border-subtle); }
.footer {
  justify-content: flex-end;
  border-top: 1px solid var(--tx-border-subtle);
}
.title { margin: 0; font-size: 15px; font-weight: 650; }
.description { margin: 4px 0 0; color: var(--tx-text-secondary); font-size: 12px; }
.body { padding: 16px; overflow: auto; }
</style>
