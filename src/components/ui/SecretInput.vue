<template>
  <div class="secret" :class="{ mono }">
    <input
      :value="modelValue"
      :type="revealed ? 'text' : 'password'"
      :placeholder="placeholder"
      class="secret-input"
      @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
    <TxTooltip :text="revealed ? t('ui.secretHide') : t('ui.secretShow')">
      <button
        type="button"
        class="icon"
        :aria-label="revealed ? t('ui.secretHide') : t('ui.secretShow')"
        @click="revealed = !revealed"
      >
        <Icon :icon="revealed ? 'lucide:eye-off' : 'lucide:eye'" />
      </button>
    </TxTooltip>
    <TxTooltip :text="t('ui.secretCopy')">
      <button type="button" class="icon" :aria-label="t('ui.secretCopy')" :disabled="!modelValue" @click="copy">
        <Icon icon="lucide:copy" />
      </button>
    </TxTooltip>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { Icon } from '@iconify/vue'
import { useI18n } from 'vue-i18n'
import { useClipboard } from '@/composables/useClipboard'
import TxTooltip from './TxTooltip.vue'

const props = defineProps<{ modelValue: string; placeholder?: string; mono?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [string] }>()
const { t } = useI18n()
const { copyText } = useClipboard()
const revealed = ref(false)

async function copy() {
  if (!props.modelValue) return
  await copyText(props.modelValue, t('ui.secretCopied'))
}
</script>

<style scoped>
.secret {
  width: 100%;
  min-width: 0;
  height: 32px;
  border: 1px solid var(--tx-border-strong);
  border-radius: var(--tx-radius-sm);
  background: var(--tx-bg-surface);
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 4px 0 9px;
  transition: border-color 120ms ease, box-shadow 120ms ease;
}
.secret:focus-within {
  border-color: var(--tx-accent);
  box-shadow: 0 0 0 3px var(--tx-focus-ring);
}
.secret-input {
  flex: 1;
  min-width: 0;
  height: 100%;
  border: 0;
  outline: 0;
  background: transparent;
  color: var(--tx-text-primary);
  font: inherit;
}
.secret.mono .secret-input { font-family: var(--tx-mono); }
.secret-input::placeholder { color: var(--tx-text-muted); }
.icon {
  flex-shrink: 0;
  width: 24px;
  height: 24px;
  border: 0;
  border-radius: var(--tx-radius-sm);
  background: transparent;
  color: var(--tx-text-muted);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.icon:hover:not(:disabled),
.icon:focus-visible:not(:disabled) { background: var(--tx-bg-hover); color: var(--tx-text-primary); }
.icon:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.icon:disabled { opacity: 0.4; cursor: not-allowed; }
.icon svg { width: 14px; height: 14px; }
</style>
