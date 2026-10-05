<script setup lang="ts">
import { computed, onMounted } from 'vue'
import RangeSlider from './RangeSlider.vue'
import { EXTS, useLibrary } from '../stores/library'

const store = useLibrary()
const f = computed(() => store.filter)

// 分辨率刻度：0–8000px，每 50px 一格（实测最大短边 7156px）
const SHORT_MIN = 0
const SHORT_MAX = 8000
// 体积刻度：1KB–1GB，对数刻度；否则几千张图会全挤在最左边
const SIZE_MIN = 1 << 10
const SIZE_MAX = 1 << 30

const shortRange = computed<[number, number]>(() => [
  f.value.minShortSide ?? SHORT_MIN,
  f.value.maxShortSide ?? SHORT_MAX,
])
const sizeRange = computed<[number, number]>(() => [
  f.value.minSize ?? SIZE_MIN,
  f.value.maxSize ?? SIZE_MAX,
])

const isAllShort = computed(
  () => f.value.minShortSide === null && f.value.maxShortSide === null,
)
const isAllSize = computed(() => f.value.minSize === null && f.value.maxSize === null)

function fmtSize(bytes: number): string {
  if (bytes >= 1 << 30) return (bytes / (1 << 30)).toFixed(2) + ' GB'
  if (bytes >= 1 << 20) {
    const mb = bytes / (1 << 20)
    return (mb >= 100 ? mb.toFixed(0) : mb.toFixed(1)) + ' MB'
  }
  return Math.round(bytes / 1024) + ' KB'
}

function fmtShort(v: number): string {
  return v === SHORT_MAX ? '不限' : v + ' px'
}

function onShortDrag(v: [number, number]) {
  store.dragFilter({
    minShortSide: v[0] > SHORT_MIN ? Math.round(v[0]) : null,
    maxShortSide: v[1] < SHORT_MAX ? Math.round(v[1]) : null,
  })
}

function onSizeDrag(v: [number, number]) {
  store.dragFilter({
    minSize: v[0] > SIZE_MIN ? Math.round(v[0]) : null,
    maxSize: v[1] < SIZE_MAX ? Math.round(v[1]) : null,
  })
}

/** 数字框直接输入：短边单位 px，体积单位 MB */
function setShortMin(raw: string) {
  const v = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({ minShortSide: v === null || Number.isNaN(v) ? null : Math.max(0, v) })
}
function setShortMax(raw: string) {
  const v = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({ maxShortSide: v === null || Number.isNaN(v) ? null : Math.max(0, v) })
}
function setSizeMin(raw: string) {
  const mb = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({ minSize: mb === null || Number.isNaN(mb) ? null : Math.round(mb * 1048576) })
}
function setSizeMax(raw: string) {
  const mb = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({ maxSize: mb === null || Number.isNaN(mb) ? null : Math.round(mb * 1048576) })
}

function toggleExt(e: string) {
  const cur = new Set(f.value.exts)
  if (cur.has(e)) cur.delete(e)
  else cur.add(e)
  store.applyFilter({ exts: [...cur] })
}

/** 当前生效的条件，每个都能单独删掉 */
const activeChips = computed(() => {
  const chips: { key: string; text: string; clear: () => Partial<typeof f.value> }[] = []
  if (!isAllShort.value) {
    chips.push({
      key: 'short',
      text: `短边 ${shortRange.value[0]}–${shortRange.value[1] === SHORT_MAX ? '不限' : shortRange.value[1]} px`,
      clear: () => ({ minShortSide: null, maxShortSide: null }),
    })
  }
  if (!isAllSize.value) {
    chips.push({
      key: 'size',
      text: `体积 ${fmtSize(sizeRange.value[0])} – ${sizeRange.value[1] === SIZE_MAX ? '不限' : fmtSize(sizeRange.value[1])}`,
      clear: () => ({ minSize: null, maxSize: null }),
    })
  }
  for (const e of f.value.exts) {
    chips.push({
      key: 'ext-' + e,
      text: e.toUpperCase(),
      clear: () => ({ exts: f.value.exts.filter((x) => x !== e) }),
    })
  }
  if (f.value.onlyGray) chips.push({ key: 'gray', text: '只看灰阶', clear: () => ({ onlyGray: false }) })
  if (f.value.onlyDuplicated)
    chips.push({ key: 'dup', text: '只看重复', clear: () => ({ onlyDuplicated: false }) })
  if (f.value.onlyDecodeError)
    chips.push({ key: 'bad', text: '只看读不出的', clear: () => ({ onlyDecodeError: false }) })
  if (f.value.search)
    chips.push({ key: 'search', text: `搜索「${f.value.search}」`, clear: () => ({ search: null }) })
  return chips
})

onMounted(() => {
  if (!store.histShort || !store.histSize) void store.loadHistograms()
})
</script>

