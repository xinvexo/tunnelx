<template>
  <div class="connections-layout">
    <ConnectionTopBar :title="t('connection.allTitle')" :subtitle="t('connection.allSubtitle')">
      <template #actions>
        <TxButton
          :icon="batchAction.icon"
          :tone="batchAction.tone"
          :loading="batchAction.loading"
          :disabled="batchAction.disabled"
          @click="runBatchAction"
        >
          {{ batchAction.label }}
        </TxButton>
        <TxButton icon="lucide:plus" @click="createOpen = true">{{ t('connection.create') }}</TxButton>
      </template>
    </ConnectionTopBar>

    <WorkspacePage wide>
      <div class="summary-strip">
        <div>
          <strong>{{ connections.count }}</strong>
          <span>{{ t('connection.stats.total') }}</span>
        </div>
        <div>
          <strong>{{ runningCount }}</strong>
          <span>{{ t('connection.stats.running') }}</span>
        </div>
        <div>
          <strong>{{ attentionCount }}</strong>
          <span>{{ t('connection.stats.attention') }}</span>
        </div>
      </div>

      <div v-if="connections.count" class="connection-list">
        <div class="list-head">
          <span class="provider-head">{{ t('connection.head.provider') }}</span>
          <span>{{ t('connection.head.name') }}</span>
          <span>{{ t('connection.head.connectionInfo') }}</span>
          <span>{{ t('connection.head.tunnels') }}</span>
          <span>{{ t('connection.head.status') }}</span>
          <span class="actions-head" />
        </div>
        <div
          v-for="item in resources"
          :key="`${item.providerId}:${item.id}`"
          class="connection-row"
          @dblclick="openResource(item)"
        >
          <span class="provider-badge">{{ connections.providerName(item.providerId) }}</span>
          <button class="connection-name" type="button" @click="openResource(item)">
            <span>{{ item.name }}</span>
          </button>
          <span class="info-cell mono secondary">{{ resourceConnectionInfo(item) }}</span>
          <span class="tunnel-cell secondary">{{ resourceTunnelText(item) }}</span>
          <span class="row-status">
            <TxStatus :status="connections.runtimeOf(item).status" />
          </span>
          <div class="row-actions">
            <TxIconButton
              v-if="!isResourceRunning(item)"
              icon="lucide:play"
              :label="t('connection.start')"
              primary
              :action-key="resourceActionKey(item, 'start')"
              :locks="resourceLockKey(item)"
              :loading="isResourceBusy(item)"
              :disabled="isResourceBusy(item)"
              @click="connections.start(item)"
            />
            <TxIconButton
              v-else
              icon="lucide:square"
              :label="t('connection.stop')"
              danger
              :action-key="resourceActionKey(item, 'stop')"
              :locks="resourceLockKey(item)"
              :loading="isResourceBusy(item)"
              :disabled="isResourceBusy(item)"
              :success="t('connection.stopped')"
              @click="connections.stop(item)"
            />
            <div class="more-wrap">
              <TxIconButton
                icon="lucide:ellipsis"
                :label="t('connection.more')"
                :disabled="isResourceBusy(item)"
                @click.stop="toggleMoreMenu(item)"
              />
              <div
                v-if="openMoreKey === resourceKey(item)"
                class="more-menu"
                @click.stop
              >
                <button type="button" :disabled="isResourceBusy(item)" @click="duplicateResource(item)">
                  <Icon icon="lucide:copy" />
                  <span>{{ t('connection.duplicate') }}</span>
                </button>
                <button type="button" :disabled="isResourceBusy(item)" @click="renameResource(item)">
                  <Icon icon="lucide:pencil" />
                  <span>{{ t('connection.rename') }}</span>
                </button>
                <button type="button" class="danger" :disabled="isResourceBusy(item)" @click="deleteResource(item)">
                  <Icon icon="lucide:trash-2" />
                  <span>{{ t('connection.remove') }}</span>
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div v-else class="empty">
        <Icon icon="lucide:network" />
        <h3>{{ t('connection.emptyTitle') }}</h3>
        <p>{{ t('connection.emptyDesc') }}</p>
        <TxButton tone="primary" icon="lucide:plus" @click="createOpen = true">{{ t('connection.create') }}</TxButton>
      </div>
    </WorkspacePage>

    <ConnectionCreateDialog v-model="createOpen" @created="openResource" />
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import { useRouter } from 'vue-router'
import ConnectionCreateDialog from '@/components/ConnectionCreateDialog.vue'
import ConnectionTopBar from '@/components/ConnectionTopBar.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxIconButton from '@/components/ui/TxIconButton.vue'
import TxStatus from '@/components/ui/TxStatus.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { useAsyncAction } from '@/composables/useAsyncAction'
import { validateConnectionName } from '@/domain/connectionName'
import { useConnectionStore } from '@/providers/connections'
import type { TunnelResource } from '@/providers/contract'
import { providerResourceConnectionInfo, providerResourceTunnelText } from '@/providers/registry'
import { connectionPath } from '@/providers/routes'
import { useUiStore } from '@/stores/ui'

