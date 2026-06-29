<template>
  <ProviderRuntimeCard
    provider-name="frpc"
    :available="runtimeAvailable"
    :runtime-managed="runtimeAvailable"
    :installed-version="installedVersion"
    :refreshing="refreshing"
    :installing="installing"
    :install-progress-percent="activeInstallProgressPercent"
    :uninstalling="uninstalling"
    :checking-update="checkingUpdate"
    :updating="updating"
    :update-available="updateStatus?.updateAvailable"
    :latest-version="updateStatus?.latestVersion || ''"
    :install-label="t('version.installRuntime')"
    :reinstall-label="t('version.reinstallRuntime')"
    :uninstall-label="t('version.uninstallRuntime')"
    :version-label="t('version.installedVersion')"
    :available-label="t('version.status.available')"
    :missing-label="t('version.status.missing')"
    @install="installRuntime"
    @uninstall="uninstallRuntime"
    @refresh="refreshEnvironment"
    @check-update="checkRuntimeUpdate"
    @update="updateRuntime"
  >
    <div
      v-if="!runtimeAvailable"
      class="mt-14px flex min-h-42px items-center gap-8px border border-amber-600/22 rounded-[var(--tx-radius-md)] bg-amber-500/8 px-11px py-9px text-12px text-amber-800"
    >
      <Icon class="h-16px w-16px flex-none" icon="lucide:circle-alert" />
      <span>{{ t('version.noActiveBanner') }}</span>
    </div>

    <AdvancedToggle class="mt-14px" :open="advancedOpen" :label="t('version.advanced')" @click="advancedOpen = !advancedOpen" />

    <div v-if="advancedOpen" class="advanced-panel">
      <div class="mirror-bar">
        <div class="mirror-label">
          <Icon icon="lucide:cloud-download" />
          <span>{{ t('version.mirror.label') }}</span>
        </div>
        <TxSelect v-model="mirror" :options="mirrorOptions" @update:model-value="onMirrorChange" />
        <input
          v-if="mirror === CUSTOM"
          v-model="customMirror"
          class="mirror-input"
          :placeholder="t('version.mirror.placeholder')"
          @change="onCustomChange"
        />
      </div>

      <div v-if="store.remoteError" class="remote-warning">
        <Icon icon="lucide:cloud-off" />
        <span>{{ t('version.remoteUnavailable') }}</span>
      </div>

      <div v-if="store.versions.length" class="version-list">
        <div v-for="version in store.versions" :key="version.version" class="version-row">
          <div class="version-info">
            <div class="version-name">
              <strong>v{{ version.version }}</strong>
              <span v-if="version.active" class="badge active">{{ t('version.badge.active') }}</span>
              <span v-else-if="version.installed" class="badge">{{ t('version.badge.installed') }}</span>
            </div>
            <div class="meta">
              <span v-if="version.size">{{ formatSize(version.size) }}</span>
              <span v-if="version.publishedAt">{{ formatDate(version.publishedAt) }}</span>
              <span v-if="!version.downloadUrl && !version.installed" class="warn">{{ t('version.unavailable') }}</span>
            </div>
          </div>
          <span v-if="store.installing[version.version]" class="percent">{{ percent(version.version) }}%</span>
          <div class="actions">
            <TxIconButton
              v-if="!version.installed"
              icon="lucide:download"
              :label="t('version.download')"
              primary
              :action-key="installKey(version.version)"
              :locks="versionRowKey(version.version)"
              :loading="!!store.installing[version.version]"
              :disabled="!version.downloadUrl || isRowBusy(version.version)"
              :success="() => t('version.installed', { version: version.version })"
              @click="install(version.version)"
            />
            <TxIconButton
              v-if="version.active"
              class="active-action"
              icon="lucide:check"
              :label="t('version.badge.active')"
              primary
              disabled
            />
            <TxIconButton
              v-if="version.installed && !version.active"
              icon="lucide:zap"
              :label="t('version.activate')"
              primary
              :action-key="activateKey(version.version)"
              :locks="['frpc-version:activate', versionRowKey(version.version)]"
              :disabled="isActivateDisabled(version.version)"
              :success="(changed) => changed ? t('version.activated', { version: version.version }) : false"
              @click="activate(version.version)"
            />
            <TxIconButton
              v-if="version.installed"
              icon="lucide:trash-2"
              :label="t('version.deleteVersion')"
              danger
              :action-key="removeKey(version.version)"
              :locks="['frpc-version:remove', versionRowKey(version.version)]"
              :disabled="isRemoveDisabled(version.version)"
              :success="(removed) => removed ? t('version.removed') : false"
              @click="remove(version.version)"
            />
          </div>
          <div v-if="store.installing[version.version]" class="progress-bar">
            <span :style="{ width: `${percent(version.version)}%` }" />
          </div>
        </div>
      </div>
      <div v-else class="empty">{{ t('version.empty') }}</div>
    </div>
  </ProviderRuntimeCard>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import AdvancedToggle from '@/components/ui/AdvancedToggle.vue'
