<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { fetchThumbUrl } from '../api'
import type { FileRecord } from '../types'

const props = defineProps<{
  file: FileRecord
  selected: boolean
  dupCount: number
  grayThreshold: number
}>()
const emit = defineEmits<{ (e: 'toggle', id: number): void }>()

const url = ref('')
const failed = ref(false)

watch(
  () => props.file.id,
  async () => {
    if (url.value) {
      URL.revokeObjectURL(url.value)
      url.value = ''
    }
    failed.value = false
    try {
      url.value = await fetchThumbUrl(props.file.id)
    } catch {
      failed.value = true
    }
  },
  { immediate: true },
)

onUnmounted(() => {
  if (url.value) URL.revokeObjectURL(url.value)
})
</script>

<template>
  <div class="cell" :class="{ on: props.selected }" @click="emit('toggle', props.file.id)">
    <img v-if="url" :src="url" loading="lazy" alt="" />
    <div v-else class="ph">{{ failed ? '读不出' : '…' }}</div>
    <span v-if="props.dupCount > 1" class="badge left">×{{ props.dupCount }}</span>
    <span v-if="(props.file.grayScore ?? 999) <= props.grayThreshold" class="badge left low">灰</span>
    <span v-if="props.selected" class="badge right">✓</span>
  </div>
</template>

<style scoped>
.cell {
  position: relative;
  aspect-ratio: 1 / 1.32;
  background: rgba(128, 128, 128, 0.15);
  border-radius: 4px;
  overflow: hidden;
  cursor: pointer;
}
.cell.on {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}
.ph {
  display: grid;
  place-items: center;
  height: 100%;
  font-size: 11px;
  opacity: 0.5;
}
.badge {
  position: absolute;
  top: 3px;
  font-size: 10px;
  padding: 0 4px;
  border-radius: 3px;
  background: rgba(0, 0, 0, 0.6);
  color: #fff;
}
.badge.left {
  left: 3px;
}
.badge.left.low {
  top: 20px;
}
.badge.right {
  right: 3px;
  background: var(--accent);
}
</style>
