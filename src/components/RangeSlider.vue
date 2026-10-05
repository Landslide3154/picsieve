<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'

/**
 * 双向范围滑块 + 分布直方图。
 *
 * - 线性刻度（分辨率）与对数刻度（体积）都支持：对数刻度下每一档宽度相等，
 *   否则大部分图都会挤在滑块最左边，根本没法用。
 * - 拖动时只发 update:modelValue（父组件据此实时更新命中数），
 *   松手或键盘操作才发 change（父组件据此刷新图片墙）。
 */
const props = withDefaults(
  defineProps<{
    modelValue: [number, number]
    min: number
    max: number
    step?: number
    scale?: 'linear' | 'log'
    buckets?: number[]
    edges?: number[]
    label: string
    format?: (v: number) => string
  }>(),
  { step: 1, scale: 'linear', buckets: () => [], edges: () => [] },
)

const emit = defineEmits<{
  (e: 'update:modelValue', v: [number, number]): void
  (e: 'change', v: [number, number]): void
}>()

const track = ref<HTMLElement | null>(null)
const dragging = ref<null | 'lo' | 'hi'>(null)

const span = computed(() => props.max - props.min)
const logSpan = computed(() => Math.log(props.max / props.min))

function toPos(v: number): number {
  const clamped = Math.min(props.max, Math.max(props.min, v))
  if (props.scale === 'log') return Math.log(clamped / props.min) / logSpan.value
  return (clamped - props.min) / span.value
}

function fromPos(r: number): number {
  const clamped = Math.min(1, Math.max(0, r))
  if (props.scale === 'log') {
    const raw = props.min * Math.pow(props.max / props.min, clamped)
    // 对数刻度按对数步长吸附，避免出现 1048577 这种读数
    const quantum = Math.pow(props.max / props.min, props.step / 1000)
    return Math.round(raw * quantum) / quantum
  }
  return Math.round((props.min + clamped * span.value) / props.step) * props.step
}

const loPos = computed(() => toPos(props.modelValue[0]) * 100)
const hiPos = computed(() => toPos(props.modelValue[1]) * 100)

/** 直方图柱子：按 edges 在轨道上的实际位置摆放，线性/对数都对齐 */
const bars = computed(() => {
  const buckets = props.buckets ?? []
  const edges = props.edges ?? []
  const peak = Math.max(1, ...buckets)
  const [lo, hi] = props.modelValue
  return buckets.map((n, i) => {
    const a = edges[i] ?? props.min
    const b = edges[i + 1] ?? props.max
    const left = toPos(a) * 100
    const right = toPos(b) * 100
    const inside = b > lo && a < hi
    return { left, width: Math.max(0.8, right - left - 0.35), height: (n / peak) * 100, inside }
  })
})

function valueAt(clientX: number): number {
  const el = track.value
  if (!el) return props.min
  const rect = el.getBoundingClientRect()
  return fromPos((clientX - rect.left) / Math.max(1, rect.width))
}

function push(next: [number, number], commit: boolean) {
  emit('update:modelValue', next)
  if (commit) emit('change', next)
}

function moveTo(clientX: number, commit: boolean) {
  const v = valueAt(clientX)
  const [lo, hi] = props.modelValue
  if (dragging.value === 'lo') push([Math.min(v, hi), hi], commit)
  else if (dragging.value === 'hi') push([lo, Math.max(v, lo)], commit)
}

function startDrag(which: 'lo' | 'hi', e: PointerEvent) {
  e.preventDefault()
  dragging.value = which
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', onUp)
}

function onMove(e: PointerEvent) {
  if (!dragging.value) return
  moveTo(e.clientX, false)
}

function onUp() {
  if (!dragging.value) return
  dragging.value = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  window.removeEventListener('pointercancel', onUp)
  emit('change', props.modelValue)
}

/** 点轨道空白处：把最近的那个把手挪过去 */
function onTrackDown(e: PointerEvent) {
  const v = valueAt(e.clientX)
  const [lo, hi] = props.modelValue
  dragging.value = Math.abs(v - lo) <= Math.abs(v - hi) ? 'lo' : 'hi'
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  window.addEventListener('pointercancel', onUp)
  moveTo(e.clientX, false)
}

