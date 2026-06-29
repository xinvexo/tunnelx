<template>
  <div class="titlebar" :class="{ mac: isMac }" data-tauri-drag-region>
    <div data-tauri-drag-region />
    <ImportantEventsPopover />
    <div v-if="!isMac" class="win-controls">
      <TxTooltip :text="t('titlebar.minimize')" placement="bottom">
        <button :aria-label="t('titlebar.minimize')" @click="minimize"><Icon icon="lucide:minus" /></button>
      </TxTooltip>
      <TxTooltip :text="t('titlebar.maximize')" placement="bottom">
        <button :aria-label="t('titlebar.maximize')" @click="toggleMax"><Icon icon="lucide:square" /></button>
      </TxTooltip>
      <TxTooltip :text="t('titlebar.close')" placement="bottom">
        <button class="close" :aria-label="t('titlebar.close')" @click="close"><Icon icon="lucide:x" /></button>
      </TxTooltip>
    </div>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isMac } from '@/composables/usePlatform'
import ImportantEventsPopover from '@/components/ImportantEventsPopover.vue'
import TxTooltip from '@/components/ui/TxTooltip.vue'

const { t } = useI18n()
const hasTauri = '__TAURI_INTERNALS__' in window
function minimize() { if (hasTauri) void getCurrentWindow().minimize() }
function toggleMax() { if (hasTauri) void getCurrentWindow().toggleMaximize() }
function close() { if (hasTauri) void getCurrentWindow().close() }
</script>

<style scoped>
.titlebar {
  height: 38px;
  flex-shrink: 0;
  display: grid;
  grid-template-columns: 220px 1fr 220px;
  align-items: center;
  background: var(--tx-bg-sidebar);
  user-select: none;
  overflow: visible;
}
.titlebar.mac { padding-left: 76px; grid-template-columns: 144px 1fr 220px; }
.win-controls {
  justify-self: end;
  height: 100%;
  display: flex;
  align-items: center;
  padding-right: 5px;
}
.win-controls button {
  width: 34px;
  height: 28px;
  border: 0;
  border-radius: var(--tx-radius-sm);
  background: transparent;
  color: var(--tx-text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.win-controls button:hover,
.win-controls button:focus-visible { background: var(--tx-bg-hover); color: var(--tx-text-primary); }
.win-controls button:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.win-controls .close:hover { background: var(--tx-danger); color: #fff; }
.win-controls svg { width: 13px; height: 13px; }
</style>
