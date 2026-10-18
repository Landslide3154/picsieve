<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { openExternal, revealInExplorer } from '../api'
import ContextMenu, { type MenuItem } from './ContextMenu.vue'
import PreviewOverlay from './PreviewOverlay.vue'
import ThumbCell from './ThumbCell.vue'
import ThumbTip from './ThumbTip.vue'
import { useLibrary } from '../stores/library'
import { loadPref, savePref } from '../prefs'
import type { FileRecord } from '../types'

function clampSize(n: unknown): number {
  const v = typeof n === 'number' && Number.isFinite(n) ? Math.round(n) : 1
  return Math.max(0, Math.min(2, v))
}

const props = withDefaults(defineProps<{ grayThreshold?: number }>(), { grayThreshold: 8 })
const emit = defineEmits<{
  (e: 'notice', text: string): void
  (e: 'quarantine', ids: number[]): void
}>()

const store = useLibrary()
const scroller = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const viewportH = ref(800)

// 缩略图三档：只定「目标格子宽度」，实际宽度按列数摊平后回算。
// 格子是 1:1.32 的固定比例，所以行高必须跟着实际宽度走；
// 写死行高的话三档下会累积误差，滚动条位置和 End 的落点都会偏。
const SIZES = [124, 152, 196]
const SIZE_NAMES = ['小', '中', '大']
const GAP = 12
const RATIO = 1.32
const sizeIdx = ref(clampSize(loadPref<number>('thumbSize', 1)))
const columns = ref(6)
/** 摊平后每格的实际宽度（CSS px） */
const cellW = ref(152)
const rowPitch = computed(() => cellW.value * RATIO + GAP)
const rowCount = computed(() => Math.ceil(store.files.length / columns.value))
const totalH = computed(() => Math.max(0, rowCount.value * rowPitch.value - GAP))
const startRow = computed(() => Math.max(0, Math.floor(scrollTop.value / rowPitch.value) - 2))
const endRow = computed(() =>
  Math.min(rowCount.value, Math.ceil((scrollTop.value + viewportH.value) / rowPitch.value) + 2),
)
const visible = computed(() =>
  store.files.slice(startRow.value * columns.value, endRow.value * columns.value),
)
const firstVisibleIndex = computed(() => startRow.value * columns.value)

const dupCount = ref<Map<number, number>>(new Map())
// 用 content_hash 统计每张图的同内容成员数，供 ×N 角标使用。
// 增量维护：翻到十万张时如果每来一批就整表重算，会变成 O(n²) 的卡顿源。
let hashIds = new Map<string, number[]>()
let countedUpTo = 0

function computeDupCounts() {
  const files = store.files
  if (files.length < countedUpTo) {
    // 整表被换掉（改筛选/排序）：从头再来
    hashIds = new Map()
    countedUpTo = 0
    dupCount.value = new Map()
  }
  if (files.length === countedUpTo) return
  const m = new Map(dupCount.value)
  for (let i = countedUpTo; i < files.length; i++) {
    const f = files[i]
    if (!f.contentHash) continue
    const arr = hashIds.get(f.contentHash) ?? []
    if (!arr.length) hashIds.set(f.contentHash, arr)
    arr.push(f.id)
    // 同内容的旧记录数量也要跟着涨，所以整组一起刷新
    for (const id of arr) m.set(id, arr.length)
  }
  countedUpTo = files.length
  dupCount.value = m
}

watch(() => store.files, computeDupCounts, { immediate: true })
const focusedIndex = ref(-1)
const lastClicked = ref(-1)
const tip = ref<{ file: FileRecord; anchor: DOMRect } | null>(null)
const menu = ref<{ file: FileRecord; x: number; y: number } | null>(null)
const previewIndex = ref(-1)

const previewFile = computed(() =>
  previewIndex.value >= 0 ? (store.files[previewIndex.value] ?? null) : null,
)

// 预览开着时删掉了一张：列表变短就关浮层，否则同一个位置会自动换成下一张
watch(
  () => store.files.length,
  (n) => {
    if (previewIndex.value >= n) previewIndex.value = -1
  },
)