const { t } = useI18n()
const router = useRouter()
const connections = useConnectionStore()
const actions = useAsyncAction()
const ui = useUiStore()
const createOpen = ref(false)
const openMoreKey = ref<string | null>(null)
const batchResourceLock = 'provider-resource:batch-all'

const resources = computed(() => connections.resources)
const runningCount = computed(() => resources.value.filter((item) => isResourceRunning(item)).length)
const startableResources = computed(() =>
  resources.value.filter((item) => !isResourceRunning(item) && !isResourceBusy(item)),
)
const stoppableResources = computed(() =>
  resources.value.filter((item) => isResourceStoppable(item) && !isResourceBusy(item)),
)
const isStartingAll = computed(() => actions.isRunning('provider-resource:start-all'))
const isStoppingAll = computed(() => actions.isRunning('provider-resource:stop-all'))
const isBatchRunning = computed(() => actions.isBusy(batchResourceLock))
const hasStoppableResources = computed(() => stoppableResources.value.length > 0)
const batchAction = computed(() => {
  if (isStartingAll.value) {
    return {
      icon: 'lucide:play',
      tone: 'primary' as const,
      loading: true,
      disabled: true,
      label: t('connection.startAll'),
      run: startAllResources,
    }
  }
  if (isStoppingAll.value) {
    return {
      icon: 'lucide:square',
      tone: 'danger' as const,
      loading: true,
      disabled: true,
      label: t('connection.stopAll'),
      run: stopAllResources,
    }
  }
  if (hasStoppableResources.value) {
    return {
      icon: 'lucide:square',
      tone: 'danger' as const,
      loading: false,
      disabled: isBatchRunning.value || !stoppableResources.value.length,
      label: t('connection.stopAll'),
      run: stopAllResources,
    }
  }
  return {
    icon: 'lucide:play',
    tone: 'primary' as const,
    loading: false,
    disabled: isBatchRunning.value || !startableResources.value.length,
    label: t('connection.startAll'),
    run: startAllResources,
  }
})
const attentionCount = computed(() =>
  resources.value.filter((item) => {
    const status = connections.runtimeOf(item).status
    return status === 'warning' || status === 'errored'
  }).length,
)

onMounted(() => {
  void connections.init().catch(() => undefined)
  window.addEventListener('click', closeMoreMenu)
})
onBeforeUnmount(() => {
  window.removeEventListener('click', closeMoreMenu)
})

function openResource(item: TunnelResource) {
  router.push(connectionPath(item.providerId, item.id))
}

function resourceConnectionInfo(item: TunnelResource) {
  return providerResourceConnectionInfo(item, connections.runtimeOf(item)) || t('connection.notConfigured')
}

function resourceTunnelText(item: TunnelResource) {
  return providerResourceTunnelText(item, connections.runtimeOf(item))
}

function resourceLockKey(item: TunnelResource) {
  return `provider-resource:${item.providerId}:${item.id}`
}

