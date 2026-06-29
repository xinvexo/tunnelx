<template>
  <div class="editor">
    <div v-for="(item, index) in items" :key="index" class="row">
      <input v-model="item.key" class="tx-input mono" :placeholder="t('ui.kvKey')" @blur="commit" />
      <span>=</span>
      <input v-model="item.value" class="tx-input mono" :placeholder="t('ui.kvValue')" @blur="commit" />
      <TxIconButton icon="lucide:x" :label="t('ui.kvDelete')" @click="remove(index)" />
    </div>
    <TxButton size="sm" tone="ghost" icon="lucide:plus" @click="add">{{ t('ui.kvAdd') }}</TxButton>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import TxButton from './TxButton.vue'
import TxIconButton from './TxIconButton.vue'

const { t } = useI18n()

const props = defineProps<{ modelValue: Record<string, string> | null }>()
const emit = defineEmits<{ 'update:modelValue': [Record<string, string> | null] }>()
const items = ref<{ key: string; value: string }[]>([])

watch(() => props.modelValue, (value) => {
  items.value = Object.entries(value ?? {}).map(([key, itemValue]) => ({ key, value: String(itemValue) }))
}, { immediate: true })

function commit() {
  const result: Record<string, string> = {}
  for (const item of items.value) if (item.key.trim()) result[item.key.trim()] = item.value
  emit('update:modelValue', Object.keys(result).length ? result : null)
}
function add() { items.value.push({ key: '', value: '' }) }
function remove(index: number) { items.value.splice(index, 1); commit() }
</script>

<style scoped>
.editor { width: 100%; display: grid; gap: 6px; }
.row { display: grid; grid-template-columns: 1fr 12px 1fr 28px; align-items: center; gap: 6px; }
.row span { color: var(--tx-text-muted); text-align: center; }
</style>
