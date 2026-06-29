<template>
  <aside class="sidebar">
    <div class="section-head">
      <span>{{ t('sidebar.connections') }}</span>
      <TxIconButton icon="lucide:plus" :label="t('sidebar.createConnection')" @click="emit('create')" />
    </div>

    <nav class="nav">
      <RouterLink to="/connections" class="nav-item all">
        <span class="drag-slot drag-placeholder" aria-hidden="true" />
        <span class="nav-icon">
          <Icon icon="lucide:layers-3" />
        </span>
        <span class="connection-status status-placeholder" aria-hidden="true" />
        <span class="nav-label">{{ t('sidebar.allConnections') }}</span>
        <span class="count">{{ totalConnectionCount }}</span>
      </RouterLink>

      <div class="sidebar-connections">
        <RouterLink
          v-for="item in connections.resources"
          :key="`${item.providerId}:${item.id}`"
          :to="connectionPath(item.providerId, item.id)"
          class="sidebar-connection"
          :class="{ current: currentConnectionKey === connectionKey(item.providerId, item.id) }"
        >
          <span
            class="drag-slot"
            role="button"
            :aria-label="t('sidebar.reorder')"
            @pointerdown="onDragPointerDown(item, $event)"
            @click.stop
            @dblclick.stop
          >
            <Icon class="drag-handle" icon="lucide:grip-vertical" />
          </span>
          <span class="nav-icon">
            <ProviderIcon class="provider-mark" :provider-id="item.providerId" />
          </span>
          <TxStatus class="connection-status" :status="connections.runtimeOf(item).status" :show-label="false" />
          <span class="name">{{ item.name }}</span>
          <span class="proxy-count">{{ providerResourceTunnelCount(item) }}</span>
        </RouterLink>
      </div>
    </nav>

    <div class="bottom-nav">
      <RouterLink to="/activity" class="nav-item">
        <span class="nav-icon">
          <Icon icon="lucide:activity" />
        </span>
        <span class="nav-label">{{ t('sidebar.activity') }}</span>
      </RouterLink>
      <RouterLink
        to="/settings/general"
        class="nav-item"
        :class="{ 'router-link-active': route.path.startsWith('/settings') }"
      >
        <span class="nav-icon">
          <Icon icon="lucide:settings-2" />
        </span>
        <span class="nav-label">{{ t('sidebar.settings') }}</span>
      </RouterLink>
    </div>

    <div class="runtime-meter" :class="{ compact: !shouldShowRuntimeChart }">
      <svg v-if="shouldShowRuntimeChart" class="traffic-chart" viewBox="0 0 180 40" preserveAspectRatio="none" aria-hidden="true">
        <path v-if="hasVisibleChartData" class="traffic-baseline" d="M 5 36 L 175 36" />
        <path
          v-for="direction in trafficLayerOrder"
          :key="`${direction}-area`"
          class="traffic-area"
          :class="trafficClass(direction)"
          :d="trafficAreaPath(direction)"
        />
        <path
          v-for="direction in trafficLayerOrder"
          :key="`${direction}-line`"
          class="traffic-line"
          :class="trafficClass(direction)"
          :d="trafficLinePath(direction)"
        />
        <path v-if="memoryLinePath" class="traffic-line memory" :d="memoryLinePath" />
      </svg>
      <div class="meter-speeds mono" :class="{ 'memory-only': !settings.trafficStatsEnabled }">
        <TxTooltip v-if="settings.trafficStatsEnabled" class="meter-item down" :text="t('proxyList.traffic.down')" focusable>
          <span class="meter-content down">
            <Icon icon="lucide:arrow-down" />
            <span class="meter-value">{{ formatCompactSpeed(totalDownloadSpeed) }}</span>
          </span>
        </TxTooltip>
        <TxTooltip v-if="settings.trafficStatsEnabled" class="meter-item up" :text="t('proxyList.traffic.up')" focusable>
          <span class="meter-content up">
            <Icon icon="lucide:arrow-up" />
            <span class="meter-value">{{ formatCompactSpeed(totalUploadSpeed) }}</span>
          </span>
        </TxTooltip>
        <TxTooltip class="meter-item" :text="t('connection.stats.memory')" focusable>
          <span class="meter-value">{{ memoryText }}</span>
        </TxTooltip>
      </div>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import { useRoute } from 'vue-router'
