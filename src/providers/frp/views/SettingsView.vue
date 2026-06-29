<template>
  <WorkspacePage wide>
    <template v-if="form">
      <div class="page-toolbar">
        <span>{{ t('serverSettings.toolbarHint') }}</span>
        <div>
          <TxButton :disabled="!form" @click="reset">{{ t('serverSettings.reset') }}</TxButton>
          <TxButton tone="primary" :loading="saving" :disabled="!form" @click="save">{{ t('serverSettings.save') }}</TxButton>
        </div>
      </div>
      <div class="settings-grid">
        <SettingsGroup :title="t('serverSettings.groups.connection')">
          <SettingsRow :label="t('serverSettings.name')"><input v-model="form.name" class="tx-input" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.serverAddr')"><input v-model="form.server.serverAddr" class="tx-input mono" placeholder="frps.example.com" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.serverPort')"><NumberField v-model="form.server.serverPort" :min="1" :max="65535" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.frpcVersion')" :desc="t('serverSettings.frpcVersionDesc')">
            <TxSelect v-model="versionModel" :aria-label="t('serverSettings.frpcVersion')" :options="versionOptions" />
          </SettingsRow>
        </SettingsGroup>

        <SettingsGroup :title="t('serverSettings.groups.auth')">
          <SettingsRow :label="t('serverSettings.authMethod')">
            <TxSegmentedControl v-model="form.server.auth.method" :options="authMethodOptions" />
          </SettingsRow>
          <SettingsRow v-if="form.server.auth.method === 'token'" label="Token">
            <SecretInput v-model="form.server.auth.token" />
          </SettingsRow>
          <template v-else>
            <SettingsRow label="Client ID"><input v-model="form.server.auth.oidc.clientID" class="tx-input" /></SettingsRow>
            <SettingsRow label="Client Secret"><input v-model="form.server.auth.oidc.clientSecret" class="tx-input" type="password" /></SettingsRow>
            <SettingsRow label="Audience"><input v-model="form.server.auth.oidc.audience" class="tx-input" /></SettingsRow>
            <SettingsRow label="Scope"><input v-model="form.server.auth.oidc.scope" class="tx-input" /></SettingsRow>
            <SettingsRow label="Token Endpoint"><input v-model="form.server.auth.oidc.tokenEndpointURL" class="tx-input mono" /></SettingsRow>
            <SettingsRow :label="t('serverSettings.skipVerify')"><TxSwitch v-model="form.server.auth.oidc.insecureSkipVerify" /></SettingsRow>
          </template>
        </SettingsGroup>

        <SettingsGroup :title="t('serverSettings.groups.transport')">
          <SettingsRow :label="t('serverSettings.protocol')">
            <TxSelect v-model="form.server.transport.protocol" :aria-label="t('serverSettings.protocolAria')" :options="PROTOCOLS" />
          </SettingsRow>
          <SettingsRow :label="t('serverSettings.tcpMux')" short><TriStateSwitch v-model="form.server.transport.tcpMux" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.poolCount')"><NumberField v-model="form.server.transport.poolCount" :min="0" /></SettingsRow>
          <SettingsRow label="TLS" short><TriStateSwitch v-model="form.server.transport.tls.enable" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.proxyUrl')" :desc="t('serverSettings.proxyUrlDesc')" wide>
            <input v-model="form.server.transport.proxyURL" class="tx-input mono" placeholder="http://127.0.0.1:8080" />
          </SettingsRow>
        </SettingsGroup>
      </div>

      <AdvancedToggle class="advanced-toggle" :open="showAdvanced" @click="showAdvanced = !showAdvanced" />

      <div v-if="showAdvanced" class="settings-grid advanced-grid">
        <SettingsGroup :title="t('serverSettings.groups.connParams')">
          <SettingsRow :label="t('serverSettings.dialTimeout')"><NumberField v-model="form.server.transport.dialServerTimeout" :min="0" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.keepalive')"><NumberField v-model="form.server.transport.dialServerKeepalive" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.heartbeatInterval')"><NumberField v-model="form.server.transport.heartbeatInterval" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.heartbeatTimeout')"><NumberField v-model="form.server.transport.heartbeatTimeout" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.localBindIP')" wide><input v-model="form.server.transport.connectServerLocalIP" class="tx-input mono" /></SettingsRow>
        </SettingsGroup>
        <SettingsGroup :title="t('serverSettings.groups.tlsFiles')">
          <SettingsRow :label="t('serverSettings.serverName')"><input v-model="form.server.transport.tls.serverName" class="tx-input" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.certFile')"><input v-model="form.server.transport.tls.certFile" class="tx-input mono" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.keyFile')"><input v-model="form.server.transport.tls.keyFile" class="tx-input mono" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.caFile')"><input v-model="form.server.transport.tls.trustedCaFile" class="tx-input mono" /></SettingsRow>
        </SettingsGroup>
        <SettingsGroup :title="t('serverSettings.groups.logPolicy')">
          <SettingsRow :label="t('serverSettings.logTo')"><input v-model="form.server.log.to" class="tx-input mono" placeholder="console" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.logLevel')">
            <TxSelect v-model="form.server.log.level" :aria-label="t('serverSettings.logLevelAria')" :options="logLevelOptions" />
          </SettingsRow>
          <SettingsRow :label="t('serverSettings.logMaxDays')"><NumberField v-model="form.server.log.maxDays" :min="0" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.logDisableColor')" short><TxSwitch v-model="form.server.log.disablePrintColor" /></SettingsRow>
        </SettingsGroup>
        <SettingsGroup :title="t('serverSettings.groups.adminWeb')" :note="t('serverSettings.adminWebNote')">
          <SettingsRow :label="t('serverSettings.port')"><NumberField v-model="form.server.webServer.port" :min="0" :max="65535" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.listenAddr')"><input v-model="form.server.webServer.addr" class="tx-input mono" placeholder="127.0.0.1" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.username')"><input v-model="form.server.webServer.user" class="tx-input" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.password')"><input v-model="form.server.webServer.password" class="tx-input" type="password" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.assetsDir')"><input v-model="form.server.webServer.assetsDir" class="tx-input mono" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.enablePprof')"><TxSwitch v-model="form.server.webServer.pprofEnable" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.webTls')"><TxSwitch v-model="webTlsOn" /></SettingsRow>
          <template v-if="form.server.webServer.tls">
            <SettingsRow :label="t('serverSettings.webCertFile')"><input v-model="form.server.webServer.tls.certFile" class="tx-input mono" /></SettingsRow>
            <SettingsRow :label="t('serverSettings.webKeyFile')"><input v-model="form.server.webServer.tls.keyFile" class="tx-input mono" /></SettingsRow>
            <SettingsRow :label="t('serverSettings.webCaFile')"><input v-model="form.server.webServer.tls.trustedCaFile" class="tx-input mono" /></SettingsRow>
            <SettingsRow :label="t('serverSettings.webServerName')"><input v-model="form.server.webServer.tls.serverName" class="tx-input" /></SettingsRow>
          </template>
        </SettingsGroup>
        <SettingsGroup :title="t('serverSettings.groups.other')">
          <SettingsRow :label="t('serverSettings.userPrefix')"><input v-model="form.server.user" class="tx-input" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.clientId')"><input v-model="form.server.clientID" class="tx-input" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.dnsServer')"><input v-model="form.server.dnsServer" class="tx-input mono" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.stunServer')"><input v-model="form.server.natHoleStunServer" class="tx-input mono" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.loginFailExit')"><TriStateSwitch v-model="form.server.loginFailExit" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.udpPacketSize')"><NumberField v-model="form.server.udpPacketSize" :min="0" /></SettingsRow>
          <SettingsRow :label="t('serverSettings.metadata')" vertical><KeyValueEditor v-model="form.server.metadatas" /></SettingsRow>
        </SettingsGroup>
      </div>
    </template>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import AdvancedToggle from '@/components/ui/AdvancedToggle.vue'
