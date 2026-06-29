<template>
  <ProviderRuntimeCard
    provider-name="Pinggy"
    :available="status?.available"
    :runtime-managed="runtimeManaged"
    :installed-version="installedVersion"
    :refreshing="refreshing"
    :installing="installing"
    :install-progress-percent="runtimeInstallProgressPercent"
    :uninstalling="uninstalling"
    :checking-update="checkingUpdate"
    :updating="updating"
    :update-available="updateStatus?.updateAvailable"
    :latest-version="updateStatus?.latestVersion || ''"
    @install="installRuntime"
    @uninstall="uninstallRuntime"
    @refresh="refreshEnvironment"
    @check-update="checkRuntimeUpdate"
    @update="updateRuntime"
  />
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderRuntimeCard from '@/providers/components/ProviderRuntimeCard.vue'
import { providerStatus } from '@/providers/api'
import { PINGGY_PROVIDER_ID, type ProviderStatus } from '@/providers/contract'
import { useProviderRuntimeEnvironmentState } from '@/providers/useProviderRuntimeEnvironmentState'
import { useRuntimeInstallProgress } from '@/providers/useRuntimeInstallProgress'
import { useUiStore } from '@/stores/ui'
import { checkPinggyRuntimeUpdate, installPinggyRuntime, uninstallPinggyRuntime } from '../api'

const ui = useUiStore()
const { t } = useI18n()
const {
  status,
  updateStatus,
  refreshing,
  installing,
  uninstalling,
  checkingUpdate,
  updating,
  runRuntimeTask,
} = useProviderRuntimeEnvironmentState(PINGGY_PROVIDER_ID)
const {
  runtimeInstallProgressPercent,
  resetRuntimeInstallProgress,
} = useRuntimeInstallProgress(PINGGY_PROVIDER_ID)

const installedVersion = computed(() => formatRuntimeVersion(status.value?.version))
const runtimeManaged = computed(() => isManaged(status.value))

onMounted(async () => {
  if (!status.value) await refreshEnvironment()
})

function formatRuntimeVersion(version?: string | null) {
  if (!version?.trim()) return '-'
  const match = version.match(/([0-9]+\.[0-9]+\.[0-9]+)/)
  return match?.[1] ?? version.trim()
}

function isManaged(value: ProviderStatus | null) {
  const details = value?.details
  return typeof details === 'object' && details !== null
    && (details as Record<string, unknown>).managed === true
}

async function refreshEnvironment() {
  await runRuntimeTask('refresh', async () => {
    try {
      status.value = await providerStatus(PINGGY_PROVIDER_ID)
    } catch (error) {
      ui.notify(String(error), 'warning')
    }
  })
}

async function checkRuntimeUpdate(silent = false) {
  await runRuntimeTask('checkUpdate', async () => {
    try {
      updateStatus.value = await checkPinggyRuntimeUpdate()
      if (!silent && !updateStatus.value.updateAvailable) {
        ui.notify(t('runtime.latest'), 'success')
      }
    } catch (error) {
      if (!silent) ui.notify(String(error), 'warning')
    }
  })
}

async function updateRuntime() {
  await runRuntimeTask('update', async () => {
    resetRuntimeInstallProgress()
    try {
      status.value = await installPinggyRuntime()
      ui.notify(t('runtime.installed', { runtime: 'Pinggy' }), 'success')
      await refreshEnvironment()
    } catch (error) {
      ui.notify(String(error), 'danger')
    }
  })
}

async function installRuntime() {
  await runRuntimeTask('install', async () => {
    resetRuntimeInstallProgress()
    try {
      status.value = await installPinggyRuntime()
      ui.notify(t('runtime.installed', { runtime: 'Pinggy' }), 'success')
      await refreshEnvironment()
    } catch (error) {
      ui.notify(String(error), 'danger')
    }
  })
}

async function uninstallRuntime() {
  const ok = await ui.confirm({
    title: t('runtime.uninstallTitle', { runtime: 'Pinggy' }),
    message: t('runtime.uninstallMessage', { runtime: 'Pinggy' }),
    confirmLabel: t('runtime.uninstall'),
    tone: 'danger',
  })
  if (!ok) return
  await runRuntimeTask('uninstall', async () => {
    try {
      status.value = await uninstallPinggyRuntime()
      ui.notify(t('runtime.uninstalled', { runtime: 'Pinggy' }), 'success')
      await refreshEnvironment()
    } catch (error) {
      ui.notify(String(error), 'danger')
    }
  })
}
</script>