import TxIconButton from '@/components/ui/TxIconButton.vue'
import TxStatus from '@/components/ui/TxStatus.vue'
import TxTooltip from '@/components/ui/TxTooltip.vue'
import ProviderIcon from '@/providers/components/ProviderIcon.vue'
import { useSortable } from '@/composables/useSortable'
import { useSettingsStore } from '@/stores/settings'
import { useProviderMetricsStore } from '@/providers/metrics'
import { connectionKey, useConnectionStore } from '@/providers/connections'
import { providerResourceTunnelCount } from '@/providers/registry'
import { connectionPath } from '@/providers/routes'
import type { TunnelRuntimeState } from '@/providers/contract'
import { formatCompactSpeed, formatMemory } from '@/utils/format'

const { t } = useI18n()
const route = useRoute()
const emit = defineEmits<{ create: [] }>()
const settings = useSettingsStore()
const providerMetrics = useProviderMetricsStore()
const connections = useConnectionStore()
const chartNow = ref(Date.now())
const trafficScaleMax = ref(1)
const memoryHistory = ref<MemoryPoint[]>([])
const appVisible = ref(typeof document === 'undefined' || !document.hidden)
let metricsTimer: number | null = null
let metricsTimerDelay = 0
let trafficChartFrame: number | null = null
let trafficScaleFrameAt = 0
let metricsRefreshInFlight = false
const currentConnectionKey = computed(() => {
  const providerId = route.params.providerId as string | undefined
  const id = route.params.id as string | undefined
  return providerId && id ? connectionKey(providerId, id) : ''
})
const memoryText = computed(() => formatMemory(providerMetrics.appMemoryBytes))
const totalConnectionCount = computed(() => connections.count)
const sortable = useSortable({
  ids: () => connections.resources.map((item) => connectionKey(item.providerId, item.id)),
  itemSelector: '.sidebar-connection',
  handleItemSelector: '.sidebar-connection',
  onReorder: (keys) => {
    void connections.reorder(keys)
  },
})
type TrafficDirection = 'download' | 'upload'
type TrafficPoint = { timestamp: number; observedAt: number; download: number; upload: number }
type TrafficPathPoint = { x: number; y: number; value: number }
type MemoryPoint = { timestamp: number; bytes: number }
const TRAFFIC_WINDOW_SECONDS = 30
const TRAFFIC_WINDOW_MS = TRAFFIC_WINDOW_SECONDS * 1000
const TRAFFIC_BUCKET_MS = 1000
const MEMORY_HISTORY_KEEP_MS = TRAFFIC_WINDOW_MS + TRAFFIC_BUCKET_MS * 2
const TRAFFIC_CHART_LEFT = 5
const TRAFFIC_CHART_WIDTH = 170
const TRAFFIC_CHART_RIGHT = TRAFFIC_CHART_LEFT + TRAFFIC_CHART_WIDTH
const TRAFFIC_CHART_BASELINE = 36
const TRAFFIC_CHART_RANGE = 28
const TRAFFIC_VISIBLE_MIN_VALUE = 0.5
const TRAFFIC_ENTRY_EASE_MS = 900
const TRAFFIC_SCALE_DECAY_MS = 1200
const ACTIVE_METRICS_POLL_MS = 1000
const IDLE_METRICS_POLL_MS = 15000
function isLiveRuntimeStatus(status: TunnelRuntimeState): boolean {
  return status === 'starting' || status === 'running' || status === 'warning' || status === 'stopping'
}
const hasActiveRuntimeConnections = computed(() =>
  connections.resources.some((resource) => isLiveRuntimeStatus(connections.runtimeOf(resource).status)),
)
const metricsPollMs = computed(() =>
  hasActiveRuntimeConnections.value ? ACTIVE_METRICS_POLL_MS : IDLE_METRICS_POLL_MS,
)
const trafficTotals = computed(() => {
  if (!settings.trafficStatsEnabled) {
    return { hasRunning: hasActiveRuntimeConnections.value, download: 0, upload: 0 }
  }
  return {
    hasRunning: hasActiveRuntimeConnections.value || providerMetrics.hasActiveTunnels,
    download: providerMetrics.downloadSpeed,
    upload: providerMetrics.uploadSpeed,
  }
})
const trafficChart = computed(() => {
  const buckets = new Map<number, TrafficPoint>()
  const now = chartNow.value
  const start = now - TRAFFIC_WINDOW_MS
  const firstBucketTime = Math.floor(start / TRAFFIC_BUCKET_MS) * TRAFFIC_BUCKET_MS - TRAFFIC_BUCKET_MS
  const lastBucketTime = Math.floor(now / TRAFFIC_BUCKET_MS) * TRAFFIC_BUCKET_MS + TRAFFIC_BUCKET_MS
  const sampleStart = start - TRAFFIC_BUCKET_MS
  const sampleEnd = now + TRAFFIC_BUCKET_MS
  let latestSampleAt = 0
  if (settings.trafficStatsEnabled) {
    for (const sample of providerMetrics.trafficHistory) {
      latestSampleAt = Math.max(latestSampleAt, sample.timestamp)
      if (sample.timestamp < sampleStart || sample.timestamp > sampleEnd) continue
      const bucketTime = Math.floor(sample.timestamp / TRAFFIC_BUCKET_MS) * TRAFFIC_BUCKET_MS
      const bucket = buckets.get(bucketTime) ?? { timestamp: bucketTime, observedAt: sample.timestamp, download: 0, upload: 0 }
      bucket.timestamp = Math.min(bucketTime + TRAFFIC_BUCKET_MS, Math.max(bucket.timestamp, sample.timestamp))
      bucket.observedAt = Math.max(bucket.observedAt, sample.timestamp)
      bucket.download += sample.downloadSpeed
      bucket.upload += sample.uploadSpeed
      buckets.set(bucketTime, bucket)
    }
  }
  const series: TrafficPoint[] = []
  for (let timestamp = firstBucketTime; timestamp <= lastBucketTime; timestamp += TRAFFIC_BUCKET_MS) {
    series.push(buckets.get(timestamp) ?? { timestamp, observedAt: timestamp, download: 0, upload: 0 })
  }
  let max = 1
  let downTotal = 0
  let upTotal = 0
  for (const item of series) {
    max = Math.max(max, item.download, item.upload)
    downTotal += item.download
    upTotal += item.upload
  }
  return {
    series,
    max,
    layerOrder: downTotal >= upTotal ? ['download', 'upload'] as TrafficDirection[] : ['upload', 'download'] as TrafficDirection[],
    latestSampleAt,
  }
})
const hasRunningConnections = computed(() => trafficTotals.value.hasRunning)
const totalDownloadSpeed = computed(() => trafficTotals.value.download)
const totalUploadSpeed = computed(() => trafficTotals.value.upload)
const trafficSeries = computed(() => trafficChart.value.series)
const trafficMax = computed(() => trafficChart.value.max)
const trafficLayerOrder = computed(() => trafficChart.value.layerOrder)
const hasVisibleTraffic = computed(() =>
  trafficSeries.value.some((sample) => sample.download > TRAFFIC_VISIBLE_MIN_VALUE || sample.upload > TRAFFIC_VISIBLE_MIN_VALUE),
)
const memoryChart = computed(() => {
  const now = chartNow.value
  const start = now - TRAFFIC_WINDOW_MS
  const samples = memoryHistory.value.filter((sample) => sample.timestamp >= start - TRAFFIC_BUCKET_MS && sample.timestamp <= now + TRAFFIC_BUCKET_MS)
  const latestSampleAt = samples.reduce((latest, sample) => Math.max(latest, sample.timestamp), 0)
  if (!samples.length) {
    return { samples, min: 0, max: 1, latestSampleAt }
  }
  let min = Math.min(...samples.map((sample) => sample.bytes))
  let max = Math.max(...samples.map((sample) => sample.bytes))
  const minRange = Math.max(max * 0.08, 1024 * 1024)
  if (max - min < minRange) {
    const middle = (min + max) / 2
    min = Math.max(0, middle - minRange / 2)
    max = middle + minRange / 2
  }
  return { samples, min, max, latestSampleAt }
})
const hasVisibleMemory = computed(() => memoryChart.value.samples.length > 0)
const hasVisibleChartData = computed(() => hasVisibleTraffic.value || hasVisibleMemory.value)
const hasRecentMemoryHistory = computed(() => memoryChart.value.latestSampleAt >= chartNow.value - TRAFFIC_WINDOW_MS)
const shouldShowRuntimeChart = computed(() =>
  settings.trafficStatsEnabled || hasVisibleMemory.value || providerMetrics.appMemoryBytes != null,
)
const hasRecentTrafficHistory = computed(() =>
  settings.trafficStatsEnabled && trafficChart.value.latestSampleAt >= chartNow.value - TRAFFIC_WINDOW_MS,
)
const shouldRunTrafficChartTimer = computed(() =>
  appVisible.value
  && shouldShowRuntimeChart.value
  && ((settings.trafficStatsEnabled && (hasRunningConnections.value || hasRecentTrafficHistory.value)) || hasRecentMemoryHistory.value),
)
const memoryLinePath = computed(() => {
  const points = memoryPathPoints()
  return points.length ? smoothPath(points) : ''
})

