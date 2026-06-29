<template>
  <WorkspacePage wide :title="t('settings.title')" :description="t('settings.description')">
    <SettingsTabs />
    <SettingsGroup :title="t('settings.groups.startup')">
      <SettingsRow compact :label="t('settings.autostart.label')" :desc="t('settings.autostart.desc')">
        <TxSwitch v-model="autostart" :disabled="busy" @update:model-value="setAutostart" />
      </SettingsRow>
      <SettingsRow compact :label="t('settings.silentStart.label')" :desc="t('settings.silentStart.desc')">
        <TxSwitch v-model="silentStart" @update:model-value="persist" />
      </SettingsRow>
      <SettingsRow compact :label="t('settings.autoConnect.label')" :desc="t('settings.autoConnect.desc')">
        <TxSwitch v-model="autoConnect" @update:model-value="persist" />
      </SettingsRow>
    </SettingsGroup>
    <SettingsGroup :title="t('settings.groups.window')">
      <SettingsRow compact :label="t('settings.lightweight.label')" :desc="t('settings.lightweight.desc')">
        <TxSwitch v-model="lightweightMode" @update:model-value="persist" />
      </SettingsRow>
      <SettingsRow compact :label="t('settings.closeWindow.label')" :desc="t('settings.closeWindow.desc')" />
      <SettingsRow compact :label="t('settings.language.label')" :desc="t('settings.language.desc')">
        <TxSelect v-model="locale" :options="localeOptions" @update:model-value="onLocaleChange" />
      </SettingsRow>
    </SettingsGroup>
    <SettingsGroup :title="t('settings.groups.performance')">
      <SettingsRow compact :label="t('settings.trafficStats.label')" :desc="t('settings.trafficStats.desc')">
        <TxSwitch v-model="trafficStatsEnabled" @update:model-value="persist" />
      </SettingsRow>
    </SettingsGroup>
    <SettingsGroup :title="t('settings.groups.update')">
      <SettingsRow compact :label="t('settings.autoUpdate.label')" :desc="t('settings.autoUpdate.desc')">
        <TxSwitch v-model="autoUpdate" @update:model-value="persist" />
      </SettingsRow>
      <SettingsRow compact :label="updateLabel" :desc="updateDesc">
        <TxButton v-if="!updateAvailable" size="sm" :loading="checking" @click="checkUpdate">
          {{ t('settings.update.check') }}
        </TxButton>
        <TxButton v-else tone="primary" size="sm" :loading="installing" @click="installUpdate">
          {{ t('settings.update.install') }}
        </TxButton>
      </SettingsRow>
    </SettingsGroup>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { storeToRefs } from 'pinia'
import { listen } from '@tauri-apps/api/event'
import { disable, enable, isEnabled } from '@tauri-apps/plugin-autostart'
import SettingsTabs from '@/components/SettingsTabs.vue'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import TxSwitch from '@/components/ui/TxSwitch.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { setTrayLocale } from '@/api/settings'
import { checkUpdate as apiCheckUpdate, installUpdate as apiInstallUpdate, type AppUpdateProgress } from '@/api/update'
import { LOCALE_OPTIONS, currentLocale, setLocale, type Locale } from '@/i18n'
import { useSettingsStore } from '@/stores/settings'
import { useUiStore } from '@/stores/ui'

const { t } = useI18n()
const ui = useUiStore()
const settings = useSettingsStore()
const {
  silentStart,
  autoConnect,
  lightweightMode,
  autoUpdate,
  trafficStatsEnabled,
} = storeToRefs(settings)

const locale = ref<Locale>(currentLocale())
const localeOptions = LOCALE_OPTIONS
function onLocaleChange(value: string) {
  setLocale(value as Locale)
  locale.value = value as Locale
  void setTrayLocale(value).catch(console.error)
}
const autostart = ref(false)
const busy = ref(false)

const checking = ref(false)
const installing = ref(false)
const updateProgress = ref<AppUpdateProgress | null>(null)
const updateAvailable = ref(false)
const latestVersion = ref('')
const updateNotes = ref<string | null>(null)
let unlistenUpdateProgress: (() => void) | null = null
let unmounted = false

const updateLabel = computed(() =>
  updateAvailable.value ? t('settings.update.found', { version: latestVersion.value }) : t('settings.update.checkLabel'),
)
const updateDesc = computed(() => {
  if (installing.value) {
    const percent = updateProgress.value?.percent
    return typeof percent === 'number'
      ? t('settings.update.downloadingProgress', { percent: Math.floor(percent) })
      : t('settings.update.downloading')
  }
  if (updateAvailable.value) return updateNotes.value || t('settings.update.foundDesc')
  return t('settings.update.checkDesc')
})

onMounted(async () => {
  try {
    await settings.init()
    autostart.value = await isEnabled()
  } catch (error) {
    ui.notify(String(error), 'danger')
  }
  const unlisten = await listen<AppUpdateProgress>('app-update-progress', (event) => {
    updateProgress.value = event.payload
  }).catch(() => null)
  // The component may have unmounted while listen() was pending; don't leak the handler.
  if (unmounted) unlisten?.()
  else unlistenUpdateProgress = unlisten
})

onBeforeUnmount(() => {
  unmounted = true
  unlistenUpdateProgress?.()
  unlistenUpdateProgress = null
})

async function setAutostart(value: boolean) {
  busy.value = true
  try {
    if (value) await enable()
    else await disable()
  } catch (error) {
    autostart.value = !value
    ui.notify(t('settings.autostartFailed', { error: String(error) }), 'danger')
  } finally {
    busy.value = false
  }
}
async function persist() {
  try {
    await settings.saveCurrent()
    ui.notify(t('settings.saved'), 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
  }
}

async function checkUpdate() {
  checking.value = true
  try {
    const info = await apiCheckUpdate()
    if (info.available) {
      updateAvailable.value = true
      latestVersion.value = info.latestVersion
      updateNotes.value = info.notes
      ui.notify(t('settings.update.found', { version: info.latestVersion }), 'success')
    } else {
      ui.notify(t('settings.update.latest', { version: info.currentVersion }), 'success')
    }
  } catch (error) {
    ui.notify(t('settings.update.checkFailed', { error: String(error) }), 'danger')
  } finally {
    checking.value = false
  }
}

async function installUpdate() {
  installing.value = true
  updateProgress.value = null
  try {
    ui.notify(t('settings.update.downloading'), 'info')
    await apiInstallUpdate()
    // apiInstallUpdate 成功会触发应用重启，此行代码通常不会执行。
    // 但如果重启被延迟或失败，至少重置标志让用户可以重试。
  } catch (error) {
    ui.notify(t('settings.update.installFailed', { error: String(error) }), 'danger')
  } finally {
    installing.value = false
  }
}
</script>
