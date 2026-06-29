<template>
  <WorkspacePage wide :title="t('environment.title')" :description="t('environment.description')">
    <SettingsTabs />
    <template v-if="panels.length">
      <div class="provider-tabs" role="tablist">
        <button
          v-for="panel in panels"
          :key="panel.id"
          type="button"
          role="tab"
          :aria-selected="activePanelId === panel.id"
          :class="{ active: activePanelId === panel.id }"
          @click="activePanelId = panel.id"
        >
          <ProviderIcon :provider-id="panel.id" />
          <span>{{ panel.name }}</span>
        </button>
      </div>

      <div class="environment-panel-host">
        <div
          v-for="panel in panels"
          :key="panel.id"
          v-show="activePanelId === panel.id"
          class="environment-panel"
        >
          <component :is="panel.component" />
        </div>
      </div>
    </template>
    <p v-else class="empty-copy">{{ t('environment.empty') }}</p>
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import SettingsTabs from '@/components/SettingsTabs.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import ProviderIcon from '@/providers/components/ProviderIcon.vue'
import { providerEnvironmentPanels } from '@/providers/registry'

const { t } = useI18n()
const route = useRoute()
const router = useRouter()
const panels = computed(() => providerEnvironmentPanels())
const activePanelId = ref('')

watch(panels, (next) => {
  if (!next.length) {
    activePanelId.value = ''
    return
  }
  const queryProvider = typeof route.query.provider === 'string' ? route.query.provider : ''
  if (queryProvider && next.some((panel) => panel.id === queryProvider)) {
    activePanelId.value = queryProvider
    return
  }
  if (!next.some((panel) => panel.id === activePanelId.value)) {
    activePanelId.value = next[0].id
  }
}, { immediate: true })

watch(() => route.query.provider, (value) => {
  const providerId = typeof value === 'string' ? value : ''
  if (providerId && panels.value.some((panel) => panel.id === providerId)) {
    activePanelId.value = providerId
  }
})

watch(activePanelId, (providerId) => {
  if (!providerId || route.path !== '/settings/versions') return
  if (route.query.provider === providerId) return
  void router.replace({ path: route.path, query: { ...route.query, provider: providerId } })
})
</script>

<style scoped>
.provider-tabs {
  display: flex;
  align-items: center;
  min-height: 35px;
  max-width: 100%;
  margin: -2px 0 14px;
  border-bottom: 1px solid var(--tx-border-subtle);
  gap: 18px;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
}

.provider-tabs::-webkit-scrollbar {
  display: none;
}

.provider-tabs button {
  position: relative;
  height: 34px;
  flex: 0 0 auto;
  border: 0;
  border-radius: 0;
  background: transparent;
  color: var(--tx-text-muted);
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 1px;
  cursor: pointer;
  font-weight: 600;
  line-height: 1;
  white-space: nowrap;
  transition: color 120ms ease;
}

.provider-tabs button::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 2px;
  border-radius: 999px;
  background: transparent;
}

.provider-tabs button:hover,
.provider-tabs button:focus-visible {
  color: var(--tx-text-primary);
}

.provider-tabs button:focus-visible {
  outline: none;
  border-radius: var(--tx-radius-sm);
  box-shadow: 0 0 0 3px var(--tx-focus-ring);
}

.provider-tabs button.active {
  color: var(--tx-text-primary);
}

.provider-tabs button.active::after {
  background: var(--tx-accent);
}

.provider-tabs svg,
.provider-tabs img {
  width: 14px;
  height: 14px;
  flex-shrink: 0;
}

.environment-panel-host {
  min-height: 360px;
  contain: layout;
  overflow-anchor: none;
}

.environment-panel {
  min-height: 360px;
}

.empty-copy {
  margin: 0;
  color: var(--tx-text-muted);
}

@media (max-width: 720px) {
  .provider-tabs {
    width: 100%;
    gap: 14px;
  }
}
</style>