<template>
  <aside class="filters">
    <section class="facet">
      <header>
        <b>分辨率（短边）</b>
        <span class="val num">{{ isAllShort ? '不限' : `${shortRange[0]}–${shortRange[1]} px` }}</span>
      </header>
      <RangeSlider
        :model-value="shortRange"
        :min="SHORT_MIN"
        :max="SHORT_MAX"
        :step="50"
        label="分辨率短边"
        :buckets="store.histShort?.buckets ?? []"
        :edges="store.histShort?.edges ?? []"
        :format="fmtShort"
        @update:model-value="onShortDrag"
        @change="onShortDrag"
      />
      <div class="inputs">
        <input
          type="number"
          min="0"
          :max="SHORT_MAX"
          :value="f.minShortSide ?? ''"
          placeholder="不限"
          aria-label="短边下限（像素）"
          @change="setShortMin(($event.target as HTMLInputElement).value)"
        />
        <span class="dim">到</span>
        <input
          type="number"
          min="0"
          :max="SHORT_MAX"
          :value="f.maxShortSide ?? ''"
          placeholder="不限"
          aria-label="短边上限（像素）"
          @change="setShortMax(($event.target as HTMLInputElement).value)"
        />
        <span class="unit">px</span>
        <button
          class="btn link sm"
          :disabled="isAllShort"
          aria-label="重置分辨率条件"
          @click="store.resetFacet({ minShortSide: null, maxShortSide: null })"
        >
          重置
        </button>
      </div>
      <p class="hint">灰柱是全库在这条刻度上的分布，蓝色段是当前选中的范围</p>
    </section>

    <section class="facet">
      <header>
        <b>文件体积</b>
        <span class="val num">
          {{ isAllSize ? '不限' : `${fmtSize(sizeRange[0])} – ${sizeRange[1] === SIZE_MAX ? '不限' : fmtSize(sizeRange[1])}` }}
        </span>
      </header>
      <RangeSlider
        :model-value="sizeRange"
        :min="SIZE_MIN"
        :max="SIZE_MAX"
        :step="5"
        scale="log"
        label="文件体积"
        :buckets="store.histSize?.buckets ?? []"
        :edges="store.histSize?.edges ?? []"
        :format="fmtSize"
        @update:model-value="onSizeDrag"
        @change="onSizeDrag"
      />
      <div class="inputs">
        <input
          type="number"
          min="0"
          :value="f.minSize === null ? '' : (f.minSize / 1048576).toFixed(1)"
          placeholder="不限"
          aria-label="体积下限（MB）"
          @change="setSizeMin(($event.target as HTMLInputElement).value)"
        />
        <span class="dim">到</span>
        <input
          type="number"
          min="0"
          :value="f.maxSize === null ? '' : (f.maxSize / 1048576).toFixed(1)"
          placeholder="不限"
          aria-label="体积上限（MB）"
          @change="setSizeMax(($event.target as HTMLInputElement).value)"
        />
        <span class="unit">MB</span>
        <button
          class="btn link sm"
          :disabled="isAllSize"
          aria-label="重置体积条件"
          @click="store.resetFacet({ minSize: null, maxSize: null })"
        >
          重置
        </button>
      </div>
      <p class="hint">刻度是对数的：每一档宽度相同，否则小图会全挤在左边</p>
    </section>

    <section class="facet">
      <header><b>格式</b></header>
      <div class="chips">
        <button
          v-for="e in EXTS"
          :key="e"
          class="chip"
          :class="{ on: f.exts.includes(e) }"
          @click="toggleExt(e)"
        >
          {{ e.toUpperCase() }}
        </button>
      </div>
    </section>

    <section class="facet">
      <header><b>只看</b></header>
      <label class="check" :class="{ on: f.onlyGray }">
        <input
          type="checkbox"
          :checked="f.onlyGray"
          @change="store.applyFilter({ onlyGray: !f.onlyGray })"
        />
        灰阶图（黑白灰）
      </label>
      <label class="check" :class="{ on: f.onlyDuplicated }">
        <input
          type="checkbox"
          :checked="f.onlyDuplicated"
          @change="store.applyFilter({ onlyDuplicated: !f.onlyDuplicated })"
        />
        有重复的
      </label>
      <label class="check" :class="{ on: f.onlyDecodeError }">
        <input
          type="checkbox"
          :checked="f.onlyDecodeError"
          @change="store.applyFilter({ onlyDecodeError: !f.onlyDecodeError })"
        />
        读不出的
      </label>
    </section>

    <section class="facet result">
      <p class="hit num">
        命中 <b>{{ store.total.toLocaleString('zh-CN') }}</b> 张
        <em v-if="store.total > store.files.length">（已载入 {{ store.files.length }}）</em>
      </p>
      <div v-if="activeChips.length" class="active">
        <button
          v-for="c in activeChips"
          :key="c.key"
          class="tag"
          :title="'移除条件：' + c.text"
          @click="store.applyFilter(c.clear())"
        >
          {{ c.text }} <span aria-hidden="true">×</span>
        </button>
      </div>
      <button class="btn ghost block" :disabled="!activeChips.length" @click="store.resetAll()">
        全部重置
      </button>
    </section>

    <p v-if="store.error" class="error-text small">{{ store.error }}</p>
  </aside>
</template>

<style scoped>
.filters {
  width: 244px;
  flex: none;
  padding: 14px 14px 20px;
  border-right: 1px solid var(--line);
  background: var(--panel);
  overflow-y: auto;
}
.facet {
  margin-bottom: 18px;
}
.facet header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.facet b {
  font-size: 13px;
}
.val {
  font-size: 11px;
  color: var(--dim);
}
.inputs {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
}
.inputs input {
  width: 62px;
  padding: 3px 7px;
  font-size: 12px;
}
.unit {
  font-size: 11px;
  color: var(--dim);
}
.hint {
  margin: 6px 0 0;
  font-size: 11px;
  color: var(--dimmer);
  line-height: 1.45;
}
.chips {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.result {
  border-top: 1px solid var(--line);
  padding-top: 14px;
}
.hit {
  margin: 0 0 8px;
  font-size: 15px;
}
.hit b {
  font-size: 17px;
}
.hit em {
  font-style: normal;
  font-size: 11px;
  color: var(--dim);
}
.active {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  margin-bottom: 10px;
}
.tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--accent-soft);
  color: #bcd9ff;
  border: 1px solid rgba(108, 176, 255, 0.3);
  cursor: pointer;
}
.tag:hover {
  background: rgba(108, 176, 255, 0.28);
}
.block {
  width: 100%;
  justify-content: center;
}
</style>
