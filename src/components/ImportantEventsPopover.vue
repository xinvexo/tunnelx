<template>
  <div ref="root" class="events" data-testid="important-events">
    <button
      class="event-trigger"
      :class="{ active: open, attention: unreadCount > 0 }"
      type="button"
      :aria-expanded="open"
      :aria-label="t('events.open')"
      @click="toggle"
    >
      <span class="trigger-dot" :class="latestTone" />
      <span>{{ triggerText }}</span>
      <span v-if="unreadCount > 0" class="trigger-count">{{ unreadCount }}</span>
    </button>

    <Transition name="events-pop">
      <section v-if="open" class="event-popover" role="dialog" :aria-label="t('events.title')">
        <header class="popover-head">
          <div>
            <h2>{{ t('events.title') }}</h2>
          </div>
          <button
            v-if="ui.importantEvents.length"
            class="clear-btn"
            type="button"
            @click="ui.clearImportantEvents()"
          >
            {{ t('events.clear') }}
          </button>
        </header>

        <div v-if="ui.importantEvents.length" class="event-list">
          <article
            v-for="event in ui.importantEvents"
            :key="event.id"
            class="event-item"
            :class="[event.tone, { clickable: eventRoute(event) }]"
            :role="eventRoute(event) ? 'link' : undefined"
            :tabindex="eventRoute(event) ? 0 : undefined"
            :aria-label="eventRoute(event) ? t('events.openEvent') : undefined"
            @click="openEvent(event)"
            @keydown.enter.prevent="openEvent(event)"
            @keydown.space.prevent="openEvent(event)"
          >
            <div class="event-meta">
              <span class="event-dot" />
              <span>{{ toneLabel(event.tone) }}</span>
              <span class="separator">·</span>
              <time :datetime="new Date(event.createdAt).toISOString()">{{ formatEventTime(event.createdAt) }}</time>
            </div>
            <h3>{{ eventTitle(event) }}</h3>
            <p v-if="eventMessage(event)">{{ eventMessage(event) }}</p>
          </article>
        </div>

        <div v-else class="event-empty">
          <span class="empty-dot" />
          <strong>{{ t('events.emptyTitle') }}</strong>
        </div>
      </section>
    </Transition>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { connectionPath } from '@/providers/routes'
import { useUiStore, type ImportantEventItem, type ImportantEventTone } from '@/stores/ui'

const ui = useUiStore()
const { locale, t, te } = useI18n()
const router = useRouter()
const root = ref<HTMLElement | null>(null)
const open = ref(false)

const unreadCount = computed(() => ui.unreadImportantEventCount)
const latestTone = computed(() => ui.importantEvents[0]?.tone ?? 'none')
const triggerText = computed(() => {
  if (!ui.importantEvents.length) return t('events.none')
  if (unreadCount.value > 0) return t('events.unread', { count: unreadCount.value })
  return t('events.hasEvents')
})

function toggle() {
  open.value = !open.value
  if (open.value) ui.markImportantEventsRead()
}

function close() {
  open.value = false
}

function onPointerDown(event: PointerEvent) {
  if (!open.value) return
  if (root.value?.contains(event.target as Node)) return
  close()
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !open.value) return
  event.preventDefault()
  close()
}

function toneLabel(tone: ImportantEventTone) {
  return t(`events.tone.${tone}`)
}

function eventTitle(event: ImportantEventItem) {
  if (event.titleKey && te(event.titleKey)) {
    return t(event.titleKey, event.titleParams ?? {})
  }
  return event.title
}

function eventMessage(event: ImportantEventItem) {
  if (event.messageKey && te(event.messageKey)) {
    return t(event.messageKey, event.messageParams ?? {})
  }
  return event.message ?? ''
}

function eventRoute(event: ImportantEventItem) {
  const source = event.source ?? ''
  if (source === 'update:app') return '/settings/general'
  if (source === 'watchdog') return '/activity'
  if (source.startsWith('update:')) {
    const providerId = source.slice('update:'.length).trim()
    return providerId ? `/settings/versions?provider=${encodeURIComponent(providerId)}` : '/settings/versions'
  }
  if (source.startsWith('start:') || source.startsWith('runtime:')) {
    const parts = source.split(':')
    const providerId = parts[1]?.trim()
    const tunnelId = parts[2]?.trim()
    if (providerId && tunnelId) return connectionPath(providerId, tunnelId, 'logs')
  }
  return ''
}

function openEvent(event: ImportantEventItem) {
  const target = eventRoute(event)
  if (!target) return
  close()
  void router.push(target)
}

function formatEventTime(value: number) {
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ''
  return new Intl.DateTimeFormat(locale.value, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).format(date)
}

