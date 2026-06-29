<template>
  <AppShell />
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import AppShell from '@/components/AppShell.vue'
import { useSettingsStore } from '@/stores/settings'
import { useConnectionStore } from '@/providers/connections'
import { initFrontendProviders } from '@/providers/registry'
import { setTrayLocale } from '@/api/settings'
import { currentLocale } from '@/i18n'
import { runStartupUpdateCheck } from '@/services/startupUpdateCheck'

// 启动时注册事件监听并做首次加载。
onMounted(() => {
  void useSettingsStore().init().catch(console.error)
  void initFrontendProviders().catch(console.error)
  void useConnectionStore().init().catch(console.error)
  void setTrayLocale(currentLocale()).catch(console.error)
  void runStartupUpdateCheck().catch(console.error)
})
</script>
