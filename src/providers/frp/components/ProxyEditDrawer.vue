<template>
  <TxDrawer
    :model-value="modelValue"
    :title="isNew ? t('proxyEditor.titleNew') : t('proxyEditor.titleEdit')"
    :description="t('proxyEditor.description')"
    :close-label="t('proxyEditor.close')"
    width="lg"
    @update:model-value="close"
  >
    <template v-if="draft && cfg">
      <SettingsGroup :title="t('proxyEditor.basic')">
            <SettingsRow :label="t('proxyEditor.name')" :desc="t('proxyEditor.nameDesc')">
              <input v-model="cfg.name" class="tx-input" :placeholder="t('proxyEditor.namePlaceholder')" />
            </SettingsRow>
            <SettingsRow :label="t('proxyEditor.type')">
              <TxSelect
                :model-value="cfg.type"
                :aria-label="t('proxyEditor.typeAria')"
                :options="proxyTypeOptions"
                @update:model-value="changeType($event as ProxyType)"
              />
            </SettingsRow>
            <SettingsRow :label="t('proxyEditor.enable')"><TxSwitch v-model="draft.enabled" /></SettingsRow>
          </SettingsGroup>

          <SettingsGroup :title="t('proxyEditor.localTarget')">
            <SettingsRow :label="t('proxyEditor.usePlugin')" :desc="t('proxyEditor.usePluginDesc')"><TxSwitch v-model="pluginOn" /></SettingsRow>
            <template v-if="!pluginOn">
              <SettingsRow :label="t('proxyEditor.localIP')"><input v-model="cfg.localIP" class="tx-input mono" placeholder="127.0.0.1" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.localPort')"><NumberField v-model="cfg.localPort" :min="1" :max="65535" zero-as-null /></SettingsRow>
            </template>
            <template v-else>
              <SettingsRow :label="t('proxyEditor.pluginType')">
                <TxSelect v-model="pluginType" :aria-label="t('proxyEditor.pluginTypeAria')" :options="pluginOptions" />
              </SettingsRow>
              <PluginEditor v-if="cfg.plugin" :model-value="cfg.plugin" />
            </template>
          </SettingsGroup>

          <SettingsGroup :title="t('proxyEditor.entrySuffix', { type: typeLabel })" :note="entryNote">
            <template v-if="cfg.type === 'tcp' || cfg.type === 'udp'">
              <SettingsRow :label="t('proxyEditor.remotePort')" :desc="t('proxyEditor.remotePortDesc')">
                <NumberField v-model="cfg.remotePort" :min="1" :max="65535" zero-as-null />
              </SettingsRow>
            </template>
            <template v-else-if="cfg.type === 'http'">
              <SettingsRow :label="t('proxyEditor.domainMode')">
                <TxSelect v-model="domainMode" :aria-label="t('proxyEditor.domainModeAria')" :options="domainModeOptions" />
              </SettingsRow>
              <SettingsRow v-if="showCustomDomains" :label="t('proxyEditor.customDomains')" :desc="t('proxyEditor.customDomainsDesc')" vertical>
                <StringListEditor v-model="cfg.customDomains" placeholder="a.example.com" />
              </SettingsRow>
              <SettingsRow v-if="showSubdomain" :label="t('proxyEditor.subdomain')" :desc="t('proxyEditor.subdomainDesc')">
                <input v-model="cfg.subdomain" class="tx-input" placeholder="app" />
              </SettingsRow>
              <SettingsRow :label="t('proxyEditor.locations')"><StringListEditor v-model="cfg.locations" placeholder="/api" compact /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.httpUser')"><input v-model="cfg.httpUser" class="tx-input" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.httpPassword')"><input v-model="cfg.httpPassword" class="tx-input" type="password" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.hostRewrite')"><input v-model="cfg.hostHeaderRewrite" class="tx-input" /></SettingsRow>
            </template>
            <template v-else-if="cfg.type === 'https' || cfg.type === 'tcpmux'">
              <SettingsRow :label="t('proxyEditor.domainMode')">
                <TxSelect v-model="domainMode" :aria-label="t('proxyEditor.domainModeAria')" :options="domainModeOptions" />
              </SettingsRow>
              <SettingsRow v-if="showCustomDomains" :label="t('proxyEditor.customDomains')" :desc="t('proxyEditor.customDomainsDesc')" vertical>
                <StringListEditor v-model="cfg.customDomains" placeholder="a.example.com" />
              </SettingsRow>
              <SettingsRow v-if="showSubdomain" :label="t('proxyEditor.subdomain')" :desc="t('proxyEditor.subdomainDesc')">
                <input v-model="cfg.subdomain" class="tx-input" placeholder="app" />
              </SettingsRow>
            </template>
            <template v-else>
              <SettingsRow :label="t('proxyEditor.secretKey')"><input v-model="cfg.secretKey" class="tx-input" type="password" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.allowUsers')" vertical><StringListEditor v-model="cfg.allowUsers" placeholder="*" /></SettingsRow>
            </template>
          </SettingsGroup>

          <AdvancedToggle class="-mt-4px mb-14px" :open="showAdvanced" @click="showAdvanced = !showAdvanced" />

          <template v-if="showAdvanced">
            <SettingsGroup :title="t('proxyEditor.transport')" :columns="2">
              <SettingsRow :label="t('proxyEditor.encryption')" short><TxSwitch v-model="cfg.transport.useEncryption" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.compression')" short><TxSwitch v-model="cfg.transport.useCompression" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.bandwidthLimit')" :desc="t('proxyEditor.bandwidthLimitDesc')" wide><input v-model="cfg.transport.bandwidthLimit" class="tx-input mono" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.bandwidthSide')">
                <TxSelect v-model="cfg.transport.bandwidthLimitMode" :aria-label="t('proxyEditor.bandwidthSideAria')" :options="bandwidthModeOptions" />
              </SettingsRow>
              <SettingsRow label="Proxy Protocol">
                <TxSelect v-model="cfg.transport.proxyProtocolVersion" :aria-label="t('proxyEditor.proxyProtocolAria')" :options="proxyProtocolOptions" />
              </SettingsRow>
            </SettingsGroup>
            <SettingsGroup :title="t('proxyEditor.healthCheck')" :columns="2">
              <SettingsRow :label="t('proxyEditor.healthCheckType')" wide>
                <TxSelect v-model="cfg.healthCheck.type" :aria-label="t('proxyEditor.healthCheckTypeAria')" :options="healthCheckOptions" />
              </SettingsRow>
              <template v-if="cfg.healthCheck.type">
                <SettingsRow :label="t('proxyEditor.timeoutSeconds')"><NumberField v-model="cfg.healthCheck.timeoutSeconds" :min="0" /></SettingsRow>
                <SettingsRow :label="t('proxyEditor.maxFailed')"><NumberField v-model="cfg.healthCheck.maxFailed" :min="0" /></SettingsRow>
                <SettingsRow :label="t('proxyEditor.intervalSeconds')" :wide="cfg.healthCheck.type !== 'http'"><NumberField v-model="cfg.healthCheck.intervalSeconds" :min="0" /></SettingsRow>
              </template>
              <SettingsRow v-if="cfg.healthCheck.type === 'http'" :label="t('proxyEditor.path')"><input v-model="cfg.healthCheck.path" class="tx-input mono" /></SettingsRow>
              <SettingsRow v-if="cfg.healthCheck.type === 'http'" :label="t('proxyEditor.httpHeaders')" vertical wide>
                <KeyValueEditor v-model="healthCheckHeaders" />
              </SettingsRow>
            </SettingsGroup>
            <SettingsGroup :title="t('proxyEditor.loadBalancer')" :columns="2">
              <SettingsRow :label="t('proxyEditor.group')"><input v-model="cfg.loadBalancer.group" class="tx-input" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.groupKey')"><input v-model="cfg.loadBalancer.groupKey" class="tx-input" type="password" /></SettingsRow>
            </SettingsGroup>
            <SettingsGroup :title="t('proxyEditor.metadata')">
              <SettingsRow :label="t('proxyEditor.metadataLabel')" vertical><KeyValueEditor v-model="cfg.metadatas" /></SettingsRow>
              <SettingsRow :label="t('proxyEditor.annotations')" vertical><KeyValueEditor v-model="cfg.annotations" /></SettingsRow>
            </SettingsGroup>
          </template>
    </template>

    <template #footer>
      <TxButton @click="close(false)">{{ t('proxyEditor.cancel') }}</TxButton>
      <TxButton tone="primary" @click="submit">{{ t('proxyEditor.save') }}</TxButton>
    </template>
  </TxDrawer>