function onDragPointerDown(item: { providerId: string; id: string }, event: PointerEvent) {
  sortable.start(connectionKey(item.providerId, item.id), event)
}

onMounted(async () => {
  if (typeof document !== 'undefined') {
    appVisible.value = !document.hidden
    document.addEventListener('visibilitychange', syncVisibilityState)
  }
  try {
    await connections.init()
    if (appVisible.value) {
      startMetricsTimer()
    }
    syncRuntimeTimers()
  } catch {
    // The Tauri bridge is unavailable in a regular web preview.
  }
})
onBeforeUnmount(() => {
  if (typeof document !== 'undefined') {
    document.removeEventListener('visibilitychange', syncVisibilityState)
  }
  stopMetricsTimer()
  stopTrafficChartTimer()
})

async function refreshMetrics() {
  if (metricsRefreshInFlight || !appVisible.value) return
  metricsRefreshInFlight = true
  try {
    await providerMetrics.refresh()
    chartNow.value = Date.now()
    recordMemorySample()
    syncRuntimeTimers()
  } catch {
    providerMetrics.runtime = null
  } finally {
    metricsRefreshInFlight = false
  }
}

function startMetricsTimer() {
  if (!appVisible.value) return
  const delay = metricsPollMs.value
  if (metricsTimer != null && metricsTimerDelay === delay) return
  stopMetricsTimer()
  metricsTimerDelay = delay
  void refreshMetrics()
  metricsTimer = window.setInterval(refreshMetrics, delay)
}