// 只有「筛选/排序变了」才回到顶部并关掉预览；
// 搬走文件导致的列表变化不算——那时预览要原地换成下一张，不能突然关掉
watch(
  () => store.filter,
  () => {
    interruptAuto()
    tip.value = null
    focusedIndex.value = -1
    lastClicked.value = -1
    previewIndex.value = -1
    const el = scroller.value
    if (el) {
      el.scrollTop = 0
      scrollTop.value = 0
    }
  },
)

function onScroll() {
  const el = scroller.value
  if (!el) return
  scrollTop.value = el.scrollTop
  viewportH.value = el.clientHeight
  tip.value = null
  // 快到底了就提前取下一批，滚动时不会看到空白
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - rowPitch.value * 2) {
    void store.loadMore()
  }
}

function measure() {
  const el = scroller.value
  if (!el) return
  // 可用宽度 = 容器宽 - 左右各 10px 内边距；列数按「目标格子宽 + 间距」排，
  // 再用实际宽度回算行高——这样三档下滚动条都不会骗人
  const avail = Math.max(1, el.clientWidth - 20)
  const target = SIZES[sizeIdx.value]
  columns.value = Math.max(1, Math.floor((avail + GAP) / (target + GAP)))
  cellW.value = (avail - GAP * (columns.value - 1)) / columns.value
  viewportH.value = el.clientHeight
}

function onResize() {
  measure()
}

/** Ctrl+滚轮换档：必须非 passive 才能 preventDefault，否则 WebView 会顺手把整页缩放掉 */
function setSize(next: number) {
  const idx = clampSize(next)
  if (idx === sizeIdx.value) return
  const el = scroller.value
  // 把「当前第一屏的图」钉在原地，换档后不会突然迷路
  const firstIndex = el ? Math.floor(el.scrollTop / rowPitch.value) * columns.value : 0
  sizeIdx.value = idx
  savePref('thumbSize', idx)
  measure()
  if (el) {
    const row = Math.floor(firstIndex / columns.value)
    el.scrollTop = row * rowPitch.value
    scrollTop.value = el.scrollTop
  }
  emit('notice', `缩略图：${SIZE_NAMES[idx]}`)
}

function onWheel(e: WheelEvent) {
  if (!e.ctrlKey) return
  e.preventDefault()
  setSize(sizeIdx.value + (e.deltaY > 0 ? 1 : -1))
}

let wheelTarget: HTMLElement | null = null
onMounted(() => {
  measure()
  window.addEventListener('resize', onResize)
  wheelTarget = scroller.value
  wheelTarget?.addEventListener('wheel', onWheel, { passive: false })
})
onUnmounted(() => {
  window.removeEventListener('resize', onResize)
  wheelTarget?.removeEventListener('wheel', onWheel)
})

// ---------- 交互 ----------
function onSelect(id: number, mods: { shift: boolean; ctrl: boolean }) {
  const index = store.files.findIndex((f) => f.id === id)
  if (mods.shift && lastClicked.value >= 0 && index >= 0) {
    store.selectRange(lastClicked.value, index)
  } else {
    store.toggle(id)
  }
  if (index >= 0) {
    lastClicked.value = index
    focusedIndex.value = index
  }
}

function onOpen(file: FileRecord) {
  openExternal(file.path).catch((e) => emit('notice', '打不开：' + String(e)))
}

async function copyPath(file: FileRecord) {
  try {
    await navigator.clipboard.writeText(file.path)
    emit('notice', '路径已复制')
  } catch {
    emit('notice', '复制失败，路径：' + file.path)
  }
}

function menuItems(file: FileRecord): MenuItem[] {
  return [
    { key: 'open', label: '用系统看图程序打开' },
    { key: 'reveal', label: '在资源管理器中显示' },
    { key: 'copy', label: '复制文件路径' },
    { key: 'select', label: store.selected.has(file.id) ? '取消选中' : '选中这张' },
    { key: 'quarantine', label: '移到隔离区', danger: true },
  ]
}

