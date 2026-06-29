<template>
  <div class="string-list" :class="{ compact }">
    <div class="items">
      <div v-for="(_, index) in rows" :key="index" class="item">
        <input
          ref="inputs"
          class="tx-input mono"
          :value="rows[index]"
          :placeholder="index === 0 ? placeholder : ''"
          @input="updateItem(index, $event)"
          @blur="trimItem(index)"
          @keydown.enter.prevent="addAfter(index)"
          @paste="pasteItems(index, $event)"
        />
        <div class="actions">
          <TxIconButton
            v-if="canRemove(index)"
            icon="lucide:x"
            :label="t('common.delete')"
            @click="remove(index)"
          />
          <TxIconButton
            v-if="index === rows.length - 1"
            icon="lucide:plus"
            :label="t('common.add')"
            primary
            @click="addAfter(index)"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import TxIconButton from './TxIconButton.vue'

const props = defineProps<{ modelValue: string[]; placeholder?: string; compact?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [string[]] }>()
const { t } = useI18n()
const rows = ref<string[]>([''])
const inputs = ref<HTMLInputElement[]>([])

watch(() => props.modelValue, syncRows, { immediate: true })

function clean(values: string[]) {
  return values.map((line) => line.trim()).filter(Boolean)
}
function sameList(left: string[], right: string[]) {
  return left.length === right.length && left.every((item, index) => item === right[index])
}
function syncRows(value: string[]) {
  const next = clean(value ?? [])
  if (rows.value.length && sameList(clean(rows.value), next)) return
  rows.value = next.length ? [...next] : ['']
}
function commit() {
  emit('update:modelValue', clean(rows.value))
}
function updateItem(index: number, event: Event) {
  rows.value[index] = (event.target as HTMLInputElement).value
  commit()
}
function canRemove(index: number) {
  return rows.value.length > 1 || !!rows.value[index]?.trim()
}
function focusRow(index: number) {
  nextTick(() => inputs.value[index]?.focus())
}
function addAfter(index: number) {
  const emptyIndex = rows.value.findIndex((item) => !item.trim())
  if (emptyIndex >= 0) {
    focusRow(emptyIndex)
    return
  }
  rows.value.splice(index + 1, 0, '')
  focusRow(index + 1)
}
function pasteItems(index: number, event: ClipboardEvent) {
  const text = event.clipboardData?.getData('text') ?? ''
  const pasted = clean(text.split(/\r?\n/))
  if (pasted.length <= 1) return
  event.preventDefault()
  rows.value.splice(index, 1, ...pasted)
  commit()
  focusRow(index + pasted.length - 1)
}
function trimItem(index: number) {
  rows.value[index] = rows.value[index]?.trim() ?? ''
  if (rows.value.length > 1) rows.value = clean(rows.value)
  if (!rows.value.length) rows.value = ['']
  commit()
}
function remove(index: number) {
  if (rows.value.length === 1) rows.value = ['']
  else rows.value.splice(index, 1)
  if (!rows.value.length) rows.value = ['']
  commit()
}
</script>

<style scoped>
.string-list {
  width: 100%;
  display: grid;
  gap: 6px;
}
.items {
  min-width: 0;
  display: grid;
  gap: 6px;
}
.item {
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  gap: 6px;
}
.actions {
  min-width: 28px;
  display: inline-flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
}
</style>