import KeyValueEditor from '@/components/ui/KeyValueEditor.vue'
import NumberField from '@/components/ui/NumberField.vue'
import SecretInput from '@/components/ui/SecretInput.vue'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import TriStateSwitch from '@/components/ui/TriStateSwitch.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import TxSegmentedControl from '@/components/ui/TxSegmentedControl.vue'
import TxSwitch from '@/components/ui/TxSwitch.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { LOG_LEVELS, PROTOCOLS } from '@/providers/frp/domain/options'
import type { Profile } from '@/providers/frp/domain/profile'
import { newTlsFiles } from '@/providers/frp/domain/server'
import { clone } from '@/domain/util'
import { validateConnectionName } from '@/domain/connectionName'
import { useProfileStore } from '@/providers/frp/stores/profile'
import { useUiStore } from '@/stores/ui'
import { useVersionStore } from '@/providers/frp/stores/version'

const { t } = useI18n()
const profiles = useProfileStore()
const versions = useVersionStore()
const ui = useUiStore()
const form = ref<Profile | null>(null)
const saving = ref(false)
const showAdvanced = ref(false)
const installedVersions = computed(() => versions.installed)
const versionOptions = computed(() => [
  { label: t('serverSettings.followGlobal'), value: '' },
  ...installedVersions.value.map((version) => ({ label: `v${version.version}`, value: version.version })),
])
const logLevelOptions = computed(() => [{ label: t('serverSettings.logLevelDefault'), value: '' }, ...LOG_LEVELS])
const authMethodOptions = [
  { label: 'Token', value: 'token' },
  { label: 'OIDC', value: 'oidc' },
]
const versionModel = computed({
  get: () => form.value?.frpcVersion ?? '',
  set: (value: string) => { if (form.value) form.value.frpcVersion = value || null },
})
const webTlsOn = computed({
  get: () => !!form.value?.server.webServer.tls,
  set: (value: boolean) => {
    if (form.value) form.value.server.webServer.tls = value ? newTlsFiles() : null
  },
})

