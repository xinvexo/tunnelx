<template>
  <WorkspacePage wide>
    <div class="page-toolbar">
      <span>{{ t('proxyList.count', { count: filtered.length }) }}</span>
      <div class="toolbar-actions">
        <div class="search">
          <Icon icon="lucide:search" />
          <input v-model="query" :placeholder="t('proxyList.searchPlaceholder')" />
        </div>
        <TxButton tone="primary" icon="lucide:plus" @click="openNew">{{ t('proxyList.create') }}</TxButton>
      </div>
    </div>

    <div v-if="filtered.length" class="proxy-table-wrap">
      <table class="proxy-table">
        <colgroup>
          <col class="col-main" />
          <col class="col-local" />
          <col class="col-remote" />
          <col class="col-actions" />
        </colgroup>
        <thead>
          <tr>
            <th>{{ t('proxyList.head.name') }}</th>
            <th>{{ t('proxyList.head.local') }}</th>
            <th>{{ t('proxyList.head.remote') }}</th>
            <th class="actions-head" />
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="proxy in filtered"
            :key="proxy.id"
            class="proxy-row"
            @dblclick="openEdit(proxy)"
          >
            <td>
              <div class="main-cell">
                <span
                  class="drag-button"
                  role="button"
                  :aria-label="t('proxyList.reorder')"
                  @pointerdown="onDragPointerDown(proxy.id, $event)"
                  @click.stop
                  @dblclick.stop
                >
                  <Icon class="drag-handle" icon="lucide:grip-vertical" />
                </span>
                <span class="run-dot" :class="proxyRunClass(proxy)" />
                <button class="name" type="button" @click="openEdit(proxy)">
                  {{ proxy.config.name || t('proxyList.unnamed') }}
                </button>
                <span class="type">{{ proxy.config.type.toUpperCase() }}</span>
              </div>
            </td>
            <td><span class="cell-text mono">{{ localSummary(proxy.config) }}</span></td>
            <td><span class="cell-text mono">{{ remoteSummary(proxy.config) }}</span></td>
            <td>
              <div class="row-actions" @click.stop @dblclick.stop>
                <div class="switch-slot">
                  <Icon v-if="savingProxyIds.has(proxy.id)" class="switch-loading" icon="lucide:loader-circle" />
                  <TxSwitch
                    v-else
                    :model-value="proxy.enabled"
                    @update:model-value="toggle(proxy, $event)"
                  />
                </div>
                <TxIconButton icon="lucide:pencil" :label="t('proxyList.edit')" @click="openEdit(proxy)" />
                <TxIconButton icon="lucide:trash-2" :label="t('proxyList.remove')" danger @click="remove(proxy)" />
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-else class="empty">
      <Icon icon="lucide:waypoints" />
      <h3>{{ proxies.length ? t('proxyList.emptyNoMatch') : t('proxyList.emptyNone') }}</h3>
      <p>{{ proxies.length ? t('proxyList.emptyNoMatchDesc') : t('proxyList.emptyNoneDesc') }}</p>
    </div>

    <ProxyEditDrawer v-model="editorOpen" :source="editing" @save="save" />
  </WorkspacePage>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import ProxyEditDrawer from '@/providers/frp/components/ProxyEditDrawer.vue'
import TxButton from '@/components/ui/TxButton.vue'
import TxIconButton from '@/components/ui/TxIconButton.vue'
import TxSwitch from '@/components/ui/TxSwitch.vue'
import WorkspacePage from '@/components/ui/WorkspacePage.vue'
import { useSortable } from '@/composables/useSortable'
import { localSummary, remoteSummary, type ProfileProxy } from '@/providers/frp/domain/proxy'
import { useProfileStore } from '@/providers/frp/stores/profile'
import { useRuntimeStore } from '@/providers/frp/stores/runtime'
import { useUiStore } from '@/stores/ui'

