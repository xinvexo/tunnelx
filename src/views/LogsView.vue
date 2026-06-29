<template>
  <WorkspacePage :title="t('logs.title')" :description="t('logs.description')" wide fill>
    <div class="log-shell">
      <div class="toolbar">
        <div class="search">
          <Icon icon="lucide:search" />
          <input v-model="query" :placeholder="t('logs.searchPlaceholder')" />
        </div>
        <div class="level-select">
          <TxSelect v-model="level" :aria-label="t('logs.levelAria')" tone="dark" :options="levelOptions" />
        </div>
        <label class="check"><input :checked="wrap" type="checkbox" @change="setWrap" /> {{ t('logs.wrap') }}</label>
        <label class="check"><input :checked="follow" type="checkbox" @change="setFollow" /> {{ t('logs.follow') }}</label>
        <span class="count">{{ t('logs.count', { count: filtered.length }) }}</span>
        <button class="toolbar-action" type="button" @click="copyAll">
          <Icon icon="lucide:copy" /> {{ t('logs.copy') }}
        </button>
        <button class="toolbar-action" type="button" @click="clear">
          <Icon icon="lucide:trash-2" /> {{ t('logs.clear') }}
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
            :source="line.source"
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
import LogLine from '@/components/ui/LogLine.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { useClipboard } from '@/composables/useClipboard'
import { providerTunnelLogs, watchdogLogs } from '@/providers/api'
import { connectionKey, useConnectionStore } from '@/providers/connections'
import type { TunnelRuntimeLogEvent, WatchdogLogEvent } from '@/providers/contract'
import { useUiStore } from '@/stores/ui'
import { detectLogLevel, type LogLevel } from '@/utils/logs'

interface DisplayLine {
  key: string
  groupKey: string
  source: string
  text: string
  level: LogLevel
  number: number
  sortTime: number | null
  sortIndex: number
  searchText: string
}

interface LogGroup {
  key: string
  source: string
  lines: string[]
  groupIndex: number
}

