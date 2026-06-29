<template>
  <WorkspacePage wide>
    <div class="overview-grid">
      <section class="panel status-panel">
        <div class="panel-title">
          <div>
            <h3>{{ t('connection.overview.statusTitle') }}</h3>
          </div>
        </div>
        <dl class="facts">
          <div class="fact-main"><dt>{{ t('connection.head.connectionInfo') }}</dt><dd class="mono">{{ connectionInfo }}</dd></div>
          <div class="fact-compact"><dt>{{ t('connection.head.tunnels') }}</dt><dd>{{ tunnelText }}</dd></div>
          <div class="fact-compact"><dt>PID</dt><dd>{{ runtimeInfo.pid || '-' }}</dd></div>
        </dl>
      </section>

      <section class="panel tunnel-panel">
        <div class="panel-title">
          <div>
            <h3>{{ t('connection.overview.tunnelsTitle') }}</h3>
            <p>{{ t('connection.overview.tunnelsSubtitle') }}</p>
          </div>
          <RouterLink :to="connectionPath(providerId, id, 'tunnels')">{{ t('connection.overview.viewAll') }}</RouterLink>
        </div>
        <div v-if="tunnelRows.length" class="tunnel-list">
          <div v-for="row in tunnelRows" :key="row.key" class="tunnel-card">
            <div class="tunnel-card-head">
              <div class="tunnel-card-main">
                <strong>{{ row.name || '-' }}</strong>
                <code>{{ row.target || '-' }}</code>
              </div>
              <b>{{ row.kind }}</b>
            </div>
            <div class="public-url-list">
              <span class="public-url-label">{{ t('connection.overview.publicUrls') }}</span>
              <button
                v-for="url in row.publicUrls ?? []"
                :key="url"
                class="public-url mono"
                type="button"
                @click="copyPublicUrl(url)"
              >
                <span>{{ url }}</span>
                <Icon icon="lucide:copy" />
              </button>
              <span v-if="!(row.publicUrls ?? []).length" class="public-url-empty">
                {{ t('connection.overview.noPublicUrls') }}
              </span>
            </div>
          </div>
        </div>
        <p v-else class="empty-copy">{{ t('connection.overview.noTunnels') }}</p>
      </section>

      <section v-if="quotaRows.length" class="panel quota-panel">
        <div class="panel-title">
          <div>
            <h3>{{ t('connection.overview.quotaTitle') }}</h3>
          </div>
        </div>
        <div class="quota-list">
          <div v-for="row in quotaRows" :key="row.key" class="quota-row" :class="row.tone || 'default'">
            <span>{{ row.label }}</span>
            <b>{{ row.value }}</b>
          </div>
        </div>
      </section>

      <section class="panel logs-panel">
        <div class="panel-title">
          <div>
            <h3>{{ t('connection.overview.logsTitle') }}</h3>
            <p>{{ t('connection.overview.logsSubtitle') }}</p>
          </div>
          <RouterLink :to="connectionPath(providerId, id, 'logs')">{{ t('connection.overview.openLogs') }}</RouterLink>
        </div>
        <div v-if="latestLogs.length" class="recent-logs">
          <LogLine
            v-for="(line, index) in latestLogs"
            :key="`${index}-${line}`"
            :text="line"
            compact
            wrap
          />
        </div>
        <p v-else class="empty-copy">{{ t('connection.overview.logsEmpty') }}</p>
      </section>
    </div>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { listen } from '@tauri-apps/api/event'
import { Icon } from '@iconify/vue'
import LogLine from '@/components/ui/LogLine.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { useClipboard } from '@/composables/useClipboard'
import { providerTunnelLogs } from '@/providers/api'
import { useConnectionStore } from '@/providers/connections'
import type { TunnelRuntimeLogEvent } from '@/providers/contract'
import {
  providerResourceConnectionInfo,
  providerResourceQuotaRows,
  providerResourceTunnelRows,
  providerResourceTunnelText,
} from '@/providers/registry'
import { connectionPath } from '@/providers/routes'
import { useRoute } from 'vue-router'

const route = useRoute()
const { t } = useI18n()
const { copyText } = useClipboard()
const connections = useConnectionStore()
const logs = ref<string[]>([])
const unlisteners: Array<() => void> = []
let unmounted = false
const providerId = computed(() => String(route.params.providerId ?? ''))
const id = computed(() => String(route.params.id ?? ''))
const resource = computed(() => connections.resourceOf(providerId.value, id.value))
const runtimeInfo = computed(() =>
  resource.value
    ? connections.runtimeOf(resource.value)
    : {
        providerId: providerId.value,
        tunnelId: id.value,
        status: 'stopped' as const,
        pid: null,
        message: '',
        details: {},
      },
)
const latestLogs = computed(() => logs.value.slice(-5))
const connectionInfo = computed(() => {
  const item = resource.value
  if (!item) return t('connection.notConfigured')
  return providerResourceConnectionInfo(item, runtimeInfo.value) || t('connection.notConfigured')
})
const tunnelText = computed(() => {
  const item = resource.value
  if (!item) return '0'
  return providerResourceTunnelText(item, runtimeInfo.value)
})
const tunnelRows = computed(() => {
  const item = resource.value
  if (!item) return []
  return providerResourceTunnelRows(item, runtimeInfo.value)
})
const quotaRows = computed(() => {
  const item = resource.value
  if (!item) return []
  return providerResourceQuotaRows(item, runtimeInfo.value)
})