function onMenuPick(key: string) {
  const file = menu.value?.file
  if (!file) return
  if (key === 'open') onOpen(file)
  else if (key === 'reveal') revealInExplorer(file.path).catch(() => undefined)
  else if (key === 'copy') void copyPath(file)
  else if (key === 'select') store.toggle(file.id)
  else if (key === 'quarantine') emit('quarantine', [file.id])
}

/** 键盘导航：把焦点格滚进可视区 */
async function focusTo(index: number) {
  const max = store.files.length - 1
  if (max < 0) return
  const next = Math.max(0, Math.min(max, index))
  focusedIndex.value = next
  // 需要的数据还没加载就先取一批
  if (next >= store.files.length - columns.value * 2 && store.hasMore) {
    await store.loadMore()
  }
  const el = scroller.value
  if (!el) return
  await nextTick()
  const row = Math.floor(next / columns.value)
  const top = row * rowPitch.value
  if (top < el.scrollTop) el.scrollTop = top
  else if (top + rowPitch.value > el.scrollTop + el.clientHeight) {
    el.scrollTop = top + rowPitch.value - el.clientHeight
  }
  scrollTop.value = el.scrollTop
}

/**
 * 翻页：直接按容器滚一屏（留一行重叠），比「把某个格子滚进视野」更像翻页——
 * 后者在目标行本来就可见时一格都不会动。
 */
function page(direction: 1 | -1) {
  interruptAuto()
  const el = scroller.value
  if (!el) return
  const step = Math.max(rowPitch.value, el.clientHeight - rowPitch.value)
  const max = Math.max(0, el.scrollHeight - el.clientHeight)
  el.scrollTop = Math.max(0, Math.min(max, el.scrollTop + direction * step))
  scrollTop.value = el.scrollTop
  // 焦点格跟着翻页走，键盘操作不丢
  if (store.files.length) {
    const firstRow = Math.max(0, Math.round(el.scrollTop / rowPitch.value))
    focusedIndex.value = Math.min(store.files.length - 1, firstRow * columns.value)
  }
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - rowPitch.value * 2) void store.loadMore()
}

/** End「一路到底」的自动加载开关：用户任何其它操作都会把它关掉 */
let autoScroll = false

function interruptAuto() {
  autoScroll = false
}

/**
 * End 键：滚到最底，并且一路上不断取下一批，直到全部取完。
 * 中途用户滚滑轮、点鼠标、按别的键（或者换页签）就立刻停。
 */
async function jumpToEnd() {
  const el = scroller.value
  if (!el) return
  autoScroll = true
  // 一路到底时用大批次（默认 300 太碎，十万张要跑几百趟、每趟还要按 OFFSET 跳行）
  const BIG = 3000
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
  try {
    let idle = 0
    while (autoScroll && store.hasMore) {
      const before = store.files.length
      await store.loadMore(BIG)
      if (!autoScroll) return
      await nextTick()
      el.scrollTop = el.scrollHeight
      scrollTop.value = el.scrollTop
      // 这批没进来（可能刚好在刷新，或又到底了）：最多空等 3 次就收手
      if (store.files.length === before) {
        idle++
        if (idle >= 3) break
        await sleep(150)
      } else {
        idle = 0
      }
    }
    if (!autoScroll) return
    await nextTick()
    el.scrollTop = el.scrollHeight
    scrollTop.value = el.scrollTop
    if (store.files.length) focusedIndex.value = store.files.length - 1
  } finally {
    autoScroll = false
  }
}

