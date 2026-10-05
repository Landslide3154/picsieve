<script setup lang="ts">
import { computed } from 'vue'
import { baseName, fmtBytes } from '../format'
import type { FileRecord } from '../types'

const props = defineProps<{
  file: FileRecord
  dupCount: number
  grayThreshold: number
  anchor: DOMRect
}>()

const TIP_W = 300

const fileName = computed(() => baseName(props.file.path))
const dims = computed(() =>
  props.file.width && props.file.height ? `${props.file.width} × ${props.file.height}` : '尺寸未知',
)
const gray = computed(() => {
  const s = props.file.grayScore
  if (s === null) return '灰度未算'
  return s <= props.grayThreshold ? `灰阶图（分数 ${s.toFixed(0)}）` : `彩色（灰度分数 ${s.toFixed(0)}）`
})

/** 优先贴在格子下方；下方放不下就翻到上方 */
const style = computed(() => {
  const below = props.anchor.bottom + 10
  const flip = below + 150 > window.innerHeight
  const left = Math.min(
    Math.max(8, props.anchor.left + props.anchor.width / 2 - TIP_W / 2),
    window.innerWidth - TIP_W - 8,
  )
  return {
    left: left + 'px',
    top: (flip ? Math.max(8, props.anchor.top - 150) : below) + 'px',
    width: TIP_W + 'px',
  }
})
</script>

<template>
  <div class="tip" :style="style" role="tooltip">
    <div class="name truncate">{{ fileName }}</div>
    <div class="line num">{{ dims }} · {{ fmtBytes(file.size) }} · {{ (file.ext ?? '').toUpperCase() }}</div>
    <div class="line">{{ gray }}<template v-if="dupCount > 1"> · 另有 {{ dupCount - 1 }} 张一模一样</template></div>
    <div v-if="file.pid || file.artist" class="line">
      作品 ID {{ file.pid ?? '—' }} · 画师 {{ file.artist ?? '—' }}
    </div>
    <div class="line path truncate">{{ file.path }}</div>
  </div>
</template>

<style scoped>
.tip {
  position: fixed;
  z-index: 50;
  pointer-events: none;
  background: rgba(16, 18, 22, 0.96);
  border: 1px solid var(--line-strong);
  border-radius: 8px;
  padding: 8px 10px;
  box-shadow: var(--shadow-2);
  font-size: 12px;
  line-height: 1.5;
}
.name {
  font-weight: 600;
  margin-bottom: 3px;
}
.line {
  color: #b9c2d0;
}
.path {
  color: var(--dimmer);
  font-size: 11px;
}
.hint {
  margin-top: 5px;
  padding-top: 5px;
  border-top: 1px solid var(--line);
  color: var(--accent-bright);
  font-size: 11px;
}
</style>