</template>

<script setup lang="ts">
import { computed, ref, toRaw, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import PluginEditor from '@/providers/frp/components/PluginEditor.vue'
import AdvancedToggle from '@/components/ui/AdvancedToggle.vue'
import KeyValueEditor from '@/components/ui/KeyValueEditor.vue'
import NumberField from '@/components/ui/NumberField.vue'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import StringListEditor from '@/components/ui/StringListEditor.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxDrawer from '@/components/ui/TxDrawer.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import TxSwitch from '@/components/ui/TxSwitch.vue'
import { PLUGIN_TYPES, PROXY_TYPES, localizeOptions } from '@/providers/frp/domain/options'
import { newPlugin, type PluginType } from '@/providers/frp/domain/plugin'
import { hydrateProfileProxy, newProfileProxy, newProxyConfig, type ProfileProxy, type ProxyType } from '@/providers/frp/domain/proxy'
import { clone } from '@/domain/util'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ modelValue: boolean; source: ProfileProxy | null }>()
const emit = defineEmits<{ 'update:modelValue': [boolean]; save: [ProfileProxy] }>()
const { t } = useI18n()
const ui = useUiStore()
const draft = ref<ProfileProxy | null>(null)
const showAdvanced = ref(false)
type DomainMode = 'custom' | 'subdomain' | 'both'
const domainModeValue = ref<DomainMode>('custom')
const cfg = computed<any>(() => draft.value?.config)
const isNew = computed(() => !props.source)
const proxyTypeOptions = computed(() => localizeOptions(PROXY_TYPES, t))
const typeLabel = computed(() => proxyTypeOptions.value.find((item) => item.value === cfg.value?.type)?.label ?? '')
const entryNote = computed(() =>
  ['http', 'https', 'tcpmux'].includes(cfg.value?.type)
    ? t('proxyEditor.entryNote')
    : '',
)
const domainModeOptions = computed(() => [
  { label: t('proxyEditor.domain.custom'), value: 'custom' },
  { label: t('proxyEditor.domain.subdomain'), value: 'subdomain' },
  { label: t('proxyEditor.domain.both'), value: 'both' },
])
const showCustomDomains = computed(() => domainModeValue.value !== 'subdomain')
const showSubdomain = computed(() => domainModeValue.value !== 'custom')
const domainMode = computed({
  get: () => domainModeValue.value,
  set: (value: string) => {
    const next = value as DomainMode
    domainModeValue.value = next
    if (!cfg.value) return
    if (next === 'custom') cfg.value.subdomain = ''
    if (next === 'subdomain') cfg.value.customDomains = []
  },
})
const pluginOptions = computed(() =>
  localizeOptions(PLUGIN_TYPES.filter((item) => item.value), t),
)
const bandwidthModeOptions = computed(() => [
  { label: t('proxyEditor.bandwidthMode.default'), value: '' },
  { label: t('proxyEditor.bandwidthMode.client'), value: 'client' },
  { label: t('proxyEditor.bandwidthMode.server'), value: 'server' },
])
const proxyProtocolOptions = computed(() => [
  { label: t('proxyEditor.proxyProtocol.none'), value: '' },
  { label: 'v1', value: 'v1' },
  { label: 'v2', value: 'v2' },
])
const healthCheckOptions = computed(() => [
  { label: t('proxyEditor.health.off'), value: '' },
  { label: 'TCP', value: 'tcp' },
  { label: 'HTTP', value: 'http' },
])
const healthCheckHeaders = computed<Record<string, string> | null>({
  get: () => {
    const headers = cfg.value?.healthCheck?.httpHeaders ?? []
    const result: Record<string, string> = {}
    for (const header of headers) {
      if (header.name?.trim()) result[header.name.trim()] = header.value ?? ''
    }
    return Object.keys(result).length ? result : null
  },
  set: (value) => {
    if (!cfg.value) return
    cfg.value.healthCheck.httpHeaders = Object.entries(value ?? {}).map(([name, headerValue]) => ({
      name,
      value: headerValue,
    }))
  },
})

