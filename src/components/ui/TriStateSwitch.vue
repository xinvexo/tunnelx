<template>
  <TxSegmentedControl v-model="current" :options="options" />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import TxSegmentedControl from './TxSegmentedControl.vue'

const { t } = useI18n()
const props = defineProps<{ modelValue: boolean | null }>()
const emit = defineEmits<{ 'update:modelValue': [boolean | null] }>()
const options = computed(() => [
  { label: t('ui.triDefault'), value: 'default' },
  { label: t('ui.triOn'), value: 'on' },
  { label: t('ui.triOff'), value: 'off' },
])
const current = computed({
  get: () => props.modelValue == null ? 'default' : props.modelValue ? 'on' : 'off',
  set: (value: string) => {
    emit('update:modelValue', value === 'default' ? null : value === 'on')
  },
})
</script>