function stopMetricsTimer() {
  if (metricsTimer == null) return
  window.clearInterval(metricsTimer)
  metricsTimer = null
  metricsTimerDelay = 0
}

function startTrafficChartTimer() {
  if (!appVisible.value) return
  if (trafficChartFrame != null) return
  chartNow.value = Date.now()
  const tick = () => {
    const now = Date.now()
    chartNow.value = now
    syncTrafficScaleMax(now)
    trafficChartFrame = shouldRunTrafficChartTimer.value
      ? window.requestAnimationFrame(tick)
      : null
  }
  trafficChartFrame = window.requestAnimationFrame(tick)
}

function stopTrafficChartTimer() {
  if (trafficChartFrame == null) return
  window.cancelAnimationFrame(trafficChartFrame)
  trafficChartFrame = null
  trafficScaleFrameAt = 0
}

function syncRuntimeTimers() {
  if (shouldRunTrafficChartTimer.value) startTrafficChartTimer()
  else stopTrafficChartTimer()
}

function syncVisibilityState() {
  appVisible.value = typeof document === 'undefined' || !document.hidden
  if (appVisible.value) {
    startMetricsTimer()
    syncRuntimeTimers()
  } else {
    stopMetricsTimer()
    stopTrafficChartTimer()
  }
}