const LOG_LEVEL_CACHE_LIMIT = 5000
const LOG_ROW_HEIGHT = 20.6
const LOG_OVERSCAN = 18
const WATCHDOG_LOG_KEY = 'platform:watchdog'
const logLevelCache = new Map<string, LogLevel>()
const { t } = useI18n()
const connections = useConnectionStore()
const ui = useUiStore()
const { copyText } = useClipboard()
const boxRef = ref<HTMLElement | null>(null)
const query = ref('')
const level = ref('')
const rawLines = ref<DisplayLine[]>([])
const wrap = computed(() => ui.logWrap)
const follow = computed(() => ui.logFollow)
const scrollTop = ref(0)
const viewportHeight = ref(0)
const unlisteners: Array<() => void> = []
let unmounted = false
const resourcesByKey = computed(() =>
  new Map(connections.resources.map((resource) => [connectionKey(resource.providerId, resource.id), resource])),
)
const levelOptions = computed(() => [
  { label: t('logs.level.all'), value: '' },
  { label: t('logs.level.error'), value: 'error' },
  { label: t('logs.level.warn'), value: 'warn' },
  { label: t('logs.level.info'), value: 'info' },
  { label: t('logs.level.debug'), value: 'debug' },
])
const filtered = computed(() => {
  const needle = query.value.trim().toLowerCase()
  return rawLines.value.filter((line) => {
    if (level.value && line.level !== level.value) return false
    return !needle || line.searchText.includes(needle)
  })
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

onMounted(async () => {
  await connections.init().catch(() => undefined)
  await refreshLogs()
  nextTick(syncViewportAfterRender)
  try {
    const unlistenLog = await listen<TunnelRuntimeLogEvent>('provider-tunnel-log', (event) => {
      const key = connectionKey(event.payload.providerId, event.payload.tunnelId)
      if (event.payload.reset) {
        clearConnectionLogs(key)
        return
      }
      appendLine(key, event.payload.line)
    })
    // The component may have unmounted while listen() was pending; don't leak the handler.
    if (unmounted) unlistenLog()
    else unlisteners.push(unlistenLog)
    const unlistenWatchdog = await listen<WatchdogLogEvent>('watchdog-log', (event) => {
      appendLine(WATCHDOG_LOG_KEY, event.payload.line)
      if (event.payload.important) {
        ui.recordImportantEvent({
          titleKey: watchdogTitleKey(event.payload.code),
          message: event.payload.line,
          tone: event.payload.level === 'danger' ? 'danger' : 'warning',
          source: 'watchdog',
        })
      }
    })
    if (unmounted) unlistenWatchdog()
    else unlisteners.push(unlistenWatchdog)
  } catch {
    // Browser previews do not have the Tauri event bridge.
  }
})
onBeforeUnmount(() => {
  unmounted = true
  while (unlisteners.length) unlisteners.pop()?.()
})

watch(() => connections.resources.map((item) => `${item.providerId}:${item.id}`).sort().join('\0'), refreshLogs)
watch(
  () => {
    const lines = filtered.value
    return `${lines.length}|${lines[lines.length - 1]?.key ?? ''}`
  },
  () => {
    if (!follow.value) return
    nextTick(scrollBottom)
  },
)

async function refreshLogs() {
  const entries = await Promise.all(connections.resources.map(async (resource) => {
    const lines = await providerTunnelLogs(resource.providerId, resource.id).catch(() => [] as string[])
    return {
      key: connectionKey(resource.providerId, resource.id),
      source: resource.name,
      lines,
    }
  }))
  const watchdog = await watchdogLogs().catch(() => [] as string[])
  rawLines.value = buildTimeline([
    { key: WATCHDOG_LOG_KEY, source: t('logs.source.watchdog'), lines: watchdog },
    ...entries,
  ])
  nextTick(syncViewportAfterRender)
}

function appendLine(key: string, line: string) {
  const groupIndex = key === WATCHDOG_LOG_KEY ? 0 : 1
  const source = sourceForKey(key)
  const lineIndex = rawLines.value.filter((item) => item.groupKey === key).length
  const next = parseLine(key, source, line, lineIndex, groupIndex)
  rawLines.value = renumber([...rawLines.value, next].slice(-1000))
}

function clearConnectionLogs(key: string) {
  rawLines.value = renumber(rawLines.value.filter((line) => line.groupKey !== key))
}

function watchdogTitleKey(code?: string) {
  if (code === 'owner_lost') return 'events.watchdogOwnerLostTitle'
  if (code === 'owner_heartbeat_expired') return 'events.watchdogOwnerHeartbeatExpiredTitle'
  if (code === 'watchdog_eof') return 'events.watchdogDisconnectedTitle'
  return 'events.watchdogWarningTitle'
}

function sourceForKey(key: string) {
  if (key === WATCHDOG_LOG_KEY) return t('logs.source.watchdog')
  return resourcesByKey.value.get(key)?.name ?? key
}

function buildTimeline(input: Array<Omit<LogGroup, 'groupIndex'>>): DisplayLine[] {
  const groups: LogGroup[] = input.map((group, groupIndex) => ({ ...group, groupIndex }))
  const parsed = groups.map((group) =>
    group.lines.map((text, index) => parseLine(group.key, group.source, text, index, group.groupIndex)),
  )
  const cursors = parsed.map(() => 0)
  const merged: DisplayLine[] = []

  loop:
  while (true) {
    let nextGroup = -1
    for (let groupIndex = 0; groupIndex < parsed.length; groupIndex += 1) {
      const line = parsed[groupIndex][cursors[groupIndex]]
      if (!line) continue
      if (nextGroup < 0 || compareLogLines(line, parsed[nextGroup][cursors[nextGroup]]) < 0) {
        nextGroup = groupIndex
      }
    }
    if (nextGroup < 0) break loop
    merged.push(parsed[nextGroup][cursors[nextGroup]])
    cursors[nextGroup] += 1
  }

  return renumber(merged)
}

function renumber(lines: DisplayLine[]) {
  return lines.map((line, index) => ({ ...line, number: index + 1 }))
}

function parseLine(key: string, source: string, text: string, index: number, groupIndex: number): DisplayLine {
  return {
    key: `${key}-${index}-${text}`,
    groupKey: key,
    source,
    text,
    level: cachedLogLevel(text),
    number: 0,
    sortTime: parseLogTime(text),
    sortIndex: groupIndex * 100_000 + index,
    searchText: `${source}\n${text}`.toLowerCase(),
  }
}

function compareLogLines(left: DisplayLine, right: DisplayLine) {
  if (left.sortTime !== null && right.sortTime !== null && left.sortTime !== right.sortTime) {
    return left.sortTime - right.sortTime
  }
  if (left.sortTime !== null && right.sortTime === null) return -1
  if (left.sortTime === null && right.sortTime !== null) return 1
  return left.sortIndex - right.sortIndex
}

function parseLogTime(text: string): number | null {
  const local = text.match(/^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})(?:\.(\d{1,3}))?/)
  if (local) {
    const [, year, month, day, hour, minute, second, millis = '0'] = local
    return new Date(
      Number(year),
      Number(month) - 1,
      Number(day),
      Number(hour),
      Number(minute),
      Number(second),
      Number(millis.padEnd(3, '0')),
    ).getTime()
  }
  const iso = text.match(/^(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,3})?Z)/)
  if (!iso) return null
  const value = Date.parse(iso[1])
  return Number.isFinite(value) ? value : null
}

