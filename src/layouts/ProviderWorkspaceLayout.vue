<template>
  <div class="layout">
    <ConnectionTopBar
      :title="resource?.name ?? connections.providerName(providerId)"
      :subtitle="connections.providerName(providerId)"
      :id="id"
      :base-path="basePath"
    >
      <template #actions>
        <TxStatus :status="runtimeInfo.status" />
        <TxIconButton
          v-if="!isRunning"
          icon="lucide:play"
          :label="t('layout.start')"
          primary
          :action-key="actionKey('start')"
          :locks="lockKey"
          :loading="busy"
          :disabled="!resource"
          @click="start"
        />
        <TxIconButton
          v-else
          icon="lucide:square"
          :label="t('layout.stop')"
          danger
          :action-key="actionKey('stop')"
          :locks="lockKey"
          :loading="busy"
          :success="t('layout.stopped')"
          :disabled="!resource"
          @click="stop"
        />
      </template>
    </ConnectionTopBar>
    <div class="page"><router-view /></div>
  </div>
</template>

<script setup lang="ts">
import { computed, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import ConnectionTopBar from '@/components/ConnectionTopBar.vue'
import TxIconButton from '@/components/ui/TxIconButton.vue'
import TxStatus from '@/components/ui/TxStatus.vue'
import { useAsyncAction } from '@/composables/useAsyncAction'
import { useConnectionStore } from '@/providers/connections'
import { hydrateProviderConnection } from '@/providers/registry'
import { connectionPath } from '@/providers/routes'
import { useUiStore } from '@/stores/ui'

const route = useRoute()
const { t } = useI18n()
const connections = useConnectionStore()
const ui = useUiStore()
const actions = useAsyncAction()

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
const isRunning = computed(() => {
  const status = runtimeInfo.value.status
  return status === 'starting' || status === 'running' || status === 'warning' || status === 'stopping'
})
const lockKey = computed(() => `provider:${providerId.value}:${id.value}`)
const busy = computed(() => actions.isBusy(lockKey.value))
const basePath = computed(() => connectionPath(providerId.value, id.value, '').replace(/\/$/, ''))

watch([providerId, id], async () => {
  await hydrate()
}, { immediate: true })

async function hydrate() {
  if (!providerId.value || !id.value) return
  try {
    await connections.init()
    await connections.refreshProvider(providerId.value)
    const current = connections.resourceOf(providerId.value, id.value)
    if (current) await connections.refreshResourceRuntime(current).catch(() => undefined)
    if (current) await hydrateProviderConnection(current)
  } catch (error) {
    ui.notify(String(error), 'danger')
  }
}

async function start() {
  if (!resource.value) return
  await connections.start(resource.value)
}

async function stop() {
  if (!resource.value) return
  await connections.stop(resource.value)
}

function actionKey(action: string) {
  return `provider:${action}:${providerId.value}:${id.value}`
}
</script>

<style scoped>
.layout {
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.page {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
</style>