function onKeydown(e: KeyboardEvent) {
  const cols = columns.value
  const cur = focusedIndex.value
  const n = store.files.length

  // 除了 End 自己，按任何其它键都算打断「一路到底」的自动加载
  if (e.key !== 'End') interruptAuto()

  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
    e.preventDefault()
    store.selectAllLoaded()
    emit('notice', `已全选当前加载的 ${n} 张`)
    return
  }
  if (e.key === 'Escape') {
    store.clearSelection()
    return
  }
  if (e.key === ' ') {
    if (cur >= 0 && cur < n) {
      e.preventDefault()
      previewIndex.value = cur
    }
    return
  }
  if (e.key === 'Enter') {
    if (cur >= 0 && cur < n) {
      e.preventDefault()
      onOpen(store.files[cur])
    }
    return
  }
  if (e.key === 'Delete' && store.selectedCount > 0) {
    e.preventDefault()
    emit('quarantine', [...store.selected])
    return
  }
  if (e.key === 'PageDown' || e.key === 'PageUp') {
    e.preventDefault()
    page(e.key === 'PageDown' ? 1 : -1)
    return
  }
  if (e.key === 'Home') {
    e.preventDefault()
    interruptAuto()
    const el = scroller.value
    if (el) {
      el.scrollTop = 0
      scrollTop.value = 0
    }
    void focusTo(0)
    return
  }
  if (e.key === 'End') {
    e.preventDefault()
    void jumpToEnd()
    return
  }
  const step =
    e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowDown' ? cols : e.key === 'ArrowUp' ? -cols : 0
  if (step !== 0) {
    e.preventDefault()
    void focusTo(cur < 0 ? 0 : cur + step)
  }
}

function navigatePreview(delta: number) {
  const n = store.files.length
  if (!n) return
  previewIndex.value = Math.max(0, Math.min(n - 1, previewIndex.value + delta))
}

/** 预览里直接移到隔离区：交给 App 搬，搬完同一位置就是下一张 */
function onPreviewQuarantine(id: number) {
  emit('quarantine', [id])
}
</script>

<template>
  <div
    ref="scroller"
    class="grid-scroll"
    tabindex="0"
    role="listbox"
    aria-label="图片列表"
    aria-multiselectable="true"
    @scroll.passive="onScroll"
    @keydown="onKeydown"
    @wheel.passive="interruptAuto"
    @mousedown="interruptAuto"
  >
    <div class="grid-inner" :style="{ height: totalH + 'px' }">
      <div
        class="grid-offset"
        :style="{
          transform: `translateY(${startRow * rowPitch}px)`,
          gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
        }"
      >
        <ThumbCell
          v-for="(f, i) in visible"
          :key="f.id"
          :file="f"
          :selected="store.selected.has(f.id)"
          :focused="focusedIndex === firstVisibleIndex + i"
          :dup-count="dupCount.get(f.id) ?? 1"
          :gray-threshold="props.grayThreshold"
          @select="onSelect"
          @open="onOpen"
          @menu="(file, ev) => (menu = { file, x: ev.clientX, y: ev.clientY })"
          @hover="(file, el) => (tip = { file, anchor: el.getBoundingClientRect() })"
          @leave="tip = null"
        />
      </div>
    </div>

    <p v-if="!store.files.length && !store.loading" class="empty">
      当前条件没有命中任何图片
    </p>
    <p v-else-if="store.loading" class="empty">正在查询…</p>
    <p v-else-if="store.loadingMore" class="foot">正在加载更多…</p>
    <p v-else-if="!store.hasMore && store.files.length" class="foot">
      已经到底了，共 {{ store.files.length.toLocaleString('zh-CN') }} 张
    </p>

    <ThumbTip
      v-if="tip"
      :file="tip.file"
      :anchor="tip.anchor"
      :dup-count="dupCount.get(tip.file.id) ?? 1"
      :gray-threshold="props.grayThreshold"
    />
    <ContextMenu
      v-if="menu"
      :x="menu.x"
      :y="menu.y"
      :items="menuItems(menu.file)"
      @pick="onMenuPick"
      @close="menu = null"
    />
    <PreviewOverlay
      v-if="previewFile"
      :file="previewFile"
      @close="previewIndex = -1"
      @navigate="navigatePreview"
      @quarantine="onPreviewQuarantine"
    />
  </div>
</template>

<style scoped>
.grid-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 12px 10px 18px;
  position: relative;
  outline: none;
}
.grid-inner {
  position: relative;
}
.grid-offset {
  display: grid;
  /* 12px：选中的格子会放大 4.5%，留出余量后相邻两张不会互相压住 */
  gap: 12px;
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  /* 选中的格子会放大，容器不能裁掉它 */
  overflow: visible;
}
.empty,
.foot {
  text-align: center;
  color: var(--dim);
  font-size: 12px;
  margin: 18px 0 26px;
}
.empty {
  margin-top: 60px;
  font-size: 13px;
}
</style>
