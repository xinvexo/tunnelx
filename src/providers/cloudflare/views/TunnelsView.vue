<template>
  <WorkspacePage wide>
    <template v-if="draft">
      <ProviderTunnelTable
        :count-label="t('cloudflare.tunnels.count', { enabled: enabledIngressCount, total: draft.ingress.length })"
        :columns="tableColumns"
        :rows="tableRows"
        :create-loading="zonesLoading"
        :create-disabled="!canCreateIngress"
        :empty-description="emptyHint"
        :create-label="t('cloudflare.tunnels.create')"
        name-width="18%"
        :empty-title="t('cloudflare.tunnels.empty')"
        :edit-label="t('cloudflare.tunnels.edit')"
        :remove-label="t('cloudflare.tunnels.remove')"
        @create="openNewIngress"
        @edit="openEditIngressById"
        @toggle="toggleIngressById"
        @remove="removeIngress"
        @reorder="reorderIngress"
      />

      <CloudflareIngressEditor
        v-model="editorOpen"
        :source="editingIngress"
        :zones="zones"
        @save="saveIngress"
      />
    </template>
    <div v-else class="text-13px text-[var(--tx-text-muted)]">{{ t('cloudflare.notFound') }}</div>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import CloudflareIngressEditor from '../components/IngressEditor.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import ProviderTunnelTable from '@/providers/components/ProviderTunnelTable.vue'
import { useConnectionStore } from '@/providers/connections'
import { clone } from '@/domain/util'
import type { CloudflareIngressRule, CloudflareTunnel } from '../domain'
import { useCloudflareStore } from '../stores'
import { useUiStore } from '@/stores/ui'
import { CLOUDFLARE_PROVIDER_ID } from '@/providers/contract'
import { serviceTag } from '@/providers/tunnelItems'

const route = useRoute()
const cloudflare = useCloudflareStore()
const connections = useConnectionStore()
const ui = useUiStore()
const { t } = useI18n()
const id = computed(() => String(route.params.id ?? ''))
const draft = ref<CloudflareTunnel | null>(null)
const saving = ref(false)
const savingIngressIds = ref(new Set<string>())
const editorOpen = ref(false)
const editingIngress = ref<CloudflareIngressRule | null>(null)
const zonesLoading = ref(false)
const zoneError = ref('')
const zones = computed(() => cloudflare.zones)
const canCreateIngress = computed(() => !zonesLoading.value && zones.value.length > 0)
const enabledIngressCount = computed(() => draft.value?.ingress.filter((rule) => rule.enabled).length ?? 0)
const emptyHint = computed(() => {
  if (zonesLoading.value) return t('cloudflare.tunnels.loadingZones')
  if (zoneError.value) return zoneError.value
  if (!zones.value.length) return t('cloudflare.tunnels.needAuthorization')
  return ''
})
const tableColumns = [
  { key: 'hostname', label: t('providerTunnel.hostname'), width: '25%' },
  { key: 'service', label: 'Service', width: '30%' },
  { key: 'dns', label: 'DNS', width: '14%' },
]
const tableRows = computed(() => (draft.value?.ingress ?? []).map((rule) => ({
  id: rule.id,
  name: rule.name || '-',
  tag: rule.enabled ? serviceTag(rule.service) : 'OFF',
  enabled: rule.enabled,
  saving: savingIngressIds.value.has(rule.id),
  cells: [
    { key: 'hostname', text: rule.hostname || '-', mono: true },
    { key: 'service', text: rule.service || '-', mono: true },
    {
      key: 'dns',
      text: rule.enabled
        ? (rule.dnsRouted ? t('cloudflare.tunnels.dnsReady') : t('cloudflare.tunnels.dnsOnStart'))
        : t('cloudflare.tunnels.disabled'),
      pill: true,
      tone: rule.enabled ? (rule.dnsRouted ? 'success' as const : 'default' as const) : 'muted' as const,
    },
  ],
})))

watch(id, async () => {
  await hydrate()
}, { immediate: true })

async function hydrate() {
  await cloudflare.init()
  const tunnel = cloudflare.tunnels.find((item) => item.id === id.value)
  draft.value = tunnel ? clone(tunnel) : null
  cloudflare.zones = []
  await ensureZones(false)
}