function cachedLogLevel(text: string): LogLevel {
  const cached = logLevelCache.get(text)
  if (cached) return cached
  const level = detectLogLevel(text)
  if (logLevelCache.size >= LOG_LEVEL_CACHE_LIMIT) logLevelCache.clear()
  logLevelCache.set(text, level)
  return level
}

function scrollBottom() {
  const element = boxRef.value
  if (!element) return
  element.scrollTop = element.scrollHeight
  syncViewport()
}

function onScroll() {
  const element = boxRef.value
  if (!element) return
  syncViewport()
  if (element.scrollHeight - element.scrollTop - element.clientHeight > 28) ui.setLogFollow(false)
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

function setWrap(event: Event) { ui.setLogWrap((event.target as HTMLInputElement).checked) }
function setFollow(event: Event) {
  ui.setLogFollow((event.target as HTMLInputElement).checked)
  if (ui.logFollow) nextTick(scrollBottom)
}
function resumeFollow() { ui.setLogFollow(true); nextTick(scrollBottom) }
function clear() { rawLines.value = [] }

function buildLogText(): string {
  return filtered.value
    .map((line) => `[${line.source}] ${line.text}`)
    .join('\n')
}

async function copyAll() {
  const text = buildLogText()
  if (!text) { ui.notify(t('logs.nothingToCopy'), 'info'); return }
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
  transition: border-color 120ms ease, box-shadow 120ms ease;
}
.search:focus-within {
  border-color: var(--tx-dark-border-hover);
  box-shadow: 0 0 0 3px var(--tx-focus-ring);
}
.search svg { width: 13px; height: 13px; }
.search input {
  flex: 1;
  min-width: 0;
  border: 0;
  outline: 0;
  background: transparent;
  color: #d5dbe4;
  font: inherit;
}
.level-select { width: 112px; }
.level-select :deep(.trigger) {
  height: 26px;
  border-color: #303744;
  background: #171b22;
  color: #aab3c0;
  font-size: 11px;
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
.check:focus-within { box-shadow: inset 0 0 0 1px var(--tx-dark-border-hover); }
.check input { accent-color: var(--tx-accent); }
.check input:focus { outline: none; }
.count { margin-left: auto; color: #737f90; }
.toolbar-action {
  height: 26px;
  border: 1px solid #303744;
  border-radius: 5px;
  background: transparent;
  color: #aab3c0;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 0 7px;
  font: inherit;
  cursor: pointer;
  transition: border-color 120ms ease, background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.toolbar-action:hover,
.toolbar-action:focus-visible {
  border-color: var(--tx-dark-border-hover);
  background: var(--tx-dark-bg-hover);
  color: var(--tx-dark-text-hover);
}
.toolbar-action:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-dark-border-hover);
}
.toolbar-action svg { width: 13px; height: 13px; }
.logs {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 8px 0;
  position: relative;
  font-family: var(--tx-mono);
  font-size: 11px;
  line-height: 1.55;
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
.empty-log { color: #687386; padding: 12px 12px 12px 64px; }
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
.jump svg { width: 13px; height: 13px; }

@media (max-width: 820px) {
  .toolbar { gap: 8px; }
  .search { width: min(180px, 32vw); }
  .check { font-size: 0; }
  .check input { margin: 0; }
}
</style>
