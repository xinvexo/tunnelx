<template>
  <DialogRoot :open="!!ui.confirmRequest" @update:open="onOpenChange">
    <DialogPortal>
      <DialogOverlay class="confirm-overlay" />
      <DialogContent class="confirm-card" :class="toneClass">
        <div class="confirm-main">
          <span class="confirm-icon">
            <Icon :icon="isDanger ? 'lucide:trash-2' : 'lucide:circle-alert'" />
          </span>
          <div class="confirm-copy">
            <DialogTitle class="confirm-title">{{ ui.confirmRequest?.title }}</DialogTitle>
            <DialogDescription v-if="ui.confirmRequest?.message" class="confirm-message">
              {{ ui.confirmRequest.message }}
            </DialogDescription>
          </div>
          <TxIconButton
            class="confirm-close"
            icon="lucide:x"
            :label="t('ui.dialogClose')"
            @click="ui.resolveConfirm(false)"
          />
        </div>
        <footer class="confirm-actions">
          <TxButton size="sm" @click="ui.resolveConfirm(false)">
            {{ ui.confirmRequest?.cancelLabel ?? t('common.cancel') }}
          </TxButton>
          <TxButton
            size="sm"
            :tone="isDanger ? 'danger' : 'primary'"
            @click="ui.resolveConfirm(true)"
          >
            {{ ui.confirmRequest?.confirmLabel ?? t('common.confirm') }}
          </TxButton>
        </footer>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import {
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import TxButton from '@/components/ui/TxButton.vue'
import TxIconButton from '@/components/ui/TxIconButton.vue'
import { useUiStore } from '@/stores/ui'

const { t } = useI18n()
const ui = useUiStore()
const isDanger = computed(() => ui.confirmRequest?.tone === 'danger')
const toneClass = computed(() => ({ danger: isDanger.value }))

function onOpenChange(open: boolean) {
  if (!open) ui.resolveConfirm(false)
}
</script>

<style scoped>
.confirm-overlay {
  position: fixed;
  inset: 0;
  z-index: 80;
  background: rgba(15, 23, 42, 0.24);
  backdrop-filter: blur(2px);
}
.confirm-card {
  position: fixed;
  z-index: 81;
  top: 50%;
  left: 50%;
  width: min(372px, calc(100vw - 40px));
  transform: translate(-50%, -50%);
  border: 1px solid rgba(15, 23, 42, 0.1);
  border-radius: var(--tx-radius-lg);
  background: rgba(255, 255, 255, 0.98);
  box-shadow:
    0 20px 52px rgba(15, 23, 42, 0.18),
    0 1px 2px rgba(15, 23, 42, 0.08);
  padding: 18px;
}
.confirm-main {
  display: grid;
  grid-template-columns: 34px minmax(0, 1fr) 28px;
  align-items: flex-start;
  gap: 12px;
}
.confirm-icon {
  width: 34px;
  height: 34px;
  border-radius: var(--tx-radius-md);
  background: var(--tx-accent-soft);
  color: var(--tx-accent);
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.confirm-icon svg { width: 17px; height: 17px; }
.confirm-card.danger .confirm-icon {
  background: var(--tx-danger-soft);
  color: var(--tx-danger);
}
.confirm-copy {
  min-width: 0;
  padding-top: 1px;
}
.confirm-title {
  margin: 0;
  color: var(--tx-text-primary);
  font-size: 15px;
  font-weight: 680;
  line-height: 1.35;
}
.confirm-message {
  margin: 8px 0 0;
  color: var(--tx-text-secondary);
  font-size: 13px;
  line-height: 1.55;
  overflow-wrap: anywhere;
}
.confirm-close {
  margin-top: -3px;
  margin-right: -5px;
}
.confirm-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 20px;
}
.confirm-actions :deep(.tx-button) {
  min-width: 68px;
}
</style>
