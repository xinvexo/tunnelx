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
        <TxSelect v-model="draft.proto" :options="protoOptions" :aria-label="t('providerForm.protocol')" />
      </SettingsRow>
      <SettingsRow :label="t('providerForm.localAddress')">
        <input v-model="draft.addr" class="tx-input mono" placeholder="localhost:8080" />
      </SettingsRow>
      <SettingsRow :label="t('providerForm.hostname')">
        <input v-model="draft.hostname" class="tx-input mono" :placeholder="t('providerForm.optional')" />
      </SettingsRow>
      <SettingsRow :label="t('providerForm.tcpAddress')">
        <input v-model="draft.remoteAddr" class="tx-input mono" :placeholder="t('providerForm.optional')" />
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
import { newCpolarEndpoint, type CpolarEndpoint } from '../domain'

const props = defineProps<{ modelValue: boolean; source: CpolarEndpoint | null }>()
const emit = defineEmits<{ 'update:modelValue': [boolean]; save: [CpolarEndpoint] }>()
const ui = useUiStore()
const { t } = useI18n()
const draft = ref<CpolarEndpoint | null>(null)
const protoOptions = [
  { label: 'HTTP', value: 'http' },
  { label: 'TCP', value: 'tcp' },
]

watch(() => props.modelValue, (open) => {
  if (!open) return
  draft.value = props.source ? clone(props.source) : newCpolarEndpoint()
})

function close(value: boolean) {
  emit('update:modelValue', value)
}

function submit() {
  if (!draft.value) return
  draft.value.name = draft.value.name.trim()
  draft.value.addr = draft.value.addr.trim()
  draft.value.hostname = draft.value.hostname.trim()
  draft.value.remoteAddr = draft.value.remoteAddr.trim()
  if (!draft.value.name || !draft.value.addr) {
    ui.notify(t('providerForm.nameAndLocalRequired'), 'warning')
    return
  }
  emit('save', toRaw(draft.value))
  close(false)
}
</script>
