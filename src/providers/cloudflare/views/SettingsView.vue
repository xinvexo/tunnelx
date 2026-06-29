<template>
  <WorkspacePage wide>
    <div v-if="draft" class="settings-grid">
      <SettingsGroup :title="t('cloudflare.settingsTitle')">
        <SettingsRow :label="t('cloudflare.authMode')">
          <TxSegmentedControl
            :model-value="draft.account.authMode"
            :options="authModeOptions"
            @update:model-value="selectAuthMode($event as CloudflareAuthMode)"
          />
        </SettingsRow>

        <template v-if="draft.account.authMode === 'authorization'">
          <SettingsRow :label="t('cloudflare.authStatus')" :desc="t('cloudflare.authStatusDesc')">
            <span class="status-pill" :class="isAuthorizationReady ? 'ok' : 'idle'">{{ authorizationStatus }}</span>
          </SettingsRow>
          <SettingsRow :label="t('cloudflare.accountId')">
            <code class="path-value">{{ draft.account.authorizationAccountId || '-' }}</code>
          </SettingsRow>
          <SettingsRow :label="t('cloudflare.zoneId')">
            <code class="path-value">{{ draft.account.authorizationZoneId || '-' }}</code>
          </SettingsRow>
          <div class="row-actions compact">
            <TxButton tone="primary" icon="lucide:log-in" :loading="loggingIn" @click="loginConnection">
              {{ isAuthorizationReady ? t('cloudflare.reauthorize') : t('cloudflare.authorize') }}
            </TxButton>
          </div>
        </template>

        <template v-else>
          <SettingsRow :label="t('providerCredentials.status')">
            <CredentialStatusBadge :authenticated="isTokenAuthenticated" />
          </SettingsRow>
          <SettingsRow :label="t('cloudflare.apiToken')" vertical>
            <SecretInput v-model="draft.account.apiToken" mono :placeholder="t('cloudflare.apiTokenPlaceholder')" />
          </SettingsRow>
          <SettingsRow :label="t('cloudflare.accountId')">
            <input v-model="draft.account.tokenAccountId" class="tx-input mono" :placeholder="t('cloudflare.accountIdPlaceholder')" />
          </SettingsRow>
          <SettingsRow :label="t('cloudflare.accountName')">
            <input v-model="draft.account.tokenAccountName" class="tx-input" :placeholder="t('cloudflare.accountNamePlaceholder')" />
          </SettingsRow>
          <div class="row-actions compact">
            <TxButton tone="primary" icon="lucide:shield-check" :loading="verifying" @click="authenticateToken">
              {{ isTokenAuthenticated ? t('providerCredentials.reauthenticate') : t('providerCredentials.authenticate') }}
            </TxButton>
          </div>
        </template>
      </SettingsGroup>
    </div>
    <div v-else class="empty">
      <span>{{ t('cloudflare.notFound') }}</span>
    </div>
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
import TxSegmentedControl from '@/components/ui/TxSegmentedControl.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { clone } from '@/domain/util'
import CredentialStatusBadge from '@/providers/components/CredentialStatusBadge.vue'
import { useConnectionStore } from '@/providers/connections'
import { CLOUDFLARE_PROVIDER_ID } from '@/providers/contract'
import { useUiStore } from '@/stores/ui'
import type { CloudflareAuthMode, CloudflareTunnel } from '../domain'
import { useCloudflareStore } from '../stores'

const route = useRoute()
const cloudflare = useCloudflareStore()
const connections = useConnectionStore()
const ui = useUiStore()
const { t } = useI18n()
const id = computed(() => String(route.params.id ?? ''))
const draft = ref<CloudflareTunnel | null>(null)
const verifying = ref(false)
const loggingIn = ref(false)
const authModeOptions = computed(() => [
  { label: t('cloudflare.authorizationMode'), value: 'authorization' },
  { label: t('cloudflare.tokenMode'), value: 'apiToken' },
])
const savedTunnel = computed(() => cloudflare.tunnels.find((item) => item.id === id.value))
const isAuthorizationReady = computed(() =>
  draft.value?.account.authMode === 'authorization'
  && Boolean(draft.value?.certFile.trim())
  && Boolean(draft.value?.account.authorizationAccountId.trim()),
)
const isTokenAuthenticated = computed(() => {
  const account = draft.value?.account
  const saved = savedTunnel.value?.account
  return account?.authMode === 'apiToken'
    && saved?.authMode === 'apiToken'
    && Boolean(account.apiToken.trim())
    && Boolean(account.tokenAccountId.trim())
    && account.apiToken.trim() === saved.apiToken.trim()
    && account.tokenAccountId.trim() === saved.tokenAccountId.trim()
})
const authorizationStatus = computed(() => {
  if (draft.value?.account.authMode === 'apiToken') return t('cloudflare.tokenMode')
  return isAuthorizationReady.value ? t('cloudflare.authorized') : t('cloudflare.unauthorized')
})