watch(() => profiles.current, hydrate, { immediate: true })
function hydrate() { form.value = profiles.current ? clone(profiles.current) : null }
function reset() { hydrate() }
async function save() {
  if (!form.value) return
  const name = validateConnectionName(form.value.name)
  if (!name.ok) {
    ui.notify(t(name.messageKey), 'warning')
    return
  }
  form.value.name = name.name
  if (!Number.isInteger(form.value.server.serverPort) || form.value.server.serverPort < 1 || form.value.server.serverPort > 65535) {
    ui.notify(t('serverSettings.invalidServerPort'), 'warning')
    return
  }
  saving.value = true
  try {
    await profiles.save(form.value)
    ui.notify(t('serverSettings.saved'), 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
  } finally {
    saving.value = false
  }
}

</script>

<style scoped>
.page-toolbar {
  position: sticky;
  top: 0;
  z-index: 5;
  min-height: 32px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  /* 抵消 WorkspacePage .inner 的顶部留白，吸顶时背景铺满、盖住下方滚动内容 */
  margin: -20px 0 16px;
  padding: 20px 0 14px;
  background: var(--tx-bg-app);
  border-bottom: 1px solid var(--tx-border-subtle);
  color: var(--tx-text-muted);
  font-size: 12px;
}
.page-toolbar div { display: flex; align-items: center; gap: 8px; }
.settings-grid {
  width: 100%;
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  align-items: start;
  gap: 20px;
}
.settings-grid :deep(.group) { margin-bottom: 0; }
.advanced-grid {
  margin-top: 18px;
}
.advanced-toggle {
  margin-top: 20px;
}

</style>
