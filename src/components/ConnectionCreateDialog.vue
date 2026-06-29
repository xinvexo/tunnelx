<template>
  <TxDialog
    :model-value="modelValue"
    :title="t('connectionCreate.title')"
    :description="t('connectionCreate.dialogDesc')"
    @update:model-value="emit('update:modelValue', $event)"
  >
    <div class="form">
      <label class="field">
        <span>{{ t('connectionCreate.nameLabel') }}</span>
        <input
          ref="inputRef"
          v-model="name"
          class="tx-input"
          :placeholder="t('connectionCreate.namePlaceholder')"
          @keydown.enter.prevent.stop
        />
      </label>
      <label class="field">
        <span>{{ t('connectionCreate.typeLabel') }}</span>
        <TxSelect v-model="providerId" :aria-label="t('connectionCreate.typeLabel')" :options="providerOptions" />
      </label>
    </div>
    <template #footer>
      <TxButton @click="emit('update:modelValue', false)">{{ t('common.cancel') }}</TxButton>
      <TxButton tone="primary" :loading="creating" @click="submit">{{ t('connectionCreate.create') }}</TxButton>
    </template>
  </TxDialog>
</template>

<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import TxButton from '@/components/ui/TxButton.vue'
import TxDialog from '@/components/ui/TxDialog.vue'
import TxSelect from '@/components/ui/TxSelect.vue'
import { validateConnectionName } from '@/domain/connectionName'
import { useConnectionStore } from '@/providers/connections'
import type { TunnelResource } from '@/providers/contract'
import { defaultProviderId, frontendProviderDescriptors } from '@/providers/registry'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ modelValue: boolean }>()
const emit = defineEmits<{
  'update:modelValue': [boolean]
  created: [TunnelResource]
}>()

const { t } = useI18n()
const connections = useConnectionStore()
const ui = useUiStore()
const name = ref('')
const providerId = ref(defaultProviderId())
const creating = ref(false)
const inputRef = ref<HTMLInputElement | null>(null)

const providers = computed(() => connections.providers.length ? connections.providers : frontendProviderDescriptors())
const providerOptions = computed(() =>
    providers.value.map((provider) => ({
        label: provider.name,
        value: provider.id,
    })),
)

watch(() => props.modelValue, async (open) => {
    if (!open) return
    name.value = ''
    try {
        await connections.init()
    } catch {
        // Browser previews do not have a Tauri bridge.
    }
    if (!providers.value.some((provider) => provider.id === providerId.value)) {
        providerId.value = providers.value[0]?.id ?? defaultProviderId()
    }
    await nextTick()
    inputRef.value?.focus()
})

async function submit() {
    const validated = validateConnectionName(name.value)
    if (!validated.ok) {
        ui.notify(t(validated.messageKey), 'warning')
        return
    }
    creating.value = true
    try {
        const resource = await connections.create(providerId.value, validated.name)
        emit('update:modelValue', false)
        emit('created', resource)
    } catch (error) {
        ui.notify(String(error), 'danger')
    } finally {
        creating.value = false
    }
}

</script>

<style scoped>
.form {
  display: grid;
  gap: 14px;
}
.field {
  display: grid;
  gap: 7px;
}
.field span {
  color: var(--tx-text-secondary);
  font-size: 12px;
}
</style>
