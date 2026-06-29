<template>
  <div class="sticky top-0 z-5 -mt-20px mb-16px flex min-h-32px items-center justify-between gap-16px border-b border-tx-border bg-[var(--tx-bg-app)] pb-14px pt-20px text-12px text-[var(--tx-text-muted)]">
    <span class="min-w-0 truncate">{{ countLabel }}</span>
    <div class="flex flex-none items-center justify-end gap-8px">
      <slot name="actions">
        <TxButton
          tone="primary"
          icon="lucide:plus"
          :loading="createLoading"
          :disabled="createDisabled"
          @click="emit('create')"
        >
          {{ createLabel }}
        </TxButton>
      </slot>
    </div>
  </div>

  <div v-if="rows.length" class="w-full min-w-0 overflow-hidden border border-tx-border rounded-[var(--tx-radius-md)] bg-tx-surface">
    <table class="w-full min-w-0 table-fixed border-collapse">
      <colgroup>
        <col class="w-28px" />
        <col :style="{ width: nameWidth }" />
        <col v-for="column in columns" :key="column.key" :style="column.width ? { width: column.width } : undefined" />
        <col class="w-120px" />
      </colgroup>
      <thead>
        <tr>
          <th class="h-31px border-b border-tx-border px-8px text-left text-10px text-[var(--tx-text-muted)] font-650 whitespace-nowrap" />
          <th class="h-31px border-b border-tx-border px-8px text-left text-10px text-[var(--tx-text-muted)] font-650 whitespace-nowrap">{{ nameLabel }}</th>
          <th
            v-for="column in columns"
            :key="column.key"
            class="h-31px border-b border-tx-border px-8px text-left text-10px text-[var(--tx-text-muted)] font-650 whitespace-nowrap"
          >
            {{ column.label }}
          </th>
          <th class="h-31px border-b border-tx-border px-8px text-left text-10px text-[var(--tx-text-muted)] font-650 whitespace-nowrap" />
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="(row, rowIndex) in rows"
          :key="row.id"
          class="provider-endpoint-row hover:bg-[var(--tx-bg-hover)] [&.tx-sort-lifted]:bg-tx-surface [&.tx-sort-lifted]:shadow-[0_8px_20px_rgba(15,23,42,0.12)]"
          @dblclick="emit('edit', row.id)"
        >
          <td :class="tableCellClass(rowIndex)">
            <span
              class="inline-flex h-24px w-18px flex-none cursor-grab items-center justify-center text-[var(--tx-text-muted)] active:cursor-grabbing"
              role="button"
              :aria-label="reorderLabel"
              @pointerdown="onDragPointerDown(row.id, $event)"
              @click.stop
              @dblclick.stop
            >
              <Icon class="h-14px w-14px" icon="lucide:grip-vertical" />
            </span>
          </td>
          <td :class="tableCellClass(rowIndex)">
            <div class="flex min-w-0 items-center gap-6px">
              <button
                class="mono min-w-0 flex-none cursor-pointer truncate border-0 bg-transparent p-0 text-left text-[var(--tx-text-primary)] font-550"
                :class="row.tag ? 'max-w-[calc(100%-48px)]' : 'max-w-full'"
                type="button"
                @click="emit('edit', row.id)"
              >
                {{ row.name || '-' }}
              </button>
              <span v-if="row.tag" class="flex-none text-10px text-[var(--tx-accent)] font-700 leading-none whitespace-nowrap">
                {{ row.tag }}
              </span>
            </div>
          </td>
          <td v-for="column in columns" :key="column.key" :class="tableCellClass(rowIndex)">
            <slot name="cell" :row="row" :column="column" :cell="cell(row, column.key)">
              <span
                v-if="cellPill(row, column.key)"
                class="-ml-8px inline-flex items-center rounded-full px-8px py-3px text-11px whitespace-nowrap"
                :class="pillClass(row, column.key)"
              >
                {{ cellText(row, column.key) }}
              </span>
              <span
                v-else
                class="block min-w-0 truncate text-[var(--tx-text-secondary)]"
                :class="{ mono: cellMono(row, column.key) }"
              >
                {{ cellText(row, column.key) }}
              </span>
            </slot>
          </td>
          <td :class="tableCellClass(rowIndex)">
            <div class="flex items-center justify-start gap-5px" @click.stop @dblclick.stop>
              <div class="inline-flex w-32px justify-center">
                <Icon v-if="row.saving" class="h-16px w-16px animate-spin text-[var(--tx-text-muted)]" icon="lucide:loader-circle" />
                <TxSwitch
                  v-else
                  :model-value="row.enabled"
                  :disabled="row.enableDisabled"
                  @update:model-value="emit('toggle', row.id, $event)"
                />
              </div>
              <TxIconButton icon="lucide:pencil" :label="editLabel" @click="emit('edit', row.id)" />
              <TxIconButton icon="lucide:trash-2" :label="removeLabel" danger @click="emit('remove', row.id)" />
            </div>
          </td>
        </tr>
      </tbody>
    </table>
  </div>

  <div v-else class="flex min-h-300px flex-col items-center justify-center border border-dashed border-[var(--tx-border-strong)] rounded-[var(--tx-radius-md)] text-center text-[var(--tx-text-secondary)]">
    <slot name="empty">
      <Icon class="h-30px w-30px text-[var(--tx-text-muted)]" icon="lucide:waypoints" />
      <h3 class="m-0 mt-12px text-15px text-[var(--tx-text-primary)] font-650">{{ emptyTitle }}</h3>
      <p v-if="emptyDescription" class="m-0 mt-6px">{{ emptyDescription }}</p>
    </slot>
  </div>
