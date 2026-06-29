<template>
  <header class="connection-bar">
    <div class="identity">
      <h1>{{ title }}</h1>
      <p v-if="subtitle" class="subtitle">{{ subtitle }}</p>
    </div>
    <nav v-if="id" class="tabs">
      <RouterLink :to="`${resolvedBasePath}/overview`">{{ t('topbar.overview') }}</RouterLink>
      <RouterLink :to="`${resolvedBasePath}/tunnels`">{{ t('topbar.tunnels') }}</RouterLink>
      <RouterLink :to="`${resolvedBasePath}/logs`">{{ t('topbar.logs') }}</RouterLink>
      <RouterLink :to="`${resolvedBasePath}/settings`">{{ t('topbar.settings') }}</RouterLink>
    </nav>
    <div v-else class="tabs-spacer" />
    <div class="actions"><slot name="actions" /></div>
  </header>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'
import { computed } from 'vue'

const props = defineProps<{ title: string; subtitle?: string; id?: string; basePath?: string }>()
const { t } = useI18n()
const resolvedBasePath = computed(() => props.basePath ?? `/connections/${props.id}`)
</script>

<style scoped>
.connection-bar {
  min-height: 62px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--tx-border-subtle);
  background: var(--tx-bg-app);
  /* identity | tabs(居中) | actions —— 同一行 */
  display: grid;
  grid-template-columns: minmax(160px, 1fr) auto minmax(160px, 1fr);
  align-items: stretch;
  gap: 16px;
  padding: 0 24px;
}
.identity {
  min-width: 0;
  align-self: center;
  display: flex;
  flex-direction: column;
  justify-content: center;
}
.identity h1 {
  margin: 0;
  font-size: 15px;
  font-weight: 650;
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.subtitle {
  margin: 2px 0 0;
  color: var(--tx-text-secondary);
  font-size: 12px;
  line-height: 1.3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* tab 与标题/操作同处一行，且文字靠近底部分隔线，激活下划线压在分隔线上。 */
.tabs,
.tabs-spacer { min-width: 0; }
.tabs {
  display: flex;
  align-items: stretch;
  gap: 20px;
}
.tabs a {
  position: relative;
  display: flex;
  align-items: flex-end;
  padding-bottom: 9px;
  color: var(--tx-text-secondary);
  font-weight: 500;
  line-height: 1;
  text-decoration: none;
  transition: color 120ms ease;
}
.tabs a:hover,
.tabs a:focus-visible {
  color: var(--tx-text-primary);
}
.tabs a:focus-visible {
  outline: none;
}
.tabs a.router-link-active { color: var(--tx-text-primary); }
.tabs a.router-link-active::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 2px;
  background: var(--tx-accent);
}
.actions {
  justify-self: end;
  align-self: center;
  display: flex;
  align-items: center;
  gap: 12px;
}

@media (max-width: 900px) {
  .connection-bar { gap: 12px; }
  .tabs { gap: 14px; }
  .actions { gap: 7px; }
}
@media (max-width: 760px) {
  .connection-bar { padding-left: 16px; padding-right: 16px; }
}
</style>
