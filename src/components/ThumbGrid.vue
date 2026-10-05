<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { openExternal, revealInExplorer } from '../api'
import ContextMenu, { type MenuItem } from './ContextMenu.vue'
import PreviewOverlay from './PreviewOverlay.vue'
import ThumbCell from './ThumbCell.vue'
import ThumbTip from './ThumbTip.vue'
import { useLibrary } from '../stores/library'
import type { FileRecord } from '../types'

const props = withDefaults(defineProps<{ grayThreshold?: number }>(), { grayThreshold: 8 })
const emit = defineEmits<{
  (e: 'notice', text: string): void
  (e: 'quarantine', ids: number[]): void
}>()

const store = useLibrary()
const scroller = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const viewportH = ref(800)
const CELL_W = 152
const CELL_H = 205

const columns = ref(6)
const rowCount = computed(() => Math.ceil(store.files.length / columns.value))
const totalH = computed(() => rowCount.value * CELL_H)
const startRow = computed(() => Math.max(0, Math.floor(scrollTop.value / CELL_H) - 2))
const endRow = computed(() =>
  Math.min(rowCount.value, Math.ceil((scrollTop.value + viewportH.value) / CELL_H) + 2),
)
const visible = computed(() =>
  store.files.slice(startRow.value * columns.value, endRow.value * columns.value),
)
const firstVisibleIndex = computed(() => startRow.value * columns.value)

const dupCount = ref<Map<number, number>>(new Map())
const focusedIndex = ref(-1)
const lastClicked = ref(-1)
const tip = ref<{ file: FileRecord; anchor: DOMRect } | null>(null)
const menu = ref<{ file: FileRecord; x: number; y: number } | null>(null)
const previewIndex = ref(-1)

const previewFile = computed(() =>
  previewIndex.value >= 0 ? (store.files[previewIndex.value] ?? null) : null,
)

/** 用 content_hash 统计每张图的同内容成员数，供 ×N 角标使用。 */
function computeDupCounts() {
  const byHash = new Map<string, number>()
  for (const f of store.files) {
    if (f.contentHash) byHash.set(f.contentHash, (byHash.get(f.contentHash) ?? 0) + 1)
  }
  const m = new Map<number, number>()
  for (const f of store.files) {
    if (f.contentHash) m.set(f.id, byHash.get(f.contentHash) ?? 1)
  }
  dupCount.value = m
}

watch(() => store.files, computeDupCounts, { immediate: true })

// 结果被整表换掉（改筛选/排序/搜索）时回到顶部，并清掉悬停残留
watch(
  () => store.files[0]?.id,
  () => {
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
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - CELL_H * 2) {
    void store.loadMore()
  }
}

function measure() {
  const el = scroller.value
  if (!el) return
  columns.value = Math.max(1, Math.floor((el.clientWidth - 20) / CELL_W))
  viewportH.value = el.clientHeight
}

function onResize() {
  measure()
}

onMounted(() => {
  measure()
  window.addEventListener('resize', onResize)
})
onUnmounted(() => window.removeEventListener('resize', onResize))

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
  const top = row * CELL_H
  if (top < el.scrollTop) el.scrollTop = top
  else if (top + CELL_H > el.scrollTop + el.clientHeight) {
    el.scrollTop = top + CELL_H - el.clientHeight
  }
}

function onKeydown(e: KeyboardEvent) {
  const cols = columns.value
  const cur = focusedIndex.value
  const n = store.files.length
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
  const step =
    e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowDown' ? cols : e.key === 'ArrowUp' ? -cols : 0
  if (step !== 0) {
    e.preventDefault()
    void focusTo(cur < 0 ? 0 : cur + step)
  } else if (e.key === 'Home') {
    e.preventDefault()
    void focusTo(0)
  } else if (e.key === 'End') {
    e.preventDefault()
    void focusTo(n - 1)
  }
}

function navigatePreview(delta: number) {
  const n = store.files.length
  if (!n) return
  previewIndex.value = Math.max(0, Math.min(n - 1, previewIndex.value + delta))
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
  >
    <div class="grid-inner" :style="{ height: totalH + 'px' }">
      <div
        class="grid-offset"
        :style="{
          transform: `translateY(${startRow * CELL_H}px)`,
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
  gap: 10px;
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