const profiles = useProfileStore()
const runtime = useRuntimeStore()
const ui = useUiStore()
const { t } = useI18n()
const editorOpen = ref(false)
const editing = ref<ProfileProxy | null>(null)
const savingProxyIds = ref(new Set<string>())
const query = ref('')
const proxies = computed(() => profiles.current?.proxies ?? [])
const filtered = computed(() => {
  const needle = query.value.trim().toLowerCase()
  if (!needle) return proxies.value
  return proxies.value.filter((proxy) => {
    const text = [
      proxy.config.name,
      proxy.config.type,
      localSummary(proxy.config),
      remoteSummary(proxy.config),
    ].join(' ').toLowerCase()
    return text.includes(needle)
  })
})
const sortable = useSortable({
  ids: () => filtered.value.map((proxy) => proxy.id),
  itemSelector: '.proxy-row',
  handleItemSelector: '.proxy-row',
  onReorder: reorderProxies,
})

function onDragPointerDown(id: string, event: PointerEvent) {
  if (query.value.trim()) return // 搜索过滤时禁用拖拽，避免与完整列表顺序错位
  sortable.start(id, event)
}
function reorderProxies(ids: string[]) {
  const byId = new Map(proxies.value.map((proxy) => [proxy.id, proxy]))
  const next = ids.map((id) => byId.get(id)).filter((proxy): proxy is ProfileProxy => !!proxy)
  void persist(next, true)
}
function openNew() { editing.value = null; editorOpen.value = true }
function openEdit(proxy: ProfileProxy) { editing.value = proxy; editorOpen.value = true }
async function persist(next: ProfileProxy[], silent = false) {
  try {
    await profiles.saveProxies(next)
    if (!silent) ui.notify(t('proxyList.saved'), 'success')
  } catch (error) {
    ui.notify(String(error), 'danger')
  }
}
function save(proxy: ProfileProxy) {
  const next = proxies.value.slice()
  const index = next.findIndex((item) => item.id === proxy.id)
  if (index >= 0) next[index] = proxy
  else next.push(proxy)
  void persist(next)
}
async function toggle(proxy: ProfileProxy, enabled: boolean) {
  const nextSaving = new Set(savingProxyIds.value)
  nextSaving.add(proxy.id)
  savingProxyIds.value = nextSaving
  try {
    await persist(proxies.value.map((item) => item.id === proxy.id ? { ...item, enabled } : item))
  } finally {
    const done = new Set(savingProxyIds.value)
    done.delete(proxy.id)
    savingProxyIds.value = done
  }
}
async function remove(proxy: ProfileProxy) {
  const name = proxy.config.name || t('proxyList.unnamed')
  const ok = await ui.confirm({
    title: t('common.delete'),
    message: t('proxyList.confirmRemove', { name }),
    confirmLabel: t('common.delete'),
    tone: 'danger',
  })
  if (!ok) return
  void persist(proxies.value.filter((item) => item.id !== proxy.id))
}
function proxyRunClass(proxy: ProfileProxy) {
  if (!profiles.current || !proxy.enabled) return 'off'
  const status = runtime.proxyStatusOf(profiles.current.id, proxy.config.name)
  if (status === 'running') return 'running'
  if (status === 'warning') return 'warning'
  return 'pending'
}
</script>