watch([hasRunningConnections, shouldRunTrafficChartTimer], syncRuntimeTimers)
watch(metricsPollMs, () => {
  if (appVisible.value) startMetricsTimer()
})
watch(trafficMax, () => {
  if (trafficChartFrame == null) {
    trafficScaleMax.value = trafficDisplayMax(Date.now())
  }
})

function syncTrafficScaleMax(now: number) {
  const target = trafficDisplayMax(now)
  const current = Math.max(1, trafficScaleMax.value)
  if (target >= current || trafficScaleFrameAt === 0) {
    trafficScaleMax.value = target
    trafficScaleFrameAt = now
    return
  }
  const elapsed = Math.min(250, Math.max(0, now - trafficScaleFrameAt))
  const factor = 1 - Math.exp(-elapsed / TRAFFIC_SCALE_DECAY_MS)
  trafficScaleMax.value = current + (target - current) * factor
  trafficScaleFrameAt = now
}

function trafficDisplayMax(now: number): number {
  let max = 1
  for (const sample of trafficSeries.value) {
    max = Math.max(max, trafficDisplayValue(sample, 'download', now), trafficDisplayValue(sample, 'upload', now))
  }
  return max
}

function trafficDisplayValue(sample: TrafficPoint, direction: TrafficDirection, now: number): number {
  const value = direction === 'download' ? sample.download : sample.upload
  if (value <= 0) return 0
  const age = now - sample.observedAt
  if (age >= TRAFFIC_ENTRY_EASE_MS) return value
  const progress = Math.max(0, age) / TRAFFIC_ENTRY_EASE_MS
  const eased = progress * progress * (3 - 2 * progress)
  return value * eased
}

function trafficClass(direction: TrafficDirection): 'down' | 'up' {
  return direction === 'download' ? 'down' : 'up'
}

function trafficLinePath(direction: TrafficDirection): string {
  const points = activeTrafficPathPoints(direction)
  return points.length ? smoothPath(points) : ''
}

function trafficAreaPath(direction: TrafficDirection): string {
  const points = activeTrafficPathPoints(direction)
  if (!points.length) return ''
  return `${smoothPath(points)} L ${points[points.length - 1].x.toFixed(1)} ${TRAFFIC_CHART_BASELINE.toFixed(1)} L ${points[0].x.toFixed(1)} ${TRAFFIC_CHART_BASELINE.toFixed(1)} Z`
}

function memoryPathPoints(): TrafficPathPoint[] {
  const { samples, min, max } = memoryChart.value
  if (!samples.length) return []
  const scale = Math.max(1, max - min)
  const now = chartNow.value
  const start = now - TRAFFIC_WINDOW_MS
  const points = samples.map((sample) => {
    const x = TRAFFIC_CHART_LEFT + ((sample.timestamp - start) / TRAFFIC_WINDOW_MS) * TRAFFIC_CHART_WIDTH
    const y = TRAFFIC_CHART_BASELINE - ((sample.bytes - min) / scale) * TRAFFIC_CHART_RANGE
    return { x, y, value: sample.bytes }
  })
  return clipTrafficPathPoints(points)
}

function recordMemorySample() {
  const runtime = providerMetrics.runtime
  if (!runtime) return
  const bytes = runtime.appMemoryBytes
  if (bytes == null || !Number.isFinite(bytes)) return
  const timestamp = Number.isFinite(runtime.collectedAt) ? runtime.collectedAt : Date.now()
  const cutoff = timestamp - MEMORY_HISTORY_KEEP_MS
  const next = memoryHistory.value
    .filter((sample) => sample.timestamp >= cutoff && sample.timestamp !== timestamp)
    .concat({ timestamp, bytes })
    .sort((a, b) => a.timestamp - b.timestamp)
  memoryHistory.value = next
}

function activeTrafficPathPoints(direction: TrafficDirection): TrafficPathPoint[] {
  const points = trafficPathPoints(direction)
  return points.some((point) => point.value > TRAFFIC_VISIBLE_MIN_VALUE) ? points : []
}

