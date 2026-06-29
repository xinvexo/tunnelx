<template>
  <template v-if="plugin.type === 'http_proxy'">
    <SettingsRow :label="t('plugin.httpUser')"><input v-model="plugin.httpUser" class="tx-input" /></SettingsRow>
    <SettingsRow :label="t('plugin.httpPassword')"><input v-model="plugin.httpPassword" class="tx-input" type="password" /></SettingsRow>
  </template>
  <template v-else-if="plugin.type === 'socks5'">
    <SettingsRow :label="t('plugin.username')"><input v-model="plugin.username" class="tx-input" /></SettingsRow>
    <SettingsRow :label="t('plugin.password')"><input v-model="plugin.password" class="tx-input" type="password" /></SettingsRow>
  </template>
  <template v-else-if="plugin.type === 'static_file'">
    <SettingsRow :label="t('plugin.localPath')"><input v-model="plugin.localPath" class="tx-input mono" /></SettingsRow>
    <SettingsRow :label="t('plugin.stripPrefix')"><input v-model="plugin.stripPrefix" class="tx-input mono" /></SettingsRow>
    <SettingsRow :label="t('plugin.httpUser')"><input v-model="plugin.httpUser" class="tx-input" /></SettingsRow>
    <SettingsRow :label="t('plugin.httpPassword')"><input v-model="plugin.httpPassword" class="tx-input" type="password" /></SettingsRow>
  </template>
  <template v-else-if="plugin.type === 'unix_domain_socket'">
    <SettingsRow :label="t('plugin.unixPath')"><input v-model="plugin.unixPath" class="tx-input mono" /></SettingsRow>
  </template>
  <template v-else-if="hasLocalAddr">
    <SettingsRow :label="t('plugin.localAddr')"><input v-model="plugin.localAddr" class="tx-input mono" /></SettingsRow>
    <SettingsRow v-if="'hostHeaderRewrite' in plugin" :label="t('plugin.hostRewrite')">
      <input v-model="plugin.hostHeaderRewrite" class="tx-input" />
    </SettingsRow>
    <template v-if="'crtPath' in plugin">
      <SettingsRow :label="t('plugin.crtPath')"><input v-model="plugin.crtPath" class="tx-input mono" /></SettingsRow>
      <SettingsRow :label="t('plugin.keyPath')"><input v-model="plugin.keyPath" class="tx-input mono" /></SettingsRow>
    </template>
  </template>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import type { PluginOptions } from '@/providers/frp/domain/plugin'

const { t } = useI18n()
const props = defineProps<{ modelValue: PluginOptions }>()
const plugin = computed<any>(() => props.modelValue)
const hasLocalAddr = computed(() => 'localAddr' in plugin.value)
</script>
