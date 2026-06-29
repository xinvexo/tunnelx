<template>
  <div class="shell">
    <AppTitleBar />
    <div class="workspace">
      <ConnectionSidebar @create="createOpen = true" />
      <main class="content">
        <router-view />
      </main>
    </div>
    <div class="toasts" role="status" aria-live="polite" aria-atomic="false">
      <motion.div
        v-for="toast in ui.toasts"
        :key="toast.id"
        class="toast"
        :class="toast.tone"
        layout
        :initial="{ opacity: 0, x: 22, y: 10, scale: 0.96, filter: 'blur(4px)' }"
        :animate="{ opacity: 1, x: 0, y: 0, scale: 1, filter: 'blur(0px)' }"
        :exit="{ opacity: 0, x: 16, y: 6, scale: 0.97, filter: 'blur(4px)' }"
        :transition="toastTransition"
        :style="{ '--toast-duration': `${toast.durationMs}ms` }"
      >
        <span class="toast-icon"><Icon :icon="toastIcon(toast.tone)" /></span>
        <span class="toast-message">{{ toast.message }}</span>
        <button class="toast-close" type="button" :aria-label="t('ui.toastClose')" @click="ui.dismiss(toast.id)">
          <Icon icon="lucide:x" />
        </button>
      </motion.div>
    </div>
    <ConnectionCreateDialog v-model="createOpen" @created="openCreated" />
    <ConfirmDialog />
    <PromptDialog />
  </div>
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import { motion } from 'motion-v'
import { useRouter } from 'vue-router'
import AppTitleBar from '@/components/AppTitleBar.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import ConnectionCreateDialog from '@/components/ConnectionCreateDialog.vue'
import ConnectionSidebar from '@/components/ConnectionSidebar.vue'
import PromptDialog from '@/components/PromptDialog.vue'
import { connectionPath } from '@/providers/routes'
import { useUiStore } from '@/stores/ui'

const router = useRouter()
const { t } = useI18n()
const ui = useUiStore()
const createOpen = ref(false)
const toastTransition = {
  type: 'spring',
  stiffness: 520,
  damping: 38,
  mass: 0.7,
} as const

function toastIcon(tone: string) {
  if (tone === 'success') return 'lucide:check'
  if (tone === 'warning') return 'lucide:triangle-alert'
  if (tone === 'danger') return 'lucide:x'
  return 'lucide:info'
}

function isEditableTarget(target: EventTarget | null) {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true

  const tagName = target.tagName.toLowerCase()
  if (tagName === 'textarea' || tagName === 'select') return true
  if (tagName !== 'input') return false

  const type = (target as HTMLInputElement).type
  return !['button', 'checkbox', 'color', 'file', 'image', 'radio', 'range', 'reset', 'submit'].includes(type)
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === 'Enter' && !isEditableTarget(event.target)) {
    event.preventDefault()
    event.stopPropagation()
  }
}

function openCreated(resource: { providerId: string; id: string }) {
  router.push(connectionPath(resource.providerId, resource.id))
}
onMounted(() => {
  window.addEventListener('keydown', onKeydown, true)
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeydown, true)
})
</script>

<style scoped>
.shell {
  width: 100vw;
  height: 100vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--tx-bg-sidebar);
}
.workspace {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 6px;
  padding: 0 8px 8px 0;
  background: var(--tx-bg-sidebar);
}
.content {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  border: 1px solid rgba(15, 23, 42, 0.07);
  border-radius: 10px;
  background: var(--tx-bg-app);
  box-shadow:
    0 1px 2px rgba(15, 23, 42, 0.04),
    0 8px 24px rgba(15, 23, 42, 0.035);
}
.toasts {
  position: fixed;
  z-index: 120;
  right: 18px;
  bottom: 18px;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 10px;
  pointer-events: none;
}
.toast {
  width: min(360px, calc(100vw - 36px));
  min-height: 44px;
  border: 1px solid rgba(15, 23, 42, 0.1);
  border-radius: var(--tx-radius-md);
  background: rgba(255, 255, 255, 0.94);
  color: var(--tx-text-primary);
  box-shadow:
    0 16px 36px rgba(15, 23, 42, 0.14),
    0 1px 2px rgba(15, 23, 42, 0.08);
  display: grid;
  grid-template-columns: 22px minmax(0, 1fr) 24px;
  align-items: center;
  gap: 10px;
  padding: 9px 10px 9px 12px;
  pointer-events: auto;
  backdrop-filter: blur(16px);
  transform-origin: right bottom;
  will-change: opacity, transform, filter;
  position: relative;
  overflow: hidden;
}
.toast::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  background: var(--toast-progress-color, var(--tx-accent));
  transform-origin: left center;
  animation: toast-progress var(--toast-duration, 3200ms) linear forwards;
}
.toast-icon {
  width: 22px;
  height: 22px;
  border-radius: 999px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.toast-icon svg { width: 13px; height: 13px; }
.toast-message {
  min-width: 0;
  line-height: 1.45;
  overflow-wrap: anywhere;
}
.toast-close {
  width: 24px;
  height: 24px;
  border: 0;
  border-radius: var(--tx-radius-sm);
  background: transparent;
  color: var(--tx-text-muted);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.toast-close:hover,
.toast-close:focus-visible {
  background: var(--tx-bg-hover);
  color: var(--tx-text-primary);
}
.toast-close:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.toast-close svg { width: 13px; height: 13px; }
.toast.info .toast-icon { background: rgba(37, 99, 235, 0.1); color: var(--tx-accent); }
.toast.success .toast-icon { background: rgba(22, 163, 74, 0.11); color: var(--tx-success); }
.toast.warning .toast-icon { background: rgba(217, 119, 6, 0.12); color: var(--tx-warning); }
.toast.danger .toast-icon { background: rgba(220, 38, 38, 0.11); color: var(--tx-danger); }
.toast.info { border-left: 3px solid var(--tx-accent); }
.toast.success { border-left: 3px solid var(--tx-success); }
.toast.warning { border-left: 3px solid var(--tx-warning); }
.toast.danger { border-left: 3px solid var(--tx-danger); }
.toast.info { --toast-progress-color: var(--tx-accent); }
.toast.success { --toast-progress-color: var(--tx-success); }
.toast.warning { --toast-progress-color: var(--tx-warning); }
.toast.danger { --toast-progress-color: var(--tx-danger); }
@keyframes toast-progress {
  from { transform: scaleX(1); }
  to { transform: scaleX(0); }
}
</style>
