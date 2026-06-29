<template>
  <TxDrawer
    :model-value="modelValue"
    :title="source ? t('cloudflare.ingress.edit') : t('cloudflare.ingress.create')"
    :close-label="t('common.close')"
    @update:model-value="close"
  >
    <SettingsGroup v-if="draft" :title="t('cloudflare.ingress.basic')">
      <SettingsRow :label="t('providerForm.name')">
        <input v-model="draft.name" class="tx-input mono" />
      </SettingsRow>
      <SettingsRow :label="t('cloudflare.ingress.zone')" :desc="t('cloudflare.ingress.zoneDesc')">
        <TxSelect v-model="selectedZoneName" :options="zoneOptions" :aria-label="t('cloudflare.ingress.zone')" />
      </SettingsRow>
      <SettingsRow :label="t('cloudflare.ingress.subdomain')" :desc="t('cloudflare.ingress.subdomainDesc')">
        <div class="grid min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-8px">
          <input v-model="hostnamePrefix" class="tx-input mono" placeholder="app" />
          <span class="text-12px text-[var(--tx-text-secondary)] whitespace-nowrap">.{{ selectedZoneName }}</span>
        </div>
      </SettingsRow>
      <SettingsRow label="Hostname">
        <code class="mono block min-w-0 truncate text-[var(--tx-text-secondary)]">{{ composedHostname || '-' }}</code>
      </SettingsRow>
      <SettingsRow :label="t('cloudflare.ingress.service')" :desc="t('cloudflare.ingress.serviceDesc')">
        <input v-model="draft.service" class="tx-input mono" placeholder="http://localhost:8080" />
      </SettingsRow>
    </SettingsGroup>

    <template #footer>
      <TxButton @click="close(false)">{{ t('common.cancel') }}</TxButton>
      <TxButton tone="primary" @click="submit">{{ t('common.save') }}</TxButton>
    </template>
  </TxDrawer>
</template>

<script setup lang="ts">
import { computed, ref, toRaw, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxDrawer from '@/components/ui/TxDrawer.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import { clone } from '@/domain/util'
import { useUiStore } from '@/stores/ui'
import type { CloudflareIngressRule, CloudflareZone } from '../domain'
import { newIngressRule } from '../domain'

const props = defineProps<{ modelValue: boolean; source: CloudflareIngressRule | null; zones: CloudflareZone[] }>()
const emit = defineEmits<{ 'update:modelValue': [boolean]; save: [CloudflareIngressRule] }>()
const ui = useUiStore()
const { t } = useI18n()
const draft = ref<CloudflareIngressRule | null>(null)
const selectedZoneName = ref('')
const hostnamePrefix = ref('')
const zoneOptions = computed(() => props.zones.map((zone) => ({ label: zone.name, value: zone.name })))
const composedHostname = computed(() => composeHostname(hostnamePrefix.value, selectedZoneName.value))

watch(() => props.modelValue, (open) => {
  if (!open) return
  draft.value = props.source ? clone(props.source) : newIngressRule()
  const host = draft.value.hostname.trim()
  const zone = findZoneForHostname(host) ?? props.zones[0]
  selectedZoneName.value = zone?.name ?? ''
  hostnamePrefix.value = zone ? prefixForHostname(host, zone.name) : ''
})

function close(value: boolean) {
  emit('update:modelValue', value)
}

function submit() {
  if (!draft.value) return
  draft.value.name = draft.value.name.trim()
  if (!selectedZoneName.value) {
    ui.notify(t('cloudflare.ingress.needZones'), 'warning')
    return
  }
  const hostname = composedHostname.value
  if (!draft.value.name || !hostname || !draft.value.service.trim()) {
    ui.notify(t('cloudflare.ingress.required'), 'warning')
    return
  }
  if (!isValidPrefix(hostnamePrefix.value)) {
    ui.notify(t('cloudflare.ingress.invalidSubdomain'), 'warning')
    return
  }
  draft.value.hostname = hostname
  draft.value.service = draft.value.service.trim()
  emit('save', toRaw(draft.value))
  close(false)
}

function normalizeHostname(value: string) {
  return value.trim().replace(/^\.+|\.+$/g, '').toLowerCase()
}

function findZoneForHostname(hostname: string) {
  const host = normalizeHostname(hostname)
  if (!host) return null
  return props.zones
    .slice()
    .sort((a, b) => b.name.length - a.name.length)
    .find((zone) => {
      const name = normalizeHostname(zone.name)
      return host === name || host.endsWith(`.${name}`)
    }) ?? null
}

function prefixForHostname(hostname: string, zone: string) {
  const host = normalizeHostname(hostname)
  const name = normalizeHostname(zone)
  if (!host || !name || host === name) return ''
  return host.endsWith(`.${name}`) ? host.slice(0, -(name.length + 1)) : ''
}

function composeHostname(prefix: string, zone: string) {
  const cleanZone = normalizeHostname(zone)
  if (!cleanZone) return ''
  const cleanPrefix = normalizeHostname(prefix)
  if (!cleanPrefix || cleanPrefix === '@') return cleanZone
  return `${cleanPrefix}.${cleanZone}`
}

function isValidPrefix(prefix: string) {
  const clean = normalizeHostname(prefix)
  if (!clean || clean === '@') return true
  return clean.split('.').every((part) =>
    part === '*'
      || /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/.test(part),
  )
}
</script>
