<template>
  <section class="border border-tx-border rounded-[var(--tx-radius-md)] bg-tx-surface p-18px">
    <div class="mb-14px flex items-start justify-between gap-16px">
      <div class="min-w-0">
        <div class="runtime-title">
          <h2 class="m-0 text-16px text-[var(--tx-text-primary)] font-650">{{ providerName }}</h2>
          <code v-if="showInstalledVersion" class="runtime-version-badge mono">{{ installedVersion }}</code>
        </div>
        <p v-if="subtitle" class="m-0 mt-5px text-12px text-[var(--tx-text-secondary)]">{{ subtitle }}</p>
      </div>
      <span
        class="rounded-full px-8px py-3px text-11px whitespace-nowrap"
        :class="available ? 'bg-green-500/10 text-green-700' : 'bg-red-600/10 text-[var(--tx-danger)]'"
      >
        {{ statusLabel }}
      </span>
    </div>

    <div
      v-if="updateAvailable"
      class="mb-14px flex min-h-34px items-center gap-8px rounded-[var(--tx-radius-md)] border border-blue-500/18 bg-blue-500/8 px-10px py-8px text-12px text-blue-800"
    >
      <Icon class="h-15px w-15px flex-none" icon="lucide:circle-arrow-up" />
      <span>{{ updateAvailableText }}</span>
    </div>

    <div v-if="installing || updating" class="runtime-progress" :class="{ indeterminate: progressPercent === null }">
      <div class="progress-meta">
        <span>{{ progressText }}</span>
      </div>
      <div class="progress-track">
        <span :style="progressStyle" />
      </div>
    </div>

    <div class="action-bar">
      <div class="action-group primary-actions">
        <TxButton
          icon="lucide:download"
          :loading="installing"
          :disabled="busy && !installing"
          @click="emit('install')"
        >
          {{ runtimeManaged ? reinstallText : installText }}
        </TxButton>
        <TxButton
          v-if="updateAvailable"
          icon="lucide:circle-arrow-up"
          tone="primary"
          :loading="updating"
          :disabled="busy && !updating"
          @click="emit('update')"
        >
          {{ updateText }}
        </TxButton>
      </div>
      <div class="action-group secondary-actions">
        <TxButton
          icon="lucide:refresh-cw"
          tone="ghost"
          class="toolbar-action"
          :loading="refreshing"
          :disabled="busy && !refreshing"
          @click="emit('refresh')"
        >
          {{ refreshText }}
        </TxButton>
        <TxButton
          icon="lucide:radar"
          tone="ghost"
          class="toolbar-action"
          :loading="checkingUpdate"
          :disabled="busy && !checkingUpdate"
          @click="emit('checkUpdate')"
        >
          {{ checkUpdateText }}
        </TxButton>
        <TxButton
          :class="['toolbar-action', 'danger-ghost', { placeholder: !runtimeManaged }]"
          icon="lucide:trash-2"
          tone="ghost"
          :loading="uninstalling"
          :disabled="!runtimeManaged || (busy && !uninstalling)"
          :aria-hidden="!runtimeManaged"
          :tabindex="runtimeManaged ? 0 : -1"
          @click="emit('uninstall')"
        >
          {{ uninstallText }}
        </TxButton>
      </div>
    </div>

    <slot />
  </section>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { Icon } from '@iconify/vue'
import TxButton from '@/components/ui/TxButton.vue'

const props = withDefaults(defineProps<{
  providerName: string
  subtitle?: string
  available?: boolean
  runtimeManaged?: boolean
  installedVersion: string
  refreshing?: boolean
  installing?: boolean
  uninstalling?: boolean
  checkingUpdate?: boolean
  updating?: boolean
  updateAvailable?: boolean
  latestVersion?: string
  installLabel?: string
  reinstallLabel?: string
  uninstallLabel?: string
  refreshLabel?: string
  checkUpdateLabel?: string
  updateLabel?: string
  versionLabel?: string
  availableLabel?: string
  invalidLabel?: string
  missingLabel?: string
  installProgressPercent?: number | null
  installProgressLabel?: string
}>(), {
  subtitle: '',
  available: false,
  runtimeManaged: false,
  refreshing: false,
  installing: false,
  uninstalling: false,
  checkingUpdate: false,
  updating: false,
  updateAvailable: false,
  latestVersion: '',
  installLabel: '',
  reinstallLabel: '',
  uninstallLabel: '',
  refreshLabel: '',
  checkUpdateLabel: '',
  updateLabel: '',
  versionLabel: '',
  availableLabel: '',
  invalidLabel: '',
  missingLabel: '',
  installProgressPercent: null,
  installProgressLabel: '',
})