async function save(options: { notify?: boolean } = {}) {
  const notify = options.notify ?? true
  if (!draft.value) return
  normalizeDraftName()
  saving.value = true
  try {
    const saved = await cloudflare.saveTunnel(draft.value)
    draft.value = clone(saved)
    await connections.refreshProvider(CLOUDFLARE_PROVIDER_ID)
    if (notify) ui.notify(t('cloudflare.tunnels.saved'), 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
    await hydrate()
  } finally {
    saving.value = false
  }
}

function normalizeDraftName() {
  if (!draft.value) return
  draft.value.name = draft.value.name.trim() || 'cloudflare'
}

async function openNewIngress() {
  if (!await ensureZones(true)) return
  editingIngress.value = null
  editorOpen.value = true
}

async function openEditIngress(rule: CloudflareIngressRule) {
  if (!await ensureZones(true)) return
  editingIngress.value = rule
  editorOpen.value = true
}

async function openEditIngressById(ruleId: string) {
  const rule = draft.value?.ingress.find((item) => item.id === ruleId)
  if (rule) await openEditIngress(rule)
}

function saveIngress(rule: CloudflareIngressRule) {
  if (!draft.value) return
  const next = draft.value.ingress.slice()
  const index = next.findIndex((item) => item.id === rule.id)
  if (index >= 0) {
    const previous = next[index]
    next[index] = {
      ...rule,
      dnsRouted: rule.enabled && previous.hostname.trim() === rule.hostname.trim() && rule.dnsRouted,
    }
  } else {
    next.push(rule)
  }
  draft.value.ingress = next
  void save()
}

async function toggleIngress(rule: CloudflareIngressRule, enabled: boolean) {
  if (!draft.value) return
  const nextSaving = new Set(savingIngressIds.value)
  nextSaving.add(rule.id)
  savingIngressIds.value = nextSaving
  try {
    draft.value.ingress = draft.value.ingress.map((item) =>
      item.id === rule.id ? { ...item, enabled, dnsRouted: enabled && item.dnsRouted } : item,
    )
    await save({ notify: false })
  } finally {
    const done = new Set(savingIngressIds.value)
    done.delete(rule.id)
    savingIngressIds.value = done
  }
}

async function toggleIngressById(ruleId: string, enabled: boolean) {
  const rule = draft.value?.ingress.find((item) => item.id === ruleId)
  if (rule) await toggleIngress(rule, enabled)
}

function reorderIngress(ids: string[]) {
  if (!draft.value) return
  const byId = new Map(draft.value.ingress.map((rule) => [rule.id, rule]))
  const next = ids.map((item) => byId.get(item)).filter((rule): rule is CloudflareIngressRule => !!rule)
  if (next.length !== draft.value.ingress.length) return
  draft.value.ingress = next
  void save({ notify: false })
}

async function removeIngress(ruleId: string) {
  if (!draft.value) return
  const rule = draft.value.ingress.find((item) => item.id === ruleId)
  const ok = await ui.confirm({
    title: t('cloudflare.tunnels.deleteTitle'),
    message: t('cloudflare.tunnels.deleteMessage', { name: rule?.name || t('cloudflare.tunnels.thisIngress') }),
    confirmLabel: t('common.delete'),
    tone: 'danger',
  })
  if (!ok) return
  draft.value.ingress = draft.value.ingress.filter((rule) => rule.id !== ruleId)
  void save()
}

async function ensureZones(showToast: boolean) {
  if (zones.value.length) {
    zoneError.value = ''
    return true
  }
  const current = draft.value
  if (!current) return false
  if (current.account.authMode === 'authorization' && !current.certFile.trim()) {
    zoneError.value = t('cloudflare.tunnels.needAuthorizationCurrent')
    if (showToast) ui.notify(zoneError.value, 'warning')
    return false
  }
  if (current.account.authMode === 'apiToken'
    && (!current.account.apiToken.trim() || !current.account.tokenAccountId.trim())) {
    zoneError.value = t('cloudflare.tunnels.needToken')
    if (showToast) ui.notify(zoneError.value, 'warning')
    return false
  }
  zonesLoading.value = true
  try {
    await cloudflare.refreshZones(current.id)
    zoneError.value = zones.value.length
      ? ''
      : t('cloudflare.tunnels.noZones')
    if (showToast && zoneError.value) ui.notify(zoneError.value, 'warning')
    return zones.value.length > 0
  } catch (error) {
    zoneError.value = String(error)
    if (showToast) ui.notify(zoneError.value, 'danger')
    return false
  } finally {
    zonesLoading.value = false
  }
}
</script>
