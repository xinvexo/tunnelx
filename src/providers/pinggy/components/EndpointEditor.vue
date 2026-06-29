<template>
  <TxDrawer
    :model-value="modelValue"
    :title="source ? t('providerForm.editTunnel') : t('providerForm.createTunnel')"
    @update:model-value="close"
  >
    <SettingsGroup v-if="draft" :title="t('providerForm.basic')">
      <SettingsRow :label="t('providerForm.name')">
        <input v-model="draft.name" class="tx-input mono" />
      </SettingsRow>
      <SettingsRow :label="t('providerForm.protocol')">
        <TxSelect v-model="draft.tunnelType" :options="typeOptions" :aria-label="t('providerForm.protocol')" />
      </SettingsRow>
      <SettingsRow :label="t('providerForm.localAddress')">
        <input v-model="draft.localAddr" class="tx-input mono" placeholder="http://localhost:8080" />
      </SettingsRow>
      <SettingsRow :label="t('providerForm.enabled')">
        <TxSwitch v-model="draft.enabled" />
      </SettingsRow>
    </SettingsGroup>

    <template #footer>
      <TxButton @click="close(false)">{{ t('common.cancel') }}</TxButton>
      <TxButton tone="primary" @click="submit">{{ t('common.save') }}</TxButton>
    </template>
  </TxDrawer>
</template>

<script setup lang="ts">
import { ref, toRaw, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import SettingsGroup from '@/components/ui/SettingsGroup.vue'
import SettingsRow from '@/components/ui/SettingsRow.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxDrawer from '@/components/ui/TxDrawer.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import TxSwitch from '@/components/ui/TxSwitch.vue'
import { clone } from '@/domain/util'
import { useUiStore } from '@/stores/ui'
import { newPinggyEndpoint, type PinggyEndpoint } from '../domain'

const props = defineProps<{ modelValue: boolean; source: PinggyEndpoint | null }>()
const emit = defineEmits<{ 'update:modelValue': [boolean]; save: [PinggyEndpoint] }>()
const ui = useUiStore()
const { t } = useI18n()
const draft = ref<PinggyEndpoint | null>(null)
const typeOptions = [
  { label: 'HTTP', value: 'http' },
  { label: 'TCP', value: 'tcp' },
  { label: 'UDP', value: 'udp' },
  { label: 'TLS', value: 'tls' },
  { label: 'TLSTCP', value: 'tlstcp' },
]

watch(() => props.modelValue, (open) => {
  if (!open) return
  draft.value = props.source ? clone(props.source) : newPinggyEndpoint()
})

function close(value: boolean) {
  emit('update:modelValue', value)
}

function submit() {
  if (!draft.value) return
  draft.value.name = draft.value.name.trim()
  draft.value.localAddr = draft.value.localAddr.trim()
  if (!draft.value.name || !draft.value.localAddr) {
    ui.notify(t('providerForm.nameAndLocalRequired'), 'warning')
    return
  }
  emit('save', toRaw(draft.value))
  close(false)
}
</script>
