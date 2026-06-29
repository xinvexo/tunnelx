<template>
  <ProviderRuntimeCard
    provider-name="cloudflared"
    :available="status?.available"
    :runtime-managed="status?.managed"
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
  >
    <div
      v-if="status && !status.available"
      class="mt-14px flex min-h-42px items-center gap-8px border border-amber-600/22 rounded-[var(--tx-radius-md)] bg-amber-500/8 px-11px py-9px text-12px text-amber-800"
    >
      <Icon class="h-16px w-16px flex-none" icon="lucide:circle-alert" />
      <span>{{ t('runtime.cloudflareMissing') }}</span>
    </div>
  </ProviderRuntimeCard>
</template>

<script setup lang="ts">
import { computed, onMounted } from 'vue'
import { storeToRefs } from 'pinia'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import ProviderRuntimeCard from '@/providers/components/ProviderRuntimeCard.vue'
import { CLOUDFLARE_PROVIDER_ID } from '@/providers/contract'
import { useProviderRuntimeEnvironmentState } from '@/providers/useProviderRuntimeEnvironmentState'
import { useRuntimeInstallProgress } from '@/providers/useRuntimeInstallProgress'
import { useUiStore } from '@/stores/ui'
import { useCloudflareStore } from '../stores'

const cloudflare = useCloudflareStore()
const ui = useUiStore()
const { t } = useI18n()
const { status } = storeToRefs(cloudflare)
const {
  updateStatus,
  refreshing,
  installing,
  uninstalling,
  checkingUpdate,
  updating,
  runRuntimeTask,
} = useProviderRuntimeEnvironmentState(CLOUDFLARE_PROVIDER_ID)
const {
  runtimeInstallProgressPercent,
  resetRuntimeInstallProgress,
} = useRuntimeInstallProgress(CLOUDFLARE_PROVIDER_ID)

const installedVersion = computed(() => formatCloudflaredVersion(status.value?.version))

function formatCloudflaredVersion(version?: string | null) {
  if (!version?.trim()) return '-'
  const match = version.match(/cloudflared\s+version\s+([^\s(]+)/i)
  return match?.[1] ?? version.trim()
}

onMounted(async () => {
  await cloudflare.init()
  if (!status.value) await refreshEnvironment()
})

async function refreshEnvironment() {
  await runRuntimeTask('refresh', async () => {
    try {
      await cloudflare.refreshStatus()
    } catch (error) {
      ui.notify(String(error), 'warning')
    }
  })
}

async function checkRuntimeUpdate(silent = false) {
  await runRuntimeTask('checkUpdate', async () => {
    try {
      updateStatus.value = await cloudflare.checkCloudflaredUpdate()
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
      await cloudflare.installCloudflared()
      ui.notify(t('runtime.installed', { runtime: 'cloudflared' }), 'success')
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
      await cloudflare.installCloudflared()
      ui.notify(t('runtime.installed', { runtime: 'cloudflared' }), 'success')
      await refreshEnvironment()
    } catch (error) {
      ui.notify(String(error), 'danger')
    }
  })
}

async function uninstallRuntime() {
  const ok = await ui.confirm({
    title: t('runtime.uninstallTitle', { runtime: 'cloudflared' }),
    message: t('runtime.uninstallMessage', { runtime: 'cloudflared' }),
    confirmLabel: t('runtime.uninstall'),
    tone: 'danger',
  })
  if (!ok) return
  await runRuntimeTask('uninstall', async () => {
    try {
      await cloudflare.uninstallCloudflared()
      ui.notify(t('runtime.uninstalled', { runtime: 'cloudflared' }), 'success')
      await refreshEnvironment()
    } catch (error) {
      ui.notify(String(error), 'danger')
    }
  })
}
</script>
