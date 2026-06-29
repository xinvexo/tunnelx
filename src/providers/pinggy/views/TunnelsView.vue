<template>
  <WorkspacePage wide>
    <template v-if="resource">
      <ProviderTunnelTable
        :count-label="t('providerTunnel.count', { enabled: enabledCount, total: endpoints.length })"
        :columns="tableColumns"
        :rows="tableRows"
        :create-disabled="!hasCredentials"
        :empty-description="emptyHint"
        name-width="24%"
        @create="openNew"
        @edit="openEditById"
        @toggle="toggleEndpointById"
        @remove="removeEndpointById"
        @reorder="reorderEndpoints"
      />

      <EndpointEditor v-model="editorOpen" :source="editing" @save="saveEndpoint" />
    </template>
    <div v-else class="text-13px text-[var(--tx-text-muted)]">{{ t('providerTunnel.notFound', { provider: 'Pinggy' }) }}</div>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import EndpointEditor from '../components/EndpointEditor.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import ProviderTunnelTable from '@/providers/components/ProviderTunnelTable.vue'
import { useConnectionStore } from '@/providers/connections'
import { PINGGY_PROVIDER_ID } from '@/providers/contract'
import { useUiStore } from '@/stores/ui'
import {
  applyPinggyMetadata,
  enabledEndpointCount,
  pinggyMetadata,
  type PinggyEndpoint,
  type PinggyMetadata,
} from '../domain'

const route = useRoute()
const connections = useConnectionStore()
const ui = useUiStore()
const { t } = useI18n()
const id = computed(() => String(route.params.id ?? ''))
const resource = computed(() => connections.resourceOf(PINGGY_PROVIDER_ID, id.value))
const metadata = computed(() => resource.value ? pinggyMetadata(resource.value) : null)
const endpoints = computed(() => metadata.value?.endpoints ?? [])
const enabledCount = computed(() => metadata.value ? enabledEndpointCount(metadata.value) : 0)
const hasCredentials = computed(() => !!metadata.value?.token.trim())
const credentialName = computed(() => t('providers.pinggy.token'))
const emptyHint = computed(() => hasCredentials.value ? '' : t('providerTunnel.credentialsRequired', { credential: credentialName.value }))
const editorOpen = ref(false)
const editing = ref<PinggyEndpoint | null>(null)
const savingEndpointIds = ref(new Set<string>())
const tableColumns = computed(() => [
  { key: 'local', label: t('providerTunnel.localAddress') },
])
const tableRows = computed(() => endpoints.value.map((endpoint) => ({
  id: endpoint.id,
  name: endpoint.name || '-',
  tag: endpoint.tunnelType.toUpperCase(),
  enabled: endpoint.enabled,
  enableDisabled: !hasCredentials.value && !endpoint.enabled,
  saving: savingEndpointIds.value.has(endpoint.id),
  cells: [
    { key: 'local', text: endpoint.localAddr || '-', mono: true },
  ],
})))

function endpointOf(endpointId: string) {
  return endpoints.value.find((endpoint) => endpoint.id === endpointId) ?? null
}

function openNew() {
  if (!hasCredentials.value) {
    ui.notify(t('providerTunnel.credentialsRequired', { credential: credentialName.value }), 'warning')
    return
  }
  editing.value = null
  editorOpen.value = true
}

function openEdit(endpoint: PinggyEndpoint) {
  editing.value = endpoint
  editorOpen.value = true
}

function openEditById(endpointId: string) {
  const endpoint = endpointOf(endpointId)
  if (endpoint) openEdit(endpoint)
}

async function saveMetadata(next: PinggyMetadata, successMessage?: string) {
  if (!resource.value) return
  try {
    await connections.update(applyPinggyMetadata(resource.value, next))
    if (successMessage) ui.notify(successMessage, 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
  }
}

function reorderEndpoints(ids: string[]) {
  if (!metadata.value) return
  const byId = new Map(endpoints.value.map((endpoint) => [endpoint.id, endpoint]))
  const nextEndpoints = ids.map((item) => byId.get(item)).filter((endpoint): endpoint is PinggyEndpoint => !!endpoint)
  if (nextEndpoints.length !== endpoints.value.length) return
  void saveMetadata({ ...metadata.value, endpoints: nextEndpoints })
}

function saveEndpoint(endpoint: PinggyEndpoint) {
  if (!metadata.value) return
  const nextEndpoints = metadata.value.endpoints.slice()
  const index = nextEndpoints.findIndex((item) => item.id === endpoint.id)
  if (index >= 0) nextEndpoints[index] = endpoint
  else nextEndpoints.push(endpoint)
  void saveMetadata({ ...metadata.value, endpoints: nextEndpoints }, t('providerTunnel.saved', { provider: 'Pinggy' }))
}

async function toggleEndpoint(endpoint: PinggyEndpoint, enabled: boolean) {
  if (!metadata.value) return
  if (enabled && !hasCredentials.value) {
    ui.notify(t('providerTunnel.credentialsRequired', { credential: credentialName.value }), 'warning')
    return
  }
  const next = {
    ...metadata.value,
    endpoints: metadata.value.endpoints.map((item) => item.id === endpoint.id ? { ...item, enabled } : item),
  }
  const nextSaving = new Set(savingEndpointIds.value)
  nextSaving.add(endpoint.id)
  savingEndpointIds.value = nextSaving
  try {
    await saveMetadata(next)
  } finally {
    const done = new Set(savingEndpointIds.value)
    done.delete(endpoint.id)
    savingEndpointIds.value = done
  }
}

async function toggleEndpointById(endpointId: string, enabled: boolean) {
  const endpoint = endpointOf(endpointId)
  if (endpoint) await toggleEndpoint(endpoint, enabled)
}

async function removeEndpoint(endpoint: PinggyEndpoint) {
  if (!metadata.value) return
  const ok = await ui.confirm({
    title: t('providerTunnel.deleteTitle'),
    message: t('providerTunnel.deleteMessage', { name: endpoint.name || t('providerTunnel.thisTunnel') }),
    confirmLabel: t('common.delete'),
    tone: 'danger',
  })
  if (!ok) return
  void saveMetadata({
    ...metadata.value,
    endpoints: metadata.value.endpoints.filter((item) => item.id !== endpoint.id),
  }, t('providerTunnel.removed', { provider: 'Pinggy' }))
}

async function removeEndpointById(endpointId: string) {
  const endpoint = endpointOf(endpointId)
  if (endpoint) await removeEndpoint(endpoint)
}
</script>
