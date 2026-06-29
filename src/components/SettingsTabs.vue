<template>
  <nav class="tabs">
    <RouterLink to="/settings/general">{{ t('settings.tabs.general') }}</RouterLink>
    <RouterLink to="/settings/versions">{{ t('settings.tabs.versions') }}</RouterLink>
    <RouterLink to="/settings/about">{{ t('settings.tabs.about') }}</RouterLink>
  </nav>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

<style scoped>
.tabs {
  display: flex;
  gap: 4px;
  margin-bottom: 18px;
  padding: 3px;
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: #eef0f3;
}
.tabs a {
  height: 28px;
  flex: 1;
  border-radius: var(--tx-radius-sm);
  color: var(--tx-text-secondary);
  font-weight: 500;
  display: flex;
  align-items: center;
  justify-content: center;
  text-decoration: none;
  transition: background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.tabs a:hover:not(.router-link-active),
.tabs a:focus-visible:not(.router-link-active) {
  background: var(--tx-bg-hover);
  color: var(--tx-text-primary);
}
.tabs a:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.tabs a.router-link-active {
  background: var(--tx-bg-surface);
  color: var(--tx-text-primary);
  box-shadow: 0 1px 2px rgba(15, 23, 42, 0.1);
}
</style>