async function copyPublicUrl(url: string) {
  await copyText(url, t('connection.overview.publicUrlCopied'))
}

watch([providerId, id], async () => {
  if (!providerId.value || !id.value) return
  logs.value = await providerTunnelLogs(providerId.value, id.value).catch(() => [])
}, { immediate: true })

onMounted(async () => {
  try {
    const unlisten = await listen<TunnelRuntimeLogEvent>('provider-tunnel-log', (event) => {
      const payload = event.payload
      if (payload.providerId !== providerId.value || payload.tunnelId !== id.value) return
      if (payload.reset) {
        logs.value = []
        return
      }
      logs.value = [...logs.value, payload.line].slice(-1000)
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
</script>

<style scoped>
.overview-grid {
  display: grid;
  gap: 20px;
}
.panel {
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: var(--tx-bg-surface);
  padding: 18px;
}
.panel-title {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 18px;
}
.panel-title h3 {
  margin: 0;
  color: var(--tx-text-primary);
  font-size: 15px;
}
.panel-title p {
  margin: 5px 0 0;
  color: var(--tx-text-secondary);
  font-size: 12px;
}
.panel-title a {
  color: var(--tx-accent);
  font-size: 12px;
  text-decoration: none;
}
.facts {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(104px, 0.22fr) minmax(104px, 0.22fr);
  gap: 0;
}
.facts div {
  min-width: 0;
  border-left: 1px solid var(--tx-border-subtle);
  padding: 0 18px;
}
.facts div:first-child { border-left: 0; padding-left: 0; }
.facts dt {
  color: var(--tx-text-muted);
  font-size: 12px;
}
.facts dd {
  margin: 8px 0 0;
  overflow: hidden;
  color: var(--tx-text-primary);
  font-size: 14px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fact-compact dd {
  font-size: 13px;
}
.tunnel-list {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 10px;
}
.tunnel-card {
  min-width: 0;
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-sm);
  background: var(--tx-bg-surface);
  padding: 12px 14px;
}
.tunnel-card-head {
  min-width: 0;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}
.tunnel-card-main {
  min-width: 0;
}
.tunnel-card-main strong,
.tunnel-card-main code {
  display: block;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.tunnel-card-main strong {
  color: var(--tx-text-primary);
  font-size: 14px;
  font-weight: 650;
}
.tunnel-card-main code {
  margin-top: 6px;
  color: var(--tx-text-secondary);
  font-size: 12px;
}
.tunnel-card b {
  flex: none;
  border-radius: 999px;
  background: var(--tx-bg-muted);
  color: var(--tx-text-muted);
  font-size: 11px;
  font-weight: 650;
  line-height: 18px;
  padding: 0 8px;
}
.public-url-list {
  min-width: 0;
  display: grid;
  grid-template-columns: max-content minmax(0, 1fr);
  gap: 4px 10px;
  align-items: start;
  margin-top: 10px;
}
.public-url-label {
  color: var(--tx-text-muted);
  font-size: 12px;
  line-height: 22px;
}
.public-url {
  max-width: 100%;
  min-width: 0;
  min-height: 22px;
  border: 0;
  border-radius: 0;
  background: transparent;
  color: var(--tx-accent);
  display: inline-flex;
  align-items: center;
  justify-self: start;
  gap: 5px;
  padding: 0;
  cursor: pointer;
  font-size: 12px;
  line-height: 22px;
  transition: color 120ms ease;
}
.public-url + .public-url {
  grid-column: 2;
}
.public-url span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.public-url svg {
  width: 11px;
  height: 11px;
  flex: none;
  color: var(--tx-text-muted);
}
.public-url:hover,
.public-url:focus-visible {
  color: var(--tx-accent-hover);
}
.public-url:focus-visible {
  outline: none;
}
.public-url:hover span,
.public-url:focus-visible span {
  text-decoration: underline;
  text-underline-offset: 3px;
}
.public-url-empty {
  grid-column: 2;
  color: var(--tx-text-muted);
  font-size: 12px;
  line-height: 22px;
}
.quota-list {
  overflow: hidden;
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-sm);
}
.quota-row {
  min-height: 38px;
  border-bottom: 1px solid var(--tx-border-subtle);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 0 12px;
}
.quota-row:last-child {
  border-bottom: 0;
}
.quota-row span {
  color: var(--tx-text-secondary);
  font-size: 12px;
}
.quota-row b {
  color: var(--tx-text-primary);
  font-size: 13px;
  font-weight: 650;
}
.quota-row.warning b {
  color: #b45309;
}
.quota-row.danger b {
  color: var(--tx-danger);
}
.recent-logs {
  max-height: 220px;
  overflow: auto;
  border-radius: var(--tx-radius-sm);
  background: #101318;
  padding: 8px;
}
.empty-copy {
  margin: 0;
  color: var(--tx-text-muted);
  font-size: 13px;
}

@media (max-width: 760px) {
  .facts {
    grid-template-columns: 1fr;
  }
  .facts div {
    border-left: 0;
    border-top: 1px solid var(--tx-border-subtle);
    padding: 12px 0 0;
  }
  .facts div:first-child {
    border-top: 0;
    padding-top: 0;
  }
  .public-url-list {
    grid-template-columns: 1fr;
  }
  .public-url + .public-url,
  .public-url-empty {
    grid-column: 1;
  }
}
</style>