const emit = defineEmits<{ install: []; uninstall: []; refresh: []; checkUpdate: []; update: [] }>()
const { t, te } = useI18n()
const busy = computed(() => props.refreshing || props.installing || props.uninstalling || props.checkingUpdate || props.updating)
function label(key: string, fallbackKey: string) {
  if (te(key)) return t(key)
  return t(fallbackKey)
}
const installText = computed(() => props.installLabel || label('runtime.install', 'version.installRuntime'))
const reinstallText = computed(() => props.reinstallLabel || label('runtime.reinstall', 'version.reinstallRuntime'))
const uninstallText = computed(() => props.uninstallLabel || label('runtime.uninstall', 'version.uninstallRuntime'))
const refreshText = computed(() => props.refreshLabel || label('runtime.refresh', 'version.refresh'))
const checkUpdateText = computed(() => props.checkUpdateLabel || t('runtime.checkUpdate'))
const updateText = computed(() => props.updateLabel || t('runtime.update'))
const updateAvailableText = computed(() => t('runtime.updateAvailable', { version: props.latestVersion || '-' }))
const showInstalledVersion = computed(() => props.available && props.installedVersion.trim() && props.installedVersion.trim() !== '-')
const progressPercent = computed(() => {
  const value = props.installProgressPercent
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.max(0, Math.min(100, value))
    : null
})
const progressText = computed(() => {
  if (props.installProgressLabel) return props.installProgressLabel
  return progressPercent.value === null
    ? t('runtime.installProgressUnknown')
    : t('runtime.installProgress', { percent: Math.floor(progressPercent.value) })
})
const progressStyle = computed(() => (
  progressPercent.value === null
    ? {}
    : { width: `${progressPercent.value}%` }
))
const statusLabel = computed(() => {
  if (props.available) return props.availableLabel || label('runtime.status.available', 'version.status.available')
  return props.runtimeManaged
    ? props.invalidLabel || label('runtime.status.invalid', 'version.status.invalid')
    : props.missingLabel || label('runtime.status.missing', 'version.status.missing')
})
</script>

<style scoped>
.action-bar {
  min-height: 32px;
  margin-top: 4px;
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
}

.runtime-title {
  min-height: 28px;
  display: flex;
  align-items: center;
  gap: 10px;
}

.runtime-version-badge {
  min-width: 0;
  max-width: 180px;
  height: 24px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--tx-border);
  border-radius: 999px;
  background: var(--tx-bg-muted);
  color: var(--tx-text-secondary);
  padding: 0 9px;
  font-size: 12px;
  line-height: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.action-group {
  min-width: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

.secondary-actions {
  justify-content: flex-end;
}

.placeholder {
  visibility: hidden;
  pointer-events: none;
}

.toolbar-action {
  border-color: transparent;
  box-shadow: none;
}

.toolbar-action:hover:not(:disabled),
.toolbar-action:focus-visible {
  border-color: transparent;
}

:deep(.danger-ghost.tx-button.ghost) {
  color: var(--tx-danger);
}

:deep(.danger-ghost.tx-button.ghost:hover:not(:disabled)),
:deep(.danger-ghost.tx-button.ghost:focus-visible) {
  color: var(--tx-danger);
  background: rgba(220, 38, 38, 0.1);
}

@media (max-width: 720px) {
  .action-bar {
    flex-direction: column;
  }

  .secondary-actions {
    justify-content: flex-start;
  }
}

.runtime-progress {
  margin: -4px 0 16px;
}

.progress-meta {
  height: 18px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--tx-text-secondary);
  font-size: 12px;
}

.progress-track {
  position: relative;
  height: 6px;
  overflow: hidden;
  border-radius: 999px;
  background: var(--tx-bg-muted);
}

.progress-track span {
  position: absolute;
  inset: 0 auto 0 0;
  width: 0;
  border-radius: inherit;
  background: var(--tx-accent);
  transition: width 120ms ease;
}

.runtime-progress.indeterminate .progress-track span {
  width: 36%;
  animation: runtime-progress-slide 1.1s ease-in-out infinite;
}

@keyframes runtime-progress-slide {
  0% { transform: translateX(-110%); }
  100% { transform: translateX(280%); }
}
</style>