function resourceKey(item: TunnelResource) {
  return `${item.providerId}:${item.id}`
}

function resourceActionKey(item: TunnelResource, action: string) {
  return `provider-resource:${action}:${item.providerId}:${item.id}`
}

function isResourceBusy(item: TunnelResource) {
  return actions.isBusy(resourceLockKey(item))
}

function closeMoreMenu() {
  openMoreKey.value = null
}

function toggleMoreMenu(item: TunnelResource) {
  const key = resourceKey(item)
  openMoreKey.value = openMoreKey.value === key ? null : key
}

function runBatchAction() {
  void batchAction.value.run()
}

async function duplicateResource(item: TunnelResource) {
  closeMoreMenu()
  await actions.run(
    {
      key: resourceActionKey(item, 'duplicate'),
      locks: resourceLockKey(item),
      success: t('connection.duplicated'),
    },
    () => connections.duplicate(item),
  )
}

async function renameResource(item: TunnelResource) {
  closeMoreMenu()
  const value = await ui.prompt({
    title: t('connection.rename'),
    message: t('connection.renamePrompt', { name: item.name }),
    defaultValue: item.name,
    placeholder: t('connection.renamePlaceholder'),
    confirmLabel: t('common.save'),
  })
  if (value == null) return
  const validation = validateConnectionName(value)
  if (!validation.ok) {
    ui.notify(t(validation.messageKey), 'warning')
    return
  }
  if (validation.name === item.name) return
  await actions.run(
    {
      key: resourceActionKey(item, 'rename'),
      locks: resourceLockKey(item),
      success: t('connection.renamed'),
    },
    () => connections.update({ ...item, name: validation.name }),
  )
}

async function deleteResource(item: TunnelResource) {
  closeMoreMenu()
  const ok = await ui.confirm({
    title: t('connection.remove'),
    message: t('connection.confirmRemove', { name: item.name }),
    confirmLabel: t('common.delete'),
    tone: 'danger',
  })
  if (!ok) return
  await actions.run(
    {
      key: resourceActionKey(item, 'delete'),
      locks: resourceLockKey(item),
      success: t('connection.removed'),
    },
    () => connections.delete(item),
  )
}

async function startAllResources() {
  const targets = [...startableResources.value]
  if (!targets.length) return
  await actions.run(
    {
      key: 'provider-resource:start-all',
      locks: [batchResourceLock, ...targets.map(resourceLockKey)],
      error: false,
    },
    async () => {
      const results = await Promise.allSettled(targets.map((item) => connections.start(item)))
      await Promise.all(targets.map((item) => connections.refreshResourceRuntime(item).catch(() => undefined)))
      const started = targets.filter((item, index) =>
        results[index]?.status === 'fulfilled' && isResourceStarted(item),
      ).length
      const failed = results.length - started
      if (failed) {
        ui.notify(t('connection.startAllPartial', { started, failed }), 'warning')
      } else {
        ui.notify(t('connection.startAllStarted', { count: started }), 'success')
      }
    },
  )
}

async function stopAllResources() {
  const targets = [...stoppableResources.value]
  if (!targets.length) return
  await actions.run(
    {
      key: 'provider-resource:stop-all',
      locks: [batchResourceLock, ...targets.map(resourceLockKey)],
      error: false,
    },
    async () => {
      const results = await Promise.allSettled(targets.map((item) => connections.stop(item)))
      const stopped = results.filter((result) => result.status === 'fulfilled').length
      const failed = results.length - stopped
      if (failed) {
        ui.notify(t('connection.stopAllPartial', { stopped, failed }), 'warning')
      } else {
        ui.notify(t('connection.stopAllStopped', { count: stopped }), 'success')
      }
    },
  )
}

function isResourceStoppable(item: TunnelResource) {
  const status = connections.runtimeOf(item).status
  return status === 'starting' || status === 'running' || status === 'warning'
}

function isResourceRunning(item: TunnelResource) {
  const status = connections.runtimeOf(item).status
  return status === 'starting' || status === 'running' || status === 'warning' || status === 'stopping'
}

