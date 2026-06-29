<template>
  <WorkspacePage wide>
    <div v-if="resource && draft" class="grid gap-16px">
      <SettingsGroup :title="t('providers.ngrok.settingsTitle')">
        <SettingsRow :label="t('providerCredentials.status')">
          <CredentialStatusBadge :authenticated="isAuthenticated" />
        </SettingsRow>
        <SettingsRow :label="t('providers.ngrok.authtoken')" vertical>
          <SecretInput v-model="draft.authtoken" mono :placeholder="t('providers.ngrok.authtokenPlaceholder')" />
        </SettingsRow>
        <SettingsRow :label="t('providers.ngrok.region')">
          <TxSelect v-model="draft.region" :aria-label="t('providers.ngrok.region')" :options="regionOptions" />
        </SettingsRow>
        <div class="px-14px pb-14px pt-12px">
          <TxButton class="min-w-140px" tone="primary" icon="lucide:shield-check" :loading="authenticating" @click="authenticate">
            {{ isAuthenticated ? t('providerCredentials.reauthenticate') : t('providerCredentials.authenticate') }}
          </TxButton>
        </div>
      </SettingsGroup>
    </div>
    <div v-else class="text-13px text-[var(--tx-text-muted)]">{{ t('providerTunnel.notFound', { provider: 'ngrok' }) }}</div>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import SecretInput from '@/components/ui/SecretInput.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { clone } from '@/domain/util'
import CredentialStatusBadge from '@/providers/components/CredentialStatusBadge.vue'
import { useConnectionStore } from '@/providers/connections'
import { NGROK_PROVIDER_ID } from '@/providers/contract'
import { useUiStore } from '@/stores/ui'
import { authenticateNgrokTunnel } from '../api'
import { applyNgrokMetadata, ngrokMetadata, type NgrokMetadata } from '../domain'

const route = useRoute()
const connections = useConnectionStore()
const ui = useUiStore()
const { t } = useI18n()
const id = computed(() => String(route.params.id ?? ''))
const resource = computed(() => connections.resourceOf(NGROK_PROVIDER_ID, id.value))
const draft = ref<NgrokMetadata | null>(null)
const authenticating = ref(false)
const regionOptions = computed(() => [
  { label: t('providers.ngrok.regionDefault'), value: '' },
  { label: t('providers.ngrok.regionUs'), value: 'us' },
  { label: t('providers.ngrok.regionEu'), value: 'eu' },
  { label: t('providers.ngrok.regionAp'), value: 'ap' },
  { label: t('providers.ngrok.regionAu'), value: 'au' },
  { label: t('providers.ngrok.regionSa'), value: 'sa' },
  { label: t('providers.ngrok.regionJp'), value: 'jp' },
  { label: t('providers.ngrok.regionIn'), value: 'in' },
])
const savedMetadata = computed(() => resource.value ? ngrokMetadata(resource.value) : null)
const isAuthenticated = computed(() =>
  Boolean(draft.value?.authtoken.trim())
  && draft.value?.authtoken.trim() === savedMetadata.value?.authtoken.trim()
  && draft.value?.region.trim() === savedMetadata.value?.region.trim(),
)

watch(resource, (value) => {
  draft.value = value ? clone(ngrokMetadata(value)) : null
}, { immediate: true })

async function authenticate() {
  if (!resource.value || !draft.value) return
  if (!draft.value.authtoken.trim()) {
    ui.notify(t('providers.ngrok.authtokenRequired'), 'warning')
    return
  }
  authenticating.value = true
  try {
    const saved = await authenticateNgrokTunnel(applyNgrokMetadata(resource.value, draft.value))
    await connections.refreshProvider(NGROK_PROVIDER_ID)
    draft.value = clone(ngrokMetadata(connections.resourceOf(NGROK_PROVIDER_ID, saved.id) ?? saved))
    ui.notify(t('providers.ngrok.authenticated'), 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
  } finally {
    authenticating.value = false
  }
}
</script>
