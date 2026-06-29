<template>
  <WorkspacePage wide :title="t('about.title')" :description="t('about.description')">
    <SettingsTabs />
    <SettingsGroup :title="t('about.group')">
      <SettingsRow :label="t('about.version.label')" :desc="version ? t('about.version.desc', { version }) : t('common.loading')">
        <span class="version">{{ version ? `v${version}` : '—' }}</span>
      </SettingsRow>
      <SettingsRow :label="t('about.repo.label')" :desc="t('about.repo.desc')">
        <TxButton size="sm" icon="lucide:github" @click="openRepo">GitHub</TxButton>
      </SettingsRow>
      <SettingsRow :label="t('about.providers.label')" :desc="t('about.providers.desc')">
        <div class="provider-links">
          <TxButton
            v-for="provider in providerLinks"
            :key="provider.descriptor.id"
            size="sm"
            :icon="provider.icon"
            :icon-asset="provider.iconAsset"
            @click="openExternal(provider.homepageUrl)"
          >
            {{ provider.descriptor.name }}
          </TxButton>
        </div>
      </SettingsRow>
    </SettingsGroup>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@tauri-apps/plugin-opener'
import SettingsTabs from '@/components/SettingsTabs.vue'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import TxButton from '@/components/ui/TxButton.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { appVersion } from '@/api/update'
import { frontendProviderModules } from '@/providers/registry'
import { useUiStore } from '@/stores/ui'

const REPO_URL = 'https://github.com/xinvexo/tunnelx'

const { t } = useI18n()
const ui = useUiStore()
const version = ref('')
const providerLinks = computed(() => frontendProviderModules().filter((provider) => provider.homepageUrl))

onMounted(async () => {
  try {
    version.value = await appVersion()
  } catch (error) {
    ui.notify(String(error), 'danger')
  }
})

async function openRepo() {
  await openExternal(REPO_URL)
}

async function openExternal(url?: string) {
  if (!url) return
  try {
    await openUrl(url)
  } catch (error) {
    ui.notify(t('about.openLinkFailed', { error: String(error) }), 'danger')
  }
}
</script>

<style scoped>
.version {
  font-family: var(--tx-mono);
  color: var(--tx-text-primary);
}

.provider-links {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
</style>
