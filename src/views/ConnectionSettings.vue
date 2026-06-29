<template>
  <component :is="panel" v-if="panel" />
  <WorkspacePage v-else wide>
    <p class="empty-copy">{{ t('providerSettings.notSupported') }}</p>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { useI18n } from 'vue-i18n'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { providerSettingsPanel } from '@/providers/registry'

const route = useRoute()
const { t } = useI18n()
const providerId = computed(() => String(route.params.providerId ?? ''))
const panel = computed(() => providerSettingsPanel(providerId.value))
</script>

<style scoped>
.empty-copy {
  margin: 0;
  color: var(--tx-text-muted);
}
</style>