import TxIconButton from '@/components/ui/TxIconButton.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import type { TxSelectOption } from '@/components/ui/TxSelect.vue'
import { useAsyncAction } from '@/composables/useAsyncAction'
import ProviderRuntimeCard from '@/providers/components/ProviderRuntimeCard.vue'
import { useUiStore } from '@/stores/ui'
import { useVersionStore } from '@/providers/frp/stores/version'
import { FRP_PROVIDER_ID } from '@/providers/contract'
import { useProviderRuntimeEnvironmentState } from '@/providers/useProviderRuntimeEnvironmentState'
import { formatDate, formatSize } from '@/utils/format'

const { t } = useI18n()
const store = useVersionStore()
const ui = useUiStore()
const actions = useAsyncAction()
const {
  updateStatus,
  refreshing,
  installing,
  uninstalling,
  checkingUpdate,
  updating,
  runRuntimeTask,
} = useProviderRuntimeEnvironmentState(FRP_PROVIDER_ID)
const advancedOpen = ref(false)
const installingVersion = ref('')

const CUSTOM = '__custom__'
const PRESETS = computed<{ label: string; value: string }[]>(() => [
  { label: t('version.mirror.direct'), value: '' },
  { label: 'ghfast.top', value: 'https://ghfast.top' },
  { label: 'gh-proxy.com', value: 'https://gh-proxy.com' },
  { label: 'ghproxy.net', value: 'https://ghproxy.net' },
])
const mirrorOptions = computed<TxSelectOption[]>(() => [
  ...PRESETS.value,
  { label: t('version.mirror.custom'), value: CUSTOM },
])
const mirror = ref('')
const customMirror = ref('')

const runtimeAvailable = computed(() => !!store.activeVersion)
const installedVersion = computed(() => store.activeVersion ? `v${store.activeVersion}` : '-')
const recommendedVersion = computed(() =>
  store.versions.find((version) => version.downloadUrl || version.installed)?.version ?? '',
)
const activeInstallVersion = computed(() =>
  installingVersion.value || Object.keys(store.installing).find((version) => store.installing[version]) || '',
)
const activeInstallProgressPercent = computed(() => {
  if (!activeInstallVersion.value) return null
  const progress = store.progress[activeInstallVersion.value]
  return progress?.total ? (progress.received / progress.total) * 100 : null
})

onMounted(async () => {
  try {
    await store.ensureLoaded()
  } catch (error) {
    ui.notify(String(error), 'danger')
  }
  syncMirrorSelection()
})

function syncMirrorSelection() {
  const saved = store.mirror
  if (PRESETS.value.some((preset) => preset.value === saved)) {
    mirror.value = saved
  } else if (saved) {
    mirror.value = CUSTOM
    customMirror.value = saved
  } else {
    mirror.value = ''
  }
}