onMounted(() => {
  document.addEventListener('pointerdown', onPointerDown, true)
  window.addEventListener('keydown', onKeydown, true)
})

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onPointerDown, true)
  window.removeEventListener('keydown', onKeydown, true)
})
</script>

<style scoped>
.events {
  position: relative;
  justify-self: start;
  margin-left: 6px;
  min-width: 0;
}
.event-trigger {
  height: 26px;
  min-width: 82px;
  border: 1px solid transparent;
  border-radius: 999px;
  background: rgba(15, 23, 42, 0.08);
  color: var(--tx-text-secondary);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 0 12px;
  font-size: 12px;
  font-weight: 650;
  white-space: nowrap;
  cursor: pointer;
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease, box-shadow 120ms ease;
}
.event-trigger:hover,
.event-trigger:focus-visible,
.event-trigger.active {
  background: rgba(255, 255, 255, 0.86);
  border-color: var(--tx-border-subtle);
  color: var(--tx-text-primary);
}
.event-trigger:focus-visible {
  outline: none;
  box-shadow: 0 0 0 3px var(--tx-focus-ring);
}
.event-trigger.attention {
  background: rgba(217, 119, 6, 0.12);
  color: #8a4b04;
}
.trigger-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: var(--tx-text-muted);
  flex: 0 0 auto;
}
.trigger-dot.warning { background: var(--tx-warning); }
.trigger-dot.danger { background: var(--tx-danger); }
.trigger-dot.info { background: var(--tx-accent); }
.trigger-count {
  min-width: 16px;
  height: 16px;
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.72);
  color: currentColor;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 0 5px;
  font-size: 10px;
}
.event-popover {
  position: absolute;
  z-index: 180;
  top: 34px;
  left: 0;
  width: min(560px, calc(100vw - 36px));
  max-height: min(520px, calc(100vh - 70px));
  border: 1px solid rgba(15, 23, 42, 0.08);
  border-radius: 18px;
  background: rgba(255, 255, 255, 0.92);
  box-shadow:
    0 22px 60px rgba(15, 23, 42, 0.16),
    0 2px 8px rgba(15, 23, 42, 0.06);
  backdrop-filter: blur(18px);
  overflow: hidden;
}
.popover-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  padding: 20px 22px 14px;
}
.popover-head h2 {
  margin: 0;
  font-size: 14px;
  font-weight: 680;
  line-height: 1.3;
}
.clear-btn {
  height: 26px;
  border: 0;
  border-radius: var(--tx-radius-sm);
  background: transparent;
  color: var(--tx-text-muted);
  padding: 0 8px;
  cursor: pointer;
}
.clear-btn:hover,
.clear-btn:focus-visible {
  background: var(--tx-bg-hover);
  color: var(--tx-text-primary);
}
.clear-btn:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.event-list {
  max-height: 404px;
  overflow: auto;
  padding: 0 14px 14px;
}
.event-item {
  border-radius: 12px;
  padding: 12px 8px 13px 14px;
}
.event-item.clickable {
  cursor: pointer;
  transition: background 120ms ease, box-shadow 120ms ease;
}
.event-item.clickable:hover,
.event-item.clickable:focus-visible {
  background: var(--tx-bg-hover);
}
.event-item.clickable:focus-visible {
  outline: none;
  box-shadow: inset 0 0 0 1px var(--tx-border-hover);
}
.event-item + .event-item {
  border-top: 1px solid var(--tx-border-subtle);
}
.event-item h3 {
  margin: 6px 0 0;
  color: var(--tx-text-primary);
  font-size: 13px;
  font-weight: 650;
  line-height: 1.45;
}
.event-item p {
  margin: 5px 0 0;
  color: var(--tx-text-secondary);
  line-height: 1.55;
  overflow-wrap: anywhere;
  user-select: text;
}
.event-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--tx-text-muted);
  font-size: 12px;
}
.event-dot {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: var(--tx-accent);
}
.event-item.warning .event-dot { background: var(--tx-warning); }
.event-item.danger .event-dot { background: var(--tx-danger); }
.separator {
  color: rgba(15, 23, 42, 0.22);
}
.event-empty {
  min-height: 132px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 22px;
  color: var(--tx-text-muted);
}
.event-empty strong {
  color: var(--tx-text-secondary);
}
.empty-dot {
  width: 10px;
  height: 10px;
  border-radius: 999px;
  background: var(--tx-text-muted);
  opacity: 0.75;
}
.events-pop-enter-active,
.events-pop-leave-active {
  transition: opacity 140ms ease, transform 140ms ease, filter 140ms ease;
}
.events-pop-enter-from,
.events-pop-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.985);
  filter: blur(2px);
}
</style>
