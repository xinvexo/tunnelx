<template>
  <WorkspacePage wide fill>
    <div class="log-shell">
      <div class="toolbar">
        <div class="search">
          <Icon icon="lucide:search" />
          <input v-model="query" :placeholder="t('logs.searchPlaceholder')" />
        </div>
        <label class="check"><input :checked="wrap" type="checkbox" @change="setWrap" /> {{ t('logs.wrap') }}</label>
        <label class="check"><input :checked="follow" type="checkbox" @change="setFollow" /> {{ t('logs.follow') }}</label>
        <span class="count">{{ t('logs.count', { count: filtered.length }) }}</span>
        <button class="toolbar-action" type="button" @click="copyAll">
          <Icon icon="lucide:copy" /> {{ t('logs.copy') }}
        </button>
        <button class="toolbar-action" type="button" @click="refresh">
          <Icon icon="lucide:refresh-cw" /> {{ t('common.refresh') }}
        </button>
      </div>

      <div ref="boxRef" class="logs" :class="{ wrap }" @scroll="onScroll">
        <div v-if="!filtered.length" class="empty-log">
          {{ rawLines.length ? t('logs.emptyNoMatch') : t('logs.emptyNone') }}
        </div>
        <div v-else class="virtual-log-list">
          <div class="virtual-pad" :style="{ height: `${virtualOffsetY}px` }" />
          <LogLine
            v-for="line in visibleLines"
            :key="line.key"
            :text="line.text"
            :level="line.level"
            :number="line.number"
            :wrap="wrap"
          />
          <div class="virtual-pad" :style="{ height: `${virtualBottomPadding}px` }" />
        </div>
      </div>

      <button v-if="!follow" class="jump" type="button" @click="resumeFollow">
        <Icon icon="lucide:arrow-down" /> {{ t('logs.backToBottom') }}
      </button>
    </div>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import { listen } from '@tauri-apps/api/event'
import { useRoute } from 'vue-router'
import LogLine from '@/components/ui/LogLine.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { useClipboard } from '@/composables/useClipboard'
import { providerTunnelLogs } from '@/providers/api'
import type { TunnelRuntimeLogEvent } from '@/providers/contract'
import { useUiStore } from '@/stores/ui'
import { detectLogLevel, type LogLevel } from '@/utils/logs'

interface DisplayLine {
  key: string
  text: string
  level: LogLevel
  number: number
  searchText: string
}

const route = useRoute()
const { t } = useI18n()
const ui = useUiStore()
const { copyText } = useClipboard()
const LOG_ROW_HEIGHT = 20.6
const LOG_OVERSCAN = 18
const providerId = computed(() => String(route.params.providerId ?? ''))
const id = computed(() => String(route.params.id ?? ''))
const lines = ref<string[]>([])
const query = ref('')
const wrap = computed(() => ui.logWrap)
const follow = computed(() => ui.logFollow)
const boxRef = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const viewportHeight = ref(0)
const unlisteners: Array<() => void> = []
let unmounted = false

const rawLines = computed<DisplayLine[]>(() =>
  lines.value.map((text, index) => ({
    key: `${index}-${text}`,
    text,
    level: detectLogLevel(text),
    number: index + 1,
    searchText: text.toLowerCase(),
  })),
)
const filtered = computed(() => {
  const needle = query.value.trim().toLowerCase()
  return rawLines.value.filter((line) => !needle || line.searchText.includes(needle))
})
const virtualVisibleCount = computed(() => Math.ceil(viewportHeight.value / LOG_ROW_HEIGHT) + LOG_OVERSCAN * 2)
const virtualStart = computed(() => {
  const rawStart = Math.max(0, Math.floor(scrollTop.value / LOG_ROW_HEIGHT) - LOG_OVERSCAN)
  const maxStart = Math.max(0, filtered.value.length - virtualVisibleCount.value)
  return Math.min(rawStart, maxStart)
})
const virtualEnd = computed(() => Math.min(filtered.value.length, virtualStart.value + virtualVisibleCount.value))
const visibleLines = computed(() => filtered.value.slice(virtualStart.value, virtualEnd.value))
const virtualOffsetY = computed(() => virtualStart.value * LOG_ROW_HEIGHT)
const virtualBottomPadding = computed(() =>
  Math.max(0, (filtered.value.length - virtualEnd.value) * LOG_ROW_HEIGHT),
)

watch([providerId, id], refresh, { immediate: true })
watch(() => `${filtered.value.length}|${filtered.value[filtered.value.length - 1]?.key ?? ''}`, () => {
  if (!follow.value) return
  nextTick(scrollBottom)
})

