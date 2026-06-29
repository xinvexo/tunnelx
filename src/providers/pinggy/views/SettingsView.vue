<template>
  <WorkspacePage wide>
    <div v-if="resource && draft" class="grid gap-16px">
      <SettingsGroup :title="t('providers.pinggy.settingsTitle')">
        <SettingsRow :label="t('providerCredentials.status')">
          <CredentialStatusBadge :authenticated="isAuthenticated" />
        </SettingsRow>
        <SettingsRow :label="t('providers.pinggy.token')" vertical>
          <SecretInput v-model="draft.token" mono :placeholder="t('providers.pinggy.tokenPlaceholder')" />
        </SettingsRow>
        <div class="px-14px pt-12px">
          <AdvancedToggle :open="advancedOpen" @click="advancedOpen = !advancedOpen" />
        </div>
        <template v-if="advancedOpen">
          <SettingsRow :label="t('providers.pinggy.server')">
            <input v-model="draft.server" class="tx-input mono" placeholder="free.pinggy.io" />
          </SettingsRow>
          <SettingsRow :label="t('providers.pinggy.serverPort')">
            <NumberField v-model="serverPortModel" :min="1" :max="65535" />
          </SettingsRow>
          <SettingsRow :label="t('providers.pinggy.debuggerPort')">
            <NumberField v-model="draft.debuggerPort" :min="1" :max="65535" :placeholder="t('providerForm.optional')" />
          </SettingsRow>
        </template>
        <div class="px-14px pb-14px pt-12px">
          <TxButton class="min-w-140px" tone="primary" icon="lucide:shield-check" :loading="authenticating" @click="authenticate">
            {{ isAuthenticated ? t('providerCredentials.reauthenticate') : t('providerCredentials.authenticate') }}
          </TxButton>
        </div>
      </SettingsGroup>
    </div>
    <div v-else class="text-13px text-[var(--tx-text-muted)]">{{ t('providerTunnel.notFound', { provider: 'Pinggy' }) }}</div>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import AdvancedToggle from '@/components/ui/AdvancedToggle.vue'
import NumberField from '@/components/ui/NumberField.vue'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import SecretInput from '@/components/ui/SecretInput.vue'
import TxButton from '@/components/ui/TxButton.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { clone } from '@/domain/util'
import CredentialStatusBadge from '@/providers/components/CredentialStatusBadge.vue'
import { useConnectionStore } from '@/providers/connections'
import { PINGGY_PROVIDER_ID } from '@/providers/contract'
import { useUiStore } from '@/stores/ui'
import { authenticatePinggyTunnel } from '../api'
import { applyPinggyMetadata, pinggyMetadata, type PinggyMetadata } from '../domain'

const route = useRoute()
const connections = useConnectionStore()
const ui = useUiStore()
const { t } = useI18n()
const id = computed(() => String(route.params.id ?? ''))
const resource = computed(() => connections.resourceOf(PINGGY_PROVIDER_ID, id.value))
const draft = ref<PinggyMetadata | null>(null)
const authenticating = ref(false)
const advancedOpen = ref(false)
const savedMetadata = computed(() => resource.value ? pinggyMetadata(resource.value) : null)
const isAuthenticated = computed(() =>
  Boolean(draft.value?.token.trim())
  && draft.value?.token.trim() === savedMetadata.value?.token.trim()
  && draft.value?.server.trim() === savedMetadata.value?.server.trim()
  && draft.value?.serverPort === savedMetadata.value?.serverPort
  && draft.value?.debuggerPort === savedMetadata.value?.debuggerPort,
)
const serverPortModel = computed({
  get: () => draft.value?.serverPort ?? 443,
  set: (value: number | null) => { if (draft.value) draft.value.serverPort = value || 443 },
})

watch(resource, (value) => {
  draft.value = value ? clone(pinggyMetadata(value)) : null
}, { immediate: true })

async function authenticate() {
  if (!resource.value || !draft.value) return
  if (!draft.value.token.trim()) {
    ui.notify(t('providers.pinggy.tokenRequired'), 'warning')
    return
  }
  if (draft.value.serverPort < 1 || draft.value.serverPort > 65535) {
    ui.notify(t('providers.pinggy.serverPortInvalid'), 'warning')
    return
  }
  authenticating.value = true
  try {
    const saved = await authenticatePinggyTunnel(applyPinggyMetadata(resource.value, draft.value))
    await connections.refreshProvider(PINGGY_PROVIDER_ID)
    draft.value = clone(pinggyMetadata(connections.resourceOf(PINGGY_PROVIDER_ID, saved.id) ?? saved))
    ui.notify(t('providers.pinggy.authenticated'), 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
  } finally {
    authenticating.value = false
  }
}
</script>
