import { checkUpdate as checkAppUpdate } from '@/api/update'
import { i18n } from '@/i18n'
import {
  CLOUDFLARE_PROVIDER_ID,
  CPOLAR_PROVIDER_ID,
  FRP_PROVIDER_ID,
  NGROK_PROVIDER_ID,
  PINGGY_PROVIDER_ID,
  type ProviderRuntimeUpdateStatus,
} from '@/providers/contract'
import { useProviderRuntimeEnvironmentState } from '@/providers/useProviderRuntimeEnvironmentState'
import { checkCloudflaredUpdate } from '@/providers/cloudflare/api'
import { checkCpolarRuntimeUpdate } from '@/providers/cpolar/api'
import { useVersionStore } from '@/providers/frp/stores/version'
import { checkNgrokRuntimeUpdate } from '@/providers/ngrok/api'
import { checkPinggyRuntimeUpdate } from '@/providers/pinggy/api'
import { useSettingsStore } from '@/stores/settings'
import { useUiStore } from '@/stores/ui'

let checkedThisLaunch = false

type RuntimeCheck = {
  providerId: string
  check: () => Promise<ProviderRuntimeUpdateStatus>
}

const runtimeChecks: RuntimeCheck[] = [
  {
    providerId: FRP_PROVIDER_ID,
    check: () => useVersionStore().checkUpdate(),
  },
  {
    providerId: CLOUDFLARE_PROVIDER_ID,
    check: checkCloudflaredUpdate,
  },
  {
    providerId: NGROK_PROVIDER_ID,
    check: async () => checkNgrokRuntimeUpdate(),
  },
  {
    providerId: CPOLAR_PROVIDER_ID,
    check: async () => checkCpolarRuntimeUpdate(),
  },
  {
    providerId: PINGGY_PROVIDER_ID,
    check: checkPinggyRuntimeUpdate,
  },
]

export async function runStartupUpdateCheck() {
  if (checkedThisLaunch) return
  checkedThisLaunch = true

  const settings = useSettingsStore()
  await settings.init()
  if (!settings.autoUpdate) return

  await Promise.allSettled([
    checkApplicationUpdate(),
    ...runtimeChecks.map(checkRuntimeUpdate),
  ])
}

async function checkApplicationUpdate() {
  const info = await checkAppUpdate()
  if (!info.available) return
  useUiStore().recordImportantEvent({
    title: i18n.global.t('events.appUpdateAvailableTitle'),
    titleKey: 'events.appUpdateAvailableTitle',
    message: i18n.global.t('events.updateAvailableMessage', {
      currentVersion: displayVersion(info.currentVersion),
      latestVersion: displayVersion(info.latestVersion),
    }),
    messageKey: 'events.updateAvailableMessage',
    messageParams: {
      currentVersion: displayVersion(info.currentVersion),
      latestVersion: displayVersion(info.latestVersion),
    },
    tone: 'info',
    source: 'update:app',
  })
}

async function checkRuntimeUpdate(item: RuntimeCheck) {
  const state = useProviderRuntimeEnvironmentState(item.providerId)
  const result = await item.check()
  state.updateStatus.value = result
  if (!result.updateAvailable) return
  useUiStore().recordImportantEvent({
    title: i18n.global.t('events.runtimeUpdateAvailableTitle', { runtime: result.runtime }),
    titleKey: 'events.runtimeUpdateAvailableTitle',
    titleParams: { runtime: result.runtime },
    message: i18n.global.t('events.updateAvailableMessage', {
      currentVersion: displayVersion(result.currentVersion),
      latestVersion: displayVersion(result.latestVersion),
    }),
    messageKey: 'events.updateAvailableMessage',
    messageParams: {
      currentVersion: displayVersion(result.currentVersion),
      latestVersion: displayVersion(result.latestVersion),
    },
    tone: 'info',
    source: `update:${item.providerId}`,
  })
}

function displayVersion(value?: string | null) {
  return value?.trim() || '-'
}