</template>

<script setup lang="ts">
import { Icon } from '@iconify/vue'
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import TxButton from '@/components/ui/TxButton.vue'
import TxIconButton from '@/components/ui/TxIconButton.vue'
import TxSwitch from '@/components/ui/TxSwitch.vue'
import { useSortable } from '@/composables/useSortable'

interface ProviderTunnelTableColumn {
  key: string
  label: string
  width?: string
}

interface ProviderTunnelTableCell {
  key: string
  text: string
  mono?: boolean
  pill?: boolean
  tone?: 'default' | 'success' | 'muted' | 'warning' | 'danger'
}

interface ProviderTunnelTableRow {
  id: string
  name: string
  tag?: string
  enabled: boolean
  saving?: boolean
  enableDisabled?: boolean
  cells: ProviderTunnelTableCell[]
}

const props = withDefaults(defineProps<{
  countLabel: string
  columns: ProviderTunnelTableColumn[]
  rows: ProviderTunnelTableRow[]
  createLabel?: string
  createLoading?: boolean
  createDisabled?: boolean
  nameLabel?: string
  nameWidth?: string
  emptyTitle?: string
  emptyDescription?: string
  editLabel?: string
  removeLabel?: string
  reorderLabel?: string
}>(), {
  createLabel: '',
  createLoading: false,
  createDisabled: false,
  nameLabel: '',
  nameWidth: '22%',
  emptyTitle: '',
  emptyDescription: '',
  editLabel: '',
  removeLabel: '',
  reorderLabel: '',
})

const emit = defineEmits<{
  create: []
  edit: [id: string]
  remove: [id: string]
  toggle: [id: string, enabled: boolean]
  reorder: [ids: string[]]
}>()

const { t } = useI18n()
const createLabel = computedLabel(() => props.createLabel, 'providerTunnel.create')
const nameLabel = computedLabel(() => props.nameLabel, 'providerTunnel.name')
const emptyTitle = computedLabel(() => props.emptyTitle, 'providerTunnel.empty')
const editLabel = computedLabel(() => props.editLabel, 'providerTunnel.edit')
const removeLabel = computedLabel(() => props.removeLabel, 'providerTunnel.remove')
const reorderLabel = computedLabel(() => props.reorderLabel, 'providerTunnel.reorder')

const sortable = useSortable({
  ids: () => props.rows.map((row) => row.id),
  itemSelector: '.provider-endpoint-row',
  handleItemSelector: '.provider-endpoint-row',
  onReorder: (ids) => emit('reorder', ids),
})

function tableCellClass(rowIndex: number) {
  return [
    'h-42px px-8px align-middle',
    rowIndex === props.rows.length - 1 ? 'border-b-0' : 'border-b border-tx-border',
  ]
}

function computedLabel(getter: () => string, key: string) {
  return computed(() => getter() || t(key))
}

function onDragPointerDown(id: string, event: PointerEvent) {
  sortable.start(id, event)
}

function cell(row: ProviderTunnelTableRow, key: string) {
  return row.cells.find((item) => item.key === key)
}

function cellText(row: ProviderTunnelTableRow, key: string) {
  return cell(row, key)?.text || '-'
}

function cellMono(row: ProviderTunnelTableRow, key: string) {
  return cell(row, key)?.mono === true
}

function cellPill(row: ProviderTunnelTableRow, key: string) {
  return cell(row, key)?.pill === true
}

function pillClass(row: ProviderTunnelTableRow, key: string) {
  const tone = cell(row, key)?.tone ?? 'default'
  if (tone === 'success') return 'bg-green-500/10 text-green-700'
  if (tone === 'warning') return 'bg-amber-500/10 text-amber-700'
  if (tone === 'danger') return 'bg-red-600/10 text-[var(--tx-danger)]'
  if (tone === 'muted') return 'bg-[var(--tx-bg-muted)] text-[var(--tx-text-muted)]'
  return 'bg-[var(--tx-bg-muted)] text-[var(--tx-text-secondary)]'
}
</script>