<style scoped>
.page-toolbar {
  min-height: 32px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  color: var(--tx-text-muted);
  font-size: 12px;
  margin-bottom: 16px;
}
.toolbar-actions {
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
.search {
  width: 190px;
  height: 30px;
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-sm);
  background: var(--tx-bg-surface);
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
}
.search svg { width: 14px; height: 14px; color: var(--tx-text-muted); }
.search input { flex: 1; min-width: 0; border: 0; outline: 0; background: transparent; color: var(--tx-text-primary); }
.search input::placeholder { color: var(--tx-text-muted); }
.proxy-table-wrap {
  width: 100%;
  min-width: 0;
  overflow: visible;
  border: 1px solid var(--tx-border-subtle);
  border-radius: var(--tx-radius-md);
  background: var(--tx-bg-surface);
}
.proxy-table {
  width: 100%;
  min-width: 0;
  table-layout: fixed;
  border-collapse: collapse;
}
.col-main { width: 24%; }
.col-local { width: 32%; }
.col-remote { width: 34%; }
.col-actions { width: 116px; }
.proxy-table th {
  height: 31px;
  border-bottom: 1px solid var(--tx-border-subtle);
  padding: 0 8px;
  color: var(--tx-text-muted);
  font-size: 10px;
  font-weight: 650;
  text-align: left;
  white-space: nowrap;
}
.proxy-table th:first-child { padding-left: 38px; }
.proxy-table th.actions-head { text-align: left; }
.proxy-table td {
  height: 42px;
  border-bottom: 1px solid var(--tx-border-subtle);
  padding: 0 8px;
  vertical-align: middle;
}
.proxy-table tbody tr:last-child td { border-bottom: 0; }
.proxy-row { transition: background 120ms ease; }
.proxy-row:hover { background: var(--tx-bg-hover); }
.main-cell {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
}
.drag-button {
  width: 16px;
  height: 22px;
  flex-shrink: 0;
  border: 0;
  border-radius: var(--tx-radius-sm);
  background: transparent;
  padding: 0;
  display: grid;
  place-items: center;
  color: var(--tx-text-muted);
  cursor: grab;
  touch-action: none;
}
.drag-button:active { cursor: grabbing; }
.drag-button:hover {
  background: var(--tx-bg-hover);
  color: var(--tx-text-secondary);
}
.drag-button:focus-visible {
  outline: none;
  background: var(--tx-bg-hover);
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.drag-handle {
  width: 12px;
  height: 12px;
  flex-shrink: 0;
  opacity: 0.75;
}
.run-dot {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: var(--tx-text-muted);
  flex-shrink: 0;
}
.run-dot.running { background: var(--tx-success); }
.run-dot.warning { background: var(--tx-warning); }
.run-dot.pending,
.run-dot.off { background: var(--tx-text-muted); }
.name {
  flex: 1 1 auto;
  min-width: 0;
  color: var(--tx-text-primary);
  border: 0;
  background: transparent;
  padding: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: left;
  font-weight: 550;
  cursor: pointer;
  transition: color 120ms ease, box-shadow 120ms ease;
}
.name:hover,
.name:focus-visible {
  color: var(--tx-accent);
}
.name:focus-visible {
  outline: none;
  border-radius: var(--tx-radius-sm);
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.type { color: var(--tx-accent); flex-shrink: 0; font-size: 10px; font-weight: 700; line-height: 1; }
.cell-text {
  display: block;
  min-width: 0;
  color: var(--tx-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 5px;
}
.switch-slot {
  width: 30px;
  height: 28px;
  flex-shrink: 0;
  display: grid;
  place-items: center;
}
.switch-loading {
  width: 15px;
  height: 15px;
  color: var(--tx-text-muted);
  animation: spin 0.8s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }
.empty {
  min-height: 300px;
  border: 1px dashed var(--tx-border-strong);
  border-radius: var(--tx-radius-md);
  color: var(--tx-text-secondary);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
}
.empty svg { width: 30px; height: 30px; color: var(--tx-text-muted); }
.empty h3 { margin: 12px 0 0; color: var(--tx-text-primary); font-size: 15px; }
.empty p { margin: 6px 0 0; }

@media (max-width: 780px) {
  .page-toolbar { align-items: stretch; flex-direction: column; }
  .toolbar-actions { justify-content: flex-start; flex-wrap: wrap; }
  .search { flex: 1 1 180px; width: auto; }
  .proxy-table th,
  .proxy-table td { padding-left: 6px; padding-right: 6px; }
  .col-main { width: 24%; }
  .col-local { width: 30%; }
  .col-remote { width: 28%; }
  .col-actions { width: 112px; }
}

@media (max-width: 480px) {
  .toolbar-actions { display: grid; grid-template-columns: 1fr; }
  .search { width: 100%; }
}
</style>