function isResourceStarted(item: TunnelResource) {
  const status = connections.runtimeOf(item).status
  return status === 'running' || status === 'warning'
}
</script>

<style scoped>
.connections-layout {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.connections-layout :deep(.page) {
  flex: 1;
  min-height: 0;
}
.summary-strip {
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: var(--tx-bg-surface);
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  margin-bottom: 18px;
  overflow: hidden;
}
.summary-strip div {
  min-height: 62px;
  border-left: 1px solid var(--tx-border-subtle);
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 5px;
  padding: 0 16px;
}
.summary-strip div:first-child {
  border-left: 0;
}
.summary-strip strong {
  color: var(--tx-text-primary);
  font-size: 20px;
  line-height: 1;
}
.summary-strip span {
  color: var(--tx-text-secondary);
  font-size: 12px;
}
.connection-list {
  overflow: visible;
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: var(--tx-bg-surface);
}
.list-head,
.connection-row {
  display: grid;
  grid-template-columns: minmax(118px, 0.9fr) minmax(120px, 1fr) minmax(160px, 1.15fr) 72px 112px 82px;
  align-items: center;
  gap: 12px;
  min-height: 44px;
  padding: 0 14px;
}
.list-head {
  border-bottom: 1px solid var(--tx-border-subtle);
  color: var(--tx-text-muted);
  font-size: 11px;
}
.connection-row {
  border-bottom: 1px solid var(--tx-border-subtle);
  cursor: default;
}
.connection-row:last-child {
  border-bottom: 0;
}
.connection-row:hover {
  background: var(--tx-bg-hover);
}
.provider-badge {
  width: fit-content;
  max-width: 100%;
  overflow: hidden;
  border-radius: 999px;
  background: var(--tx-bg-muted);
  color: var(--tx-text-secondary);
  padding: 3px 8px;
  font-size: 11px;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.provider-head {
  padding-left: 8px;
}
.connection-name {
  min-width: 0;
  border: 0;
  background: transparent;
  color: var(--tx-text-primary);
  display: block;
  padding: 0;
  text-align: left;
  cursor: pointer;
}
.connection-name span,
.info-cell,
.tunnel-cell {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.secondary {
  color: var(--tx-text-secondary);
}
.tunnel-cell {
  text-align: left;
}
.row-actions {
  display: flex;
  justify-content: flex-start;
  gap: 6px;
  min-width: 62px;
  position: relative;
}
.more-wrap {
  position: relative;
  flex: 0 0 28px;
}
.more-menu {
  position: absolute;
  z-index: 20;
  right: 0;
  top: 34px;
  width: 132px;
  padding: 5px;
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-sm);
  background: var(--tx-bg-surface);
  box-shadow: 0 12px 28px rgba(15, 23, 42, 0.14);
}
.more-menu button {
  width: 100%;
  height: 30px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--tx-text-secondary);
  display: grid;
  grid-template-columns: 18px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  padding: 0 8px;
  font: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.more-menu button:hover:not(:disabled),
.more-menu button:focus-visible {
  background: var(--tx-bg-hover);
  color: var(--tx-text-primary);
}
.more-menu button:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}
.more-menu button.danger {
  color: var(--tx-danger);
}
.more-menu svg {
  width: 14px;
  height: 14px;
}
.empty {
  min-height: 360px;
  border: 1px dashed var(--tx-border-strong);
  border-radius: var(--tx-radius-md);
  color: var(--tx-text-secondary);
  display: grid;
  place-items: center;
  align-content: center;
  gap: 8px;
  padding: 24px;
  text-align: center;
}
.empty svg {
  width: 34px;
  height: 34px;
  color: var(--tx-text-muted);
}
.empty h3 {
  margin: 0;
  color: var(--tx-text-primary);
  font-size: 16px;
}
.empty p {
  margin: 0 0 4px;
  color: var(--tx-text-secondary);
  font-size: 13px;
}
</style>