watch(id, async () => {
  await hydrate()
}, { immediate: true })

async function hydrate() {
  await cloudflare.init()
  const tunnel = cloudflare.tunnels.find((item) => item.id === id.value)
  draft.value = tunnel ? clone(tunnel) : null
}

async function authenticateToken() {
  if (!draft.value) return
  draft.value.account.authMode = 'apiToken'
  if (!draft.value.account.apiToken.trim()) {
    ui.notify(t('cloudflare.tokenRequired'), 'warning')
    return
  }
  if (!draft.value.account.tokenAccountId.trim()) {
    ui.notify(t('cloudflare.accountIdRequired'), 'warning')
    return
  }
  verifying.value = true
  try {
    const saved = await cloudflare.saveTunnel(draft.value)
    draft.value = clone(saved)
    await connections.refreshProvider(CLOUDFLARE_PROVIDER_ID)
    let zoneCount = 0
    try {
      zoneCount = (await cloudflare.refreshZones(saved.id)).length
    } catch {
      zoneCount = 0
    }
    ui.notify(t('cloudflare.tokenVerifiedWithZones', { count: zoneCount }), 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
  } finally {
    verifying.value = false
  }
}

function selectAuthMode(mode: CloudflareAuthMode) {
  if (!draft.value) return
  if (draft.value.account.authMode === mode) return
  draft.value.account.authMode = mode
  if (mode === 'authorization') {
    draft.value.account.apiToken = ''
    draft.value.account.tokenAccountId = ''
    draft.value.account.tokenAccountName = ''
  }
}

async function loginConnection() {
  if (!draft.value) return
  loggingIn.value = true
  try {
    const output = await cloudflare.loginConnection(draft.value.id)
    ui.notify(
      output.success
        ? t('cloudflare.authorizationSaved')
        : (output.stderr || output.stdout || t('cloudflare.authorizationIncomplete')),
      output.success ? 'success' : 'warning',
    )
    await hydrate()
  } catch (error) {
    ui.notify(String(error), 'danger')
  } finally {
    loggingIn.value = false
  }
}

</script>

<style scoped>
.settings-grid {
  display: grid;
  gap: 16px;
}

.settings-grid :deep(.row:not(.vertical)) {
  grid-template-columns: minmax(148px, 0.5fr) minmax(0, 1fr);
  gap: 14px;
}

.settings-grid :deep(.control) {
  justify-content: stretch;
}

.row-actions {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(136px, 1fr));
  gap: 8px;
  padding: 12px 14px 14px;
}

.row-actions.compact {
  grid-template-columns: repeat(auto-fit, minmax(150px, max-content));
}

.row-actions :deep(.tx-button) {
  width: 100%;
}

.path-value {
  min-width: 0;
  display: block;
  overflow: hidden;
  color: var(--tx-text-secondary);
  font-family: var(--tx-mono);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-pill {
  width: max-content;
  max-width: 100%;
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  padding: 3px 8px;
  background: var(--tx-bg-muted);
  color: var(--tx-text-secondary);
  font-size: 12px;
  white-space: nowrap;
}

.status-pill.ok {
  background: rgba(22, 163, 74, 0.1);
  color: #15803d;
}

.status-pill.idle {
  color: var(--tx-text-muted);
}

.empty {
  color: var(--tx-text-muted);
  font-size: 13px;
}

.empty {
  min-height: 220px;
  display: flex;
  align-items: center;
  justify-content: center;
}
</style>