onMounted(async () => {
  nextTick(syncViewportAfterRender)
  try {
    const unlisten = await listen<TunnelRuntimeLogEvent>('provider-tunnel-log', (event) => {
      const payload = event.payload
      if (payload.providerId !== providerId.value || payload.tunnelId !== id.value) return
      if (payload.reset) {
        lines.value = []
        return
      }
      lines.value = [...lines.value, payload.line].slice(-1000)
    })
    // The component may have unmounted while listen() was pending; don't leak the handler.
    if (unmounted) unlisten()
    else unlisteners.push(unlisten)
  } catch {
    // Browser previews do not have the Tauri event bridge.
  }
})
onBeforeUnmount(() => {
  unmounted = true
  while (unlisteners.length) unlisteners.pop()?.()
})

async function refresh() {
  if (!providerId.value || !id.value) return
  lines.value = await providerTunnelLogs(providerId.value, id.value).catch(() => [])
  nextTick(syncViewportAfterRender)
}

function setWrap(event: Event) {
  ui.setLogWrap((event.target as HTMLInputElement).checked)
}

function setFollow(event: Event) {
  ui.setLogFollow((event.target as HTMLInputElement).checked)
  if (ui.logFollow) nextTick(scrollBottom)
}

function resumeFollow() {
  ui.setLogFollow(true)
  nextTick(scrollBottom)
}

function onScroll() {
  const element = boxRef.value
  if (!element) return
  syncViewport()
  if (element.scrollHeight - element.scrollTop - element.clientHeight > 28) ui.setLogFollow(false)
}

function scrollBottom() {
  const element = boxRef.value
  if (!element) return
  element.scrollTop = element.scrollHeight
  syncViewport()
}

function syncViewport() {
  const element = boxRef.value
  if (!element) return
  scrollTop.value = element.scrollTop
  viewportHeight.value = element.clientHeight
}

function syncViewportAfterRender() {
  syncViewport()
  if (follow.value) scrollBottom()
}

async function copyAll() {
  const text = filtered.value.map((line) => line.text).join('\n')
  if (!text) {
    ui.notify(t('logs.nothingToCopy'), 'info')
    return
  }
  await copyText(text, t('logs.copied'))
}
</script>

<style scoped>
.log-shell {
  position: relative;
  flex: 1;
  min-height: 0;
  border: 1px solid #252b35;
  border-radius: var(--tx-radius-md);
  background: #101318;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.toolbar {
  min-height: 42px;
  flex-shrink: 0;
  border-bottom: 1px solid #252b35;
  background: #171b22;
  color: #aab3c0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
  padding: 6px 10px;
  font-size: 11px;
}
.search {
  width: 220px;
  height: 26px;
  border: 1px solid #303744;
  border-radius: 5px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 7px;
}
.search svg {
  width: 13px;
  height: 13px;
}
.search input {
  flex: 1;
  min-width: 0;
  border: 0;
  outline: 0;
  background: transparent;
  color: #d7dde7;
}
.check {
  height: 26px;
  border-radius: 5px;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 0 5px;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.check:hover {
  background: var(--tx-dark-bg-hover);
  color: var(--tx-dark-text-hover);
}
.check:focus-within {
  box-shadow: inset 0 0 0 1px var(--tx-dark-border-hover);
}
.check input { accent-color: var(--tx-accent); }
.check input:focus { outline: none; }
.count {
  margin-left: auto;
  color: #7d8795;
}
.toolbar-action {
  border: 0;
  background: transparent;
  color: #c6ceda;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  cursor: pointer;
}
.toolbar-action svg {
  width: 13px;
  height: 13px;
}
.logs {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 8px 0;
  position: relative;
  scrollbar-color: #6f7f94 #151a22;
  scrollbar-width: thin;
}
.virtual-log-list {
  min-width: max-content;
}
.logs.wrap .virtual-log-list {
  min-width: 0;
  width: 100%;
}
.virtual-pad {
  flex-shrink: 0;
}
.logs.wrap :deep(.message) {
  white-space: pre-wrap;
}
.logs::-webkit-scrollbar {
  width: 12px;
  height: 12px;
}
.logs::-webkit-scrollbar-track {
  background: #151a22;
}
.logs::-webkit-scrollbar-thumb {
  border: 3px solid #151a22;
  border-radius: 999px;
  background: #6f7f94;
  background-clip: content-box;
}
.logs::-webkit-scrollbar-thumb:hover {
  background: #9aa8ba;
  background-clip: content-box;
}
.logs::-webkit-scrollbar-corner {
  background: #151a22;
}
.empty-log {
  padding: 18px;
  color: #7d8795;
}
.jump {
  position: absolute;
  right: 14px;
  bottom: 14px;
  height: 28px;
  border: 1px solid #343c49;
  border-radius: 999px;
  background: #202631;
  color: #cbd3df;
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 0 10px;
  cursor: pointer;
  transition: border-color 120ms ease, background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.jump:hover,
.jump:focus-visible {
  border-color: var(--tx-dark-border-hover);
  background: #273142;
  color: #fff;
}
.jump:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-dark-border-hover);
}
.jump svg {
  width: 13px;
  height: 13px;
}
</style>
