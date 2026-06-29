<template>
  <DialogRoot :open="modelValue" @update:open="emit('update:modelValue', $event)">
    <DialogPortal>
      <DialogOverlay class="fixed inset-0 z-80 bg-[rgba(15,23,42,0.16)]" />
      <DialogContent
        class="fixed bottom-8px right-8px top-46px z-81 flex max-w-[calc(100vw-32px)] flex-col overflow-hidden border border-tx-border rounded-[var(--tx-radius-lg)] bg-[var(--tx-bg-app)] shadow-[0_18px_48px_rgba(15,23,42,0.16)]"
        :class="widthClass"
      >
        <header class="flex flex-none items-center justify-between gap-16px border-b border-tx-border bg-tx-surface px-16px py-13px">
          <div class="min-w-0">
            <DialogTitle class="m-0 text-15px font-650 text-[var(--tx-text-primary)]">{{ title }}</DialogTitle>
            <DialogDescription v-if="description" class="m-0 mt-3px text-12px text-[var(--tx-text-secondary)]">
              {{ description }}
            </DialogDescription>
          </div>
          <div class="flex flex-none items-center gap-8px">
            <slot name="header-action" />
            <TxIconButton icon="lucide:x" :label="closeLabel || t('ui.dialogClose')" @click="emit('update:modelValue', false)" />
          </div>
        </header>

        <div class="min-h-0 flex-1 overflow-y-auto p-16px">
          <slot />
        </div>

        <footer v-if="$slots.footer" class="flex flex-none items-center justify-end gap-16px border-t border-tx-border bg-tx-surface px-16px py-13px">
          <slot name="footer" />
        </footer>
      </DialogContent>
    </DialogPortal>
  </DialogRoot>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import {
  DialogContent,
  DialogDescription,
  DialogOverlay,
  DialogPortal,
  DialogRoot,
  DialogTitle,
} from 'reka-ui'
import TxIconButton from './TxIconButton.vue'

const props = withDefaults(defineProps<{
  modelValue: boolean
  title: string
  description?: string
  closeLabel?: string
  width?: 'md' | 'lg'
}>(), {
  description: '',
  closeLabel: '',
  width: 'md',
})

const emit = defineEmits<{ 'update:modelValue': [boolean] }>()
const { t } = useI18n()
const widthClass = computed(() => props.width === 'lg' ? 'w-580px' : 'w-520px')
</script>
