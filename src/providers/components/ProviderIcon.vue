<template>
  <img v-if="asset" class="provider-icon provider-icon-image" :src="asset" alt="" aria-hidden="true" />
  <Icon v-else class="provider-icon" :icon="icon" aria-hidden="true" />
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { Icon } from '@iconify/vue'
import { providerIcon, providerIconAsset } from '@/providers/registry'

const props = defineProps<{ providerId: string }>()

const asset = computed(() => providerIconAsset(props.providerId))
const icon = computed(() => providerIcon(props.providerId))
</script>

<style scoped>
.provider-icon {
  display: block;
  width: 1em;
  height: 1em;
  flex: 0 0 auto;
}

.provider-icon-image {
  object-fit: contain;
  border-radius: 3px;
}
</style>
