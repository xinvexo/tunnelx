<template>
  <span class="credential-status" :class="authenticated ? 'ok' : 'idle'">
    {{ authenticated ? t('providerCredentials.authenticated') : t('providerCredentials.unauthenticated') }}
  </span>
</template>

<script setup lang="ts">
import { useI18n } from 'vue-i18n'

defineProps<{
  authenticated: boolean
}>()

const { t } = useI18n()
</script>

<style scoped>
.credential-status {
  width: max-content;
  max-width: 100%;
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  padding: 3px 8px;
  background: var(--tx-bg-muted);
  color: var(--tx-text-secondary);
  font-size: 12px;
  white-space: nowrap;
}

.credential-status.ok {
  background: rgba(22, 163, 74, 0.1);
  color: #15803d;
}

.credential-status.idle {
  color: var(--tx-text-muted);
}
</style>
