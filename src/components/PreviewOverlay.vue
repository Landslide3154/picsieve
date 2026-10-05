<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { fetchPreviewUrl } from '../api'
import { baseName, fmtBytes } from '../format'
import type { FileRecord } from '../types'

const props = defineProps<{ file: FileRecord }>()
const emit = defineEmits<{ (e: 'close'): void; (e: 'navigate', delta: number): void }>()

const url = ref('')
const loading = ref(true)
const failed = ref(false)

function release() {
  if (url.value) {
    URL.revokeObjectURL(url.value)
    url.value = ''
  }
}

watch(
  () => props.file.id,
  async () => {
    release()
    loading.value = true
    failed.value = false
    try {
      url.value = await fetchPreviewUrl(props.file.id)
    } catch {
      failed.value = true
    } finally {
      loading.value = false
    }
  },
  { immediate: true },
)

onUnmounted(release)

function fmtSize(bytes: number): string {
  return fmtBytes(bytes)
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape' || e.key === ' ') {
    e.preventDefault()
    emit('close')
  } else if (e.key === 'ArrowRight') {
    e.preventDefault()
    emit('navigate', 1)
  } else if (e.key === 'ArrowLeft') {
    e.preventDefault()
    emit('navigate', -1)
  }
}

window.addEventListener('keydown', onKey, true)
onUnmounted(() => window.removeEventListener('keydown', onKey, true))
</script>

<template>
  <div class="overlay" role="dialog" aria-modal="true" aria-label="大图预览" @click.self="emit('close')">
    <header>
      <span class="name truncate">{{ baseName(props.file.path) }}</span>
      <span class="meta num">
        {{ props.file.width }} × {{ props.file.height }} · {{ fmtSize(props.file.size) }} ·
        {{ (props.file.ext ?? '').toUpperCase() }}
      </span>
      <span class="grow" />
      <button class="btn ghost sm" @click="emit('close')">关闭（Esc）</button>
    </header>
    <div class="stage">
      <img v-if="url" :src="url" alt="" />
      <p v-else-if="loading" class="muted">正在生成预览…</p>
      <p v-else-if="failed" class="error-text">这个文件读不出来</p>
    </div>
    <footer>
      <span class="muted small">← → 看上一张 / 下一张 · 空格或 Esc 关闭 · 双击图片用系统程序打开</span>
    </footer>
  </div>
</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  z-index: 80;
  background: rgba(8, 9, 12, 0.92);
  display: flex;
  flex-direction: column;
  overscroll-behavior: contain;
}
header,
footer {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
}
header {
  border-bottom: 1px solid rgba(150, 160, 175, 0.18);
}
footer {
  border-top: 1px solid rgba(150, 160, 175, 0.18);
}
.name {
  font-weight: 600;
  max-width: 60vw;
}
.meta {
  font-size: 12px;
  color: var(--dim);
}
.grow {
  flex: 1;
}
.stage {
  flex: 1;
  min-height: 0;
  display: grid;
  place-items: center;
  padding: 16px;
}
.stage img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
  border-radius: 6px;
  box-shadow: 0 20px 50px rgba(0, 0, 0, 0.6);
}
</style>