function onKey(which: 'lo' | 'hi', e: KeyboardEvent) {
  const base = props.scale === 'log' ? span.value / 200 : props.step
  const mult = e.shiftKey ? 10 : 1
  const delta = base * mult
  const [lo, hi] = props.modelValue
  let next: [number, number] | null = null
  if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') {
    next = which === 'lo' ? [Math.max(props.min, lo - delta), hi] : [lo, Math.max(lo, hi - delta)]
  } else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') {
    next = which === 'lo' ? [Math.min(hi, lo + delta), hi] : [lo, Math.min(props.max, hi + delta)]
  } else if (e.key === 'Home') {
    next = which === 'lo' ? [props.min, hi] : [lo, props.min]
  } else if (e.key === 'End') {
    next = which === 'lo' ? [props.max, hi] : [lo, props.max]
  }
  if (next) {
    e.preventDefault()
    const fixed: [number, number] = [
      props.scale === 'log' ? next[0] : Math.round(next[0]),
      props.scale === 'log' ? next[1] : Math.round(next[1]),
    ]
    push(fixed, true)
  }
}

onUnmounted(() => {
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  window.removeEventListener('pointercancel', onUp)
})

const text = (v: number) => (props.format ? props.format(v) : String(v))
</script>

<template>
  <div class="range">
    <div v-if="bars.length" class="hist" aria-hidden="true">
      <i
        v-for="(b, i) in bars"
        :key="i"
        :class="{ in: b.inside }"
        :style="{ left: b.left + '%', width: b.width + '%', height: Math.max(2, b.height) + '%' }"
      />
    </div>
    <div ref="track" class="track" @pointerdown.self="onTrackDown">
      <div class="rail" />
      <div class="fill" :style="{ left: loPos + '%', width: Math.max(0, hiPos - loPos) + '%' }" />
      <div
        class="thumb"
        :class="{ active: dragging === 'lo' }"
        role="slider"
        tabindex="0"
        :aria-label="label + ' 下限'"
        :aria-valuemin="min"
        :aria-valuemax="max"
        :aria-valuenow="modelValue[0]"
        :aria-valuetext="text(modelValue[0])"
        :style="{ left: loPos + '%' }"
        @pointerdown="startDrag('lo', $event)"
        @keydown="onKey('lo', $event)"
      />
      <div
        class="thumb"
        :class="{ active: dragging === 'hi' }"
        role="slider"
        tabindex="0"
        :aria-label="label + ' 上限'"
        :aria-valuemin="min"
        :aria-valuemax="max"
        :aria-valuenow="modelValue[1]"
        :aria-valuetext="text(modelValue[1])"
        :style="{ left: hiPos + '%' }"
        @pointerdown="startDrag('hi', $event)"
        @keydown="onKey('hi', $event)"
      />
    </div>
  </div>
</template>

<style scoped>
.range {
  position: relative;
  padding-top: 4px;
}
.hist {
  position: relative;
  height: 36px;
}
.hist i {
  position: absolute;
  bottom: 0;
  background: rgba(150, 160, 175, 0.22);
  border-radius: 1px 1px 0 0;
}
.hist i.in {
  background: rgba(108, 176, 255, 0.55);
}
.track {
  position: relative;
  height: 22px;
  cursor: pointer;
  touch-action: none;
}
.rail,
.fill {
  position: absolute;
  top: 9px;
  height: 4px;
  border-radius: 2px;
}
.rail {
  left: 0;
  right: 0;
  background: rgba(150, 160, 175, 0.25);
}
.fill {
  background: var(--accent);
}
.thumb {
  position: absolute;
  top: 2px;
  width: 14px;
  height: 18px;
  margin-left: -7px;
  border-radius: 5px;
  background: #eef2f8;
  border: 1px solid #7f8a9b;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.5);
  cursor: grab;
  transition: box-shadow var(--speed) ease;
}
.thumb:hover,
.thumb.active {
  box-shadow:
    0 0 0 2px rgba(108, 176, 255, 0.5),
    0 1px 3px rgba(0, 0, 0, 0.6);
}
.thumb:focus-visible {
  outline: 2px solid var(--accent-bright);
  outline-offset: 1px;
}
</style>
