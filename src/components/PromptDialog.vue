<template>
  <TxDialog
    :model-value="!!ui.promptRequest"
    :title="ui.promptRequest?.title ?? ''"
    @update:model-value="(open: boolean) => { if (!open) ui.resolvePrompt(null) }"
  >
    <p v-if="ui.promptRequest?.message" class="prompt-message">{{ ui.promptRequest.message }}</p>
    <input
      ref="inputRef"
      v-model="value"
      class="tx-input"
      :placeholder="ui.promptRequest?.placeholder"
      @keydown.enter.prevent="submit"
    />
    <template #footer>
      <TxButton @click="ui.resolvePrompt(null)">
        {{ ui.promptRequest?.cancelLabel ?? t('common.cancel') }}
      </TxButton>
      <TxButton tone="primary" :disabled="!value.trim()" @click="submit">
        {{ ui.promptRequest?.confirmLabel ?? t('common.confirm') }}
      </TxButton>
    </template>
  </TxDialog>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import TxDialog from '@/components/ui/TxDialog.vue'
import TxButton from '@/components/ui/TxButton.vue'
import { useUiStore } from '@/stores/ui'

const { t } = useI18n()
const ui = useUiStore()
const value = ref('')
const inputRef = ref<HTMLInputElement | null>(null)

// 每次新请求到来时填入默认值并聚焦选中，方便直接覆盖输入。
watch(
  () => ui.promptRequest,
  (request) => {
    if (!request) return
    value.value = request.defaultValue ?? ''
    nextTick(() => {
      inputRef.value?.focus()
      inputRef.value?.select()
    })
  },
)

function submit() {
  const trimmed = value.value.trim()
  if (!trimmed) return
  ui.resolvePrompt(trimmed)
}
</script>

<style scoped>
.prompt-message {
  margin: 0 0 12px;
  color: var(--tx-text-secondary);
  font-size: 13px;
  line-height: 1.6;
}
</style>