async function refreshEnvironment() {
  await runRuntimeTask('refresh', async () => {
    try {
      await Promise.all([store.loadActive(), store.loadMirror()])
      syncMirrorSelection()
      ui.notify(t('runtime.refreshed'), 'success')
    } catch (error) {
      ui.notify(String(error), 'danger')
    }
  })
}

async function checkRuntimeUpdate(silent = false) {
  await runRuntimeTask('checkUpdate', async () => {
    try {
      updateStatus.value = await store.checkUpdate()
      if (!silent && !updateStatus.value.updateAvailable) {
        ui.notify(t('runtime.latest'), 'success')
      }
    } catch (error) {
      if (!silent) ui.notify(String(error), 'warning')
    }
  })
}

async function updateRuntime() {
  const version = updateStatus.value?.latestVersion || recommendedVersion.value
  if (!version) {
    ui.notify(t('version.empty'), 'warning')
    return
  }
  await runRuntimeTask('update', async () => {
    installingVersion.value = version
    try {
      const row = store.versions.find((item) => item.version === version)
      if (!row?.installed) await store.install(version)
      await store.activate(version)
      ui.notify(t('version.activated', { version }), 'success')
      await store.load()
      updateStatus.value = await store.checkUpdate()
    } catch (error) {
      ui.notify(String(error), 'danger')
    } finally {
      installingVersion.value = ''
    }
  })
}

async function installRuntime() {
  await runRuntimeTask('install', async () => {
    try {
      await store.ensureLoaded()
      const version = store.activeVersion ?? recommendedVersion.value
      if (!version) {
        ui.notify(t('version.empty'), 'warning')
        return
      }
      installingVersion.value = version
      const row = store.versions.find((item) => item.version === version)
      if (row?.installed && !row.active) {
        await store.activate(version)
        ui.notify(t('version.activated', { version }), 'success')
        return
      }
      await store.install(version)
      if (!store.activeVersion) await store.loadActive()
      ui.notify(t('version.installed', { version }), 'success')
    } catch (error) {
      ui.notify(String(error), 'danger')
    } finally {
      installingVersion.value = ''
    }
  })
}

async function uninstallRuntime() {
  const ok = await ui.confirm({
    title: t('version.uninstallConfirmTitle'),
    message: t('version.uninstallConfirmMessage'),
    confirmLabel: t('version.uninstallRuntime'),
    tone: 'danger',
  })
  if (!ok) return
  await runRuntimeTask('uninstall', async () => {
    try {
      await store.uninstallActive()
      ui.notify(t('version.uninstalledRuntime'), 'success')
    } catch (error) {
      ui.notify(String(error), 'danger')
    }
  })
}

async function applyMirror(value: string) {
  try {
    await store.setMirror(value)
    ui.notify(value ? t('version.mirror.switched') : t('version.mirror.switchedDirect'), 'success')
  } catch (error) {
    ui.notify(t('version.mirror.setFailed', { error: String(error) }), 'danger')
  }
}

function onMirrorChange(value: string) {
  if (value === CUSTOM) {
    if (customMirror.value.trim()) void applyMirror(customMirror.value.trim())
    return
  }
  void applyMirror(value)
}

function onCustomChange() {
  void applyMirror(customMirror.value.trim())
}

function percent(version: string) {
  const progress = store.progress[version]
  return progress?.total ? Math.floor((progress.received / progress.total) * 100) : 0
}

function versionRowKey(version: string) {
  return `frpc-version:row:${version}`
}

function installKey(version: string) {
  return `frpc-version:install:${version}`
}

function activateKey(version: string) {
  return `frpc-version:activate:${version}`
}

function removeKey(version: string) {
  return `frpc-version:remove:${version}`
}

function isRowBusy(version: string) {
  return !!store.installing[version] || actions.isBusy(versionRowKey(version))
}

