<template>
  <div class="log-line" :class="[lineLevel, { compact, wrap }]">
    <span v-if="number != null" class="line-number">{{ number }}</span>
    <span v-if="source" class="source">{{ source }}</span>
    <span class="message"><code class="hljs language-tunnel-log" v-html="highlighted"></code></span>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import hljs from 'highlight.js/lib/core'
import { detectLogLevel, type LogLevel } from '@/utils/logs'
import { tunnelLogLanguage } from '@/utils/tunnelLogLanguage'

if (!hljs.getLanguage('tunnel-log')) hljs.registerLanguage('tunnel-log', tunnelLogLanguage)

const HIGHLIGHT_CACHE_LIMIT = 2000
const highlightCache = new Map<string, string>()

const props = withDefaults(defineProps<{
  text: string
  level?: LogLevel
  number?: number
  source?: string
  compact?: boolean
  wrap?: boolean
}>(), {
  source: '',
  compact: false,
  wrap: false,
})

const lineLevel = computed(() => props.level ?? detectLogLevel(props.text))
const highlighted = computed(() => cachedHighlight(props.text ?? ''))

function cachedHighlight(text: string): string {
  const cached = highlightCache.get(text)
  if (cached != null) return cached
  const html = hljs.highlight(text, { language: 'tunnel-log', ignoreIllegals: true }).value
  if (highlightCache.size >= HIGHLIGHT_CACHE_LIMIT) highlightCache.clear()
  highlightCache.set(text, html)
  return html
}
</script>

<style scoped>
.log-line {
  display: flex;
  min-width: max-content;
  padding: 1px 12px 1px 0;
  color: #b8c0cc;
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace;
  font-size: 12px;
  line-height: 1.55;
  font-variant-numeric: tabular-nums;
}
.log-line:hover { background: rgba(255, 255, 255, 0.035); }
.log-line.error { color: #efb0b0; }
.log-line.warn { color: #dfc27d; }
.log-line.debug { color: #7d8999; }
.log-line.compact {
  min-width: 0;
  padding: 1px 0;
  line-height: 1.55;
}
.line-number {
  position: sticky;
  left: 0;
  z-index: 1;
  width: 52px;
  flex-shrink: 0;
  border-right: 1px solid #252b35;
  background: #151a22;
  color: #596577;
  text-align: right;
  padding-right: 10px;
  margin-right: 12px;
  -webkit-user-select: none;
  user-select: none;
}
.log-line:hover .line-number { background: #1a2029; }
.source {
  flex-shrink: 0;
  width: 8ch;
  max-width: 8ch;
  color: #718096;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-right: 12px;
  -webkit-user-select: none;
  user-select: none;
}
.message {
  white-space: pre;
  -webkit-user-select: text;
  user-select: text;
}
.message * {
  -webkit-user-select: text;
  user-select: text;
}
.message :deep(.hljs) {
  display: inline;
  padding: 0;
  overflow: visible;
  background: transparent;
  color: inherit;
}
.wrap {
  min-width: 0;
  width: 100%;
}
.wrap .message {
  min-width: 0;
  white-space: pre-wrap;
  word-break: break-all;
}
.compact .message {
  min-width: 0;
}
</style>