function trafficPathPoints(direction: TrafficDirection): TrafficPathPoint[] {
  const max = trafficScaleMax.value
  const now = chartNow.value
  const start = now - TRAFFIC_WINDOW_MS
  const visible = trafficSeries.value.filter((sample) => sample.timestamp >= start - TRAFFIC_BUCKET_MS && sample.timestamp <= now + TRAFFIC_BUCKET_MS)
  if (!visible.length) {
    return []
  }
  const points = visible.map((sample) => {
    const value = trafficDisplayValue(sample, direction, now)
    const x = TRAFFIC_CHART_LEFT + ((sample.timestamp - start) / TRAFFIC_WINDOW_MS) * TRAFFIC_CHART_WIDTH
    const y = TRAFFIC_CHART_BASELINE - (value / max) * TRAFFIC_CHART_RANGE
    return { x, y, value }
  })
  return clipTrafficPathPoints(points)
}

function clipTrafficPathPoints(points: TrafficPathPoint[]): TrafficPathPoint[] {
  if (!points.length) return []
  if (points.length === 1) {
    const [point] = points
    return [
      { ...point, x: TRAFFIC_CHART_LEFT },
      { ...point, x: TRAFFIC_CHART_RIGHT },
    ]
  }
  const clipped = [
    trafficPointAtX(points, TRAFFIC_CHART_LEFT),
    ...points.filter((point) => point.x > TRAFFIC_CHART_LEFT && point.x < TRAFFIC_CHART_RIGHT),
    trafficPointAtX(points, TRAFFIC_CHART_RIGHT),
  ].filter((point): point is TrafficPathPoint => point != null)
  return clipped.filter((point, index) => index === 0 || Math.abs(point.x - clipped[index - 1].x) > 0.001)
}

function trafficPointAtX(points: TrafficPathPoint[], x: number): TrafficPathPoint | null {
  const first = points[0]
  const last = points[points.length - 1]
  if (!first || !last) return null
  if (x <= first.x) return { ...first, x }
  if (x >= last.x) return { ...last, x }
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1]
    const next = points[index]
    if (x > next.x) continue
    const span = next.x - previous.x
    const progress = span <= 0 ? 0 : (x - previous.x) / span
    return {
      x,
      y: previous.y + (next.y - previous.y) * progress,
      value: previous.value + (next.value - previous.value) * progress,
    }
  }
  return { ...last, x }
}

function smoothPath(points: TrafficPathPoint[]): string {
  if (!points.length) {
    return ''
  }
  if (points.length === 1) {
    const point = points[0]
    return `M ${point.x.toFixed(1)} ${point.y.toFixed(1)}`
  }
  const [first, ...rest] = points
  const segments = [`M ${first.x.toFixed(1)} ${first.y.toFixed(1)}`]
  let previous = first
  for (const point of rest) {
    const dx = point.x - previous.x
    const c1x = previous.x + dx * 0.5
    const c2x = point.x - dx * 0.5
    segments.push(`C ${c1x.toFixed(1)} ${previous.y.toFixed(1)}, ${c2x.toFixed(1)} ${point.y.toFixed(1)}, ${point.x.toFixed(1)} ${point.y.toFixed(1)}`)
    previous = point
  }
  return segments.join(' ')
}
</script>

