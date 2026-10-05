<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import ThumbCell from './ThumbCell.vue'
import { useLibrary } from '../stores/library'

const store = useLibrary()
const scroller = ref<HTMLElement | null>(null)
const scrollTop = ref(0)
const viewportH = ref(800)
const CELL_W = 148
const CELL_H = 200

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

const dupCount = ref<Map<number, number>>(new Map())

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

// 文件列表一变就重算角标
watch(() => store.files, computeDupCounts, { immediate: true })

function onScroll() {
  const el = scroller.value
  if (!el) return
  scrollTop.value = el.scrollTop
  viewportH.value = el.clientHeight
}

function measure() {
  const el = scroller.value
  if (!el) return
  columns.value = Math.max(1, Math.floor(el.clientWidth / CELL_W))
  viewportH.value = el.clientHeight
}

onMounted(() => {
  measure()
  store.refresh()
  window.addEventListener('resize', measure)
})

onUnmounted(() => {
  window.removeEventListener('resize', measure)
})
</script>

<template>
  <div ref="scroller" class="grid-scroll" @scroll.passive="onScroll">
    <div class="grid-inner" :style="{ height: totalH + 'px' }">
      <div
        class="grid-offset"
        :style="{
          transform: `translateY(${startRow * CELL_H}px)`,
          gridTemplateColumns: `repeat(${columns}, 1fr)`,
        }"
      >
        <ThumbCell
          v-for="f in visible"
          :key="f.id"
          :file="f"
          :selected="store.selected.has(f.id)"
          :dup-count="dupCount.get(f.id) ?? 1"
          :gray-threshold="store.filter.onlyGray ? 999 : 8"
          @toggle="store.toggle"
        />
      </div>
    </div>
    <p v-if="!store.files.length && !store.loading" class="empty">当前条件没有命中任何图片</p>
  </div>
</template>

<style scoped>
.grid-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 10px;
  position: relative;
}
.grid-inner {
  position: relative;
}
.grid-offset {
  display: grid;
  gap: 8px;
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
}
.empty {
  text-align: center;
  opacity: 0.5;
  margin-top: 60px;
}
</style>