function isActivateDisabled(version: string) {
  return isRowBusy(version) || actions.isBusy('frpc-version:activate')
}

function isRemoveDisabled(version: string) {
  return store.activeVersion === version || isRowBusy(version) || actions.isBusy(['frpc-version:activate', 'frpc-version:remove'])
}

async function install(version: string) {
  await store.install(version)
}

async function activate(version: string) {
  return await store.activate(version)
}

async function remove(version: string) {
  if (store.activeVersion === version) return false
  const ok = await ui.confirm({
    title: t('common.delete'),
    message: t('version.confirmRemove', { version }),
    confirmLabel: t('common.delete'),
    tone: 'danger',
  })
  if (!ok) return false
  await store.remove(version)
  return true
}
</script>

<style scoped>
.advanced-panel {
  padding-top: 12px;
}

.mirror-bar {
  display: grid;
  grid-template-columns: auto minmax(180px, 240px);
  align-items: center;
  gap: 8px 12px;
  margin-bottom: 12px;
}

.mirror-label {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--tx-text-secondary);
  font-size: 12px;
}

.mirror-label svg { width: 15px; height: 15px; }

.mirror-input {
  grid-column: 2;
  height: 32px;
  border: 1px solid var(--tx-border-strong);
  border-radius: var(--tx-radius-sm);
  background: var(--tx-bg-surface);
  color: var(--tx-text-primary);
  padding: 0 9px;
  font-size: 12px;
  outline: none;
}

.mirror-input:focus {
  border-color: var(--tx-accent);
  box-shadow: 0 0 0 3px var(--tx-focus-ring);
}

.remote-warning {
  min-height: 38px;
  border: 1px solid rgba(217, 119, 6, 0.22);
  border-radius: var(--tx-radius-md);
  background: rgba(245, 158, 11, 0.08);
  color: #92400e;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 11px;
  margin-bottom: 12px;
}

.remote-warning svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}

.version-list {
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: var(--tx-bg-surface);
  overflow: hidden;
}

.version-row {
  position: relative;
  min-height: 54px;
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 8px 13px;
}

.version-row + .version-row { border-top: 1px solid var(--tx-border-subtle); }
.version-info { flex: 1; min-width: 0; }
.version-name { display: flex; align-items: center; gap: 7px; }
.badge { border-radius: 999px; background: #eef0f3; color: var(--tx-text-secondary); padding: 2px 6px; font-size: 10px; }
.badge.active { background: rgba(22, 163, 74, 0.1); color: #15803d; }
.meta { margin-top: 4px; color: var(--tx-text-muted); display: flex; gap: 8px; font-size: 11px; }
.warn { color: var(--tx-warning); }

.percent {
  font-family: var(--tx-mono);
  font-size: 12px;
  color: var(--tx-accent);
  font-variant-numeric: tabular-nums;
}

.actions { display: flex; align-items: center; gap: 4px; }
.actions :deep(.active-action:disabled) {
  opacity: 1;
  color: #15803d;
}
.actions :deep(.active-action:disabled svg) {
  stroke-width: 2.25;
}

.progress-bar {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 3px;
  background: var(--tx-bg-hover);
  overflow: hidden;
}

.progress-bar span {
  display: block;
  height: 100%;
  background: var(--tx-accent);
  background-image: linear-gradient(
    90deg,
    transparent 0%,
    rgba(255, 255, 255, 0.45) 50%,
    transparent 100%
  );
  background-size: 40px 100%;
  background-repeat: no-repeat;
  transition: width 0.2s ease;
  animation: progress-flow 1s linear infinite;
}

@keyframes progress-flow {
  from { background-position: -40px 0; }
  to { background-position: calc(100% + 40px) 0; }
}

.empty {
  min-height: 120px;
  border: 1px dashed var(--tx-border-strong);
  border-radius: var(--tx-radius-md);
  color: var(--tx-text-secondary);
  display: flex;
  align-items: center;
  justify-content: center;
}

</style>