<style scoped>
.sidebar {
  width: 220px;
  flex-shrink: 0;
  min-height: 0;
  background: var(--tx-bg-sidebar);
  display: flex;
  flex-direction: column;
  padding: 8px 6px 8px 8px;
}
.section-head {
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 6px 0 9px;
  color: var(--tx-text-muted);
  font-size: 11px;
  font-weight: 650;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.nav { flex: 1; min-height: 0; display: flex; flex-direction: column; }
.sidebar-connections { min-height: 0; overflow-y: auto; padding-top: 4px; }
.nav-item,
.sidebar-connection {
  height: 32px;
  border-radius: var(--tx-radius-sm);
  color: var(--tx-text-secondary);
  display: flex;
  align-items: center;
  padding: 0 8px;
  text-decoration: none;
  transition: background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.nav-item:hover,
.nav-item:focus-visible,
.sidebar-connection:hover,
.sidebar-connection:focus-visible { background: var(--tx-bg-hover); color: var(--tx-text-primary); }
.nav-item:focus-visible,
.sidebar-connection:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.nav-item.router-link-active,
.sidebar-connection.current { background: var(--tx-bg-selected); color: var(--tx-text-primary); }
.sidebar-connection.tx-sort-lifted {
  background: var(--tx-bg-selected);
}
.drag-slot {
  width: 14px;
  height: 20px;
  flex: 0 0 14px;
  display: grid;
  place-items: center;
  margin-right: 4px;
  color: var(--tx-text-muted);
  cursor: grab;
}
.drag-slot:hover,
.drag-slot:focus-visible {
  color: var(--tx-text-secondary);
}
.drag-slot:focus-visible {
  outline: none;
  border-radius: var(--tx-radius-sm);
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.drag-placeholder {
  pointer-events: none;
  visibility: hidden;
}
.drag-handle {
  width: 14px;
  height: 14px;
}
.nav-icon {
  width: 18px;
  height: 20px;
  flex: 0 0 18px;
  display: grid;
  place-items: center;
  margin-right: 8px;
}
.nav-icon svg,
.nav-icon img {
  width: 16px;
  height: 16px;
}
.provider-mark {
  color: var(--tx-text-muted);
}
.connection-status {
  flex: 0 0 7px;
  width: 7px;
  margin-right: 8px;
}
.status-placeholder {
  visibility: hidden;
}
.nav-label,
.name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.count,
.proxy-count {
  flex: 0 0 18px;
  width: 18px;
  color: var(--tx-text-muted);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  text-align: right;
}
.count { margin-left: auto; }
.runtime-meter {
  --traffic-down: #4b65d9;
  --traffic-up: #2b9aa0;
  --runtime-memory: #9a6f2f;
  flex-shrink: 0;
  border-top: 1px solid var(--tx-border-subtle);
  margin-top: 7px;
  padding: 8px 7px 0;
  display: grid;
  gap: 4px;
}
.runtime-meter.compact {
  gap: 0;
}
.traffic-chart {
  width: 100%;
  height: 40px;
  border: 0;
  background: var(--tx-bg-sidebar);
  border-radius: 4px;
  overflow: hidden;
  display: block;
}
.traffic-chart path {
  stroke-width: 1.35;
  stroke-linecap: round;
  stroke-linejoin: round;
  vector-effect: non-scaling-stroke;
}
.traffic-chart .traffic-baseline {
  fill: none;
  stroke: var(--tx-border-subtle);
  stroke-width: 1;
  opacity: 0.55;
}
.traffic-chart .traffic-area {
  stroke: none;
  opacity: 0.14;
}
.traffic-chart .traffic-line {
  fill: none;
  opacity: 1;
}
.traffic-chart .traffic-area.down { fill: var(--traffic-down); }
.traffic-chart .traffic-area.up { fill: var(--traffic-up); opacity: 0.12; }
.traffic-chart .down { stroke: var(--traffic-down); }
.traffic-chart .up { stroke: var(--traffic-up); }
.traffic-chart .memory { stroke: var(--runtime-memory); }
.meter-speeds {
  min-width: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  justify-items: center;
  gap: 4px;
  color: var(--tx-text-muted);
  font-size: 10px;
  line-height: 1.2;
}
.meter-speeds.memory-only {
  grid-template-columns: 1fr;
  justify-items: center;
}
.meter-item {
  min-width: 0;
  max-width: 100%;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  justify-content: center;
  white-space: nowrap;
}
.meter-content {
  min-width: 0;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.meter-value {
  min-width: 34px;
  overflow: visible;
  font-size: 10px;
  text-align: left;
}
.meter-content svg,
.meter-item > svg {
  width: 10px;
  height: 10px;
  flex-shrink: 0;
  color: currentColor;
}
.meter-content.down { color: var(--traffic-down); }
.meter-content.up { color: var(--traffic-up); }
.bottom-nav {
  padding-top: 7px;
  border-top: 1px solid var(--tx-border-subtle);
  display: grid;
  gap: 2px;
}
</style>