watch(() => props.modelValue, (open) => {
  if (!open) return
  draft.value = props.source ? hydrateProfileProxy(clone(props.source)) : newProfileProxy('tcp')
  domainModeValue.value = detectDomainMode(draft.value.config)
  showAdvanced.value = false
})

const pluginOn = computed({
  get: () => !!cfg.value?.plugin,
  set: (value: boolean) => { if (cfg.value) cfg.value.plugin = value ? newPlugin('socks5') : null },
})
const pluginType = computed({
  get: () => cfg.value?.plugin?.type ?? 'socks5',
  set: (value: string) => { if (cfg.value) cfg.value.plugin = newPlugin(value as PluginType) },
})

function changeType(type: ProxyType) {
  if (!draft.value) return
  const old: any = draft.value.config
  const next: any = newProxyConfig(type)
  for (const key of ['name', 'localIP', 'plugin', 'transport', 'healthCheck', 'loadBalancer', 'metadatas', 'annotations']) {
    next[key] = old[key]
  }
  // 用户已填过本地端口就保留；没填过则用新类型的预填值（http 80 / https 443）。
  if (old.localPort != null) next.localPort = old.localPort
  draft.value.config = next
  domainModeValue.value = detectDomainMode(next)
}
function detectDomainMode(config: any): DomainMode {
  const hasCustomDomains = Array.isArray(config.customDomains) && config.customDomains.length > 0
  const hasSubdomain = !!config.subdomain?.trim()
  if (hasCustomDomains && hasSubdomain) return 'both'
  if (hasSubdomain) return 'subdomain'
  return 'custom'
}
function close(value: boolean) { emit('update:modelValue', value) }
function submit() {
  if (!draft.value || !cfg.value.name.trim()) {
    ui.notify(t('proxyEditor.nameRequired'), 'warning')
    return
  }
  if (
    ['http', 'https', 'tcpmux'].includes(cfg.value.type)
    && (
      (showCustomDomains.value && !cfg.value.customDomains.length)
      || (showSubdomain.value && !cfg.value.subdomain.trim())
    )
  ) {
    ui.notify(domainModeValue.value === 'both' ? t('proxyEditor.domainBothRequired') : t('proxyEditor.domainRequired'), 'warning')
    return
  }
  emit('save', toRaw(draft.value))
  close(false)
}
</script>
