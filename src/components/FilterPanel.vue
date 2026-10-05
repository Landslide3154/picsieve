<script setup lang="ts">
import { computed, onMounted } from 'vue'
import RangeSlider from './RangeSlider.vue'
import { fmtBytes, fmtCount, fmtPixels } from '../format'
import { useLibrary } from '../stores/library'

const store = useLibrary()
const f = computed(() => store.filter)

// 清晰度刻度：总像素数（宽 × 高），2^12 到 2^30，对数刻度。
// 不用短边：同一幅画的横版竖版短边差很多，总像素更能代表「清晰度」。
const PIX_MIN = 1 << 12
const PIX_MAX = 1 << 30
// 体积刻度：1KB 到 1GB，对数刻度；否则几千张图会全挤在最左边
const SIZE_MIN = 1 << 10
const SIZE_MAX = 1 << 30

const pixelsRange = computed<[number, number]>(() => [
  f.value.minPixels ?? PIX_MIN,
  f.value.maxPixels ?? PIX_MAX,
])
const sizeRange = computed<[number, number]>(() => [
  f.value.minSize ?? SIZE_MIN,
  f.value.maxSize ?? SIZE_MAX,
])

const isAllPixels = computed(() => f.value.minPixels === null && f.value.maxPixels === null)
const isAllSize = computed(() => f.value.minSize === null && f.value.maxSize === null)
/** 格式空数组 = 不限（库里有什么都能看到） */
const isAllExts = computed(() => f.value.exts.length === 0)

/** 某个格式当前是否处于「会显示」的状态 */
function extOn(ext: string): boolean {
  return isAllExts.value || f.value.exts.includes(ext)
}

const fmtSize = (bytes: number) => fmtBytes(bytes)

function onPixelsDrag(v: [number, number]) {
  store.dragFilter({
    minPixels: v[0] > PIX_MIN ? Math.round(v[0]) : null,
    maxPixels: v[1] < PIX_MAX ? Math.round(v[1]) : null,
  })
}

function onSizeDrag(v: [number, number]) {
  store.dragFilter({
    minSize: v[0] > SIZE_MIN ? Math.round(v[0]) : null,
    maxSize: v[1] < SIZE_MAX ? Math.round(v[1]) : null,
  })
}

/** 数字框直接输入：清晰度单位「万像素」，体积单位 MB */
function setPixelsMin(raw: string) {
  const wan = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({
    minPixels: wan === null || Number.isNaN(wan) ? null : Math.max(0, Math.round(wan * 10000)),
  })
}
function setPixelsMax(raw: string) {
  const wan = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({
    maxPixels: wan === null || Number.isNaN(wan) ? null : Math.max(0, Math.round(wan * 10000)),
  })
}
function setSizeMin(raw: string) {
  const mb = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({ minSize: mb === null || Number.isNaN(mb) ? null : Math.round(mb * 1048576) })
}
function setSizeMax(raw: string) {
  const mb = raw.trim() === '' ? null : Number(raw)
  store.applyFilter({ maxSize: mb === null || Number.isNaN(mb) ? null : Math.round(mb * 1048576) })
}

/** 点格式：当前是「不限」时，先切到「除它之外全选」，
 *  这样连点几下就是「把不要的格式逐个去掉」，符合直觉。 */
function toggleExt(ext: string) {
  if (isAllExts.value) {
    store.applyFilter({ exts: store.formats.map((x) => x.ext).filter((x) => x !== ext) })
    return
  }
  const cur = new Set(f.value.exts)
  if (cur.has(ext)) cur.delete(ext)
  else cur.add(ext)
  store.applyFilter({ exts: [...cur] })
}

function showAllExts() {
  store.applyFilter({ exts: [] })
}

function onlyExt(ext: string) {
  store.applyFilter({ exts: [ext] })
}

const wan = (px: number | null) => (px === null ? '' : String(Math.round(px / 10000)))

/** 当前生效的条件，每个都能单独删掉 */
const activeChips = computed(() => {
  const chips: { key: string; text: string; clear: () => Partial<typeof f.value> }[] = []
  if (!isAllPixels.value) {
    chips.push({
      key: 'pixels',
      text: `清晰度 ${fmtPixels(pixelsRange.value[0])} – ${pixelsRange.value[1] === PIX_MAX ? '不限' : fmtPixels(pixelsRange.value[1])}`,
      clear: () => ({ minPixels: null, maxPixels: null }),
    })
  }
  if (!isAllSize.value) {
    chips.push({
      key: 'size',
      text: `体积 ${fmtSize(sizeRange.value[0])} – ${sizeRange.value[1] === SIZE_MAX ? '不限' : fmtSize(sizeRange.value[1])}`,
      clear: () => ({ minSize: null, maxSize: null }),
    })
  }
  if (!isAllExts.value) {
    chips.push({
      key: 'exts',
      text: `格式 ${f.value.exts.map((e) => e.toUpperCase()).join('/')}`,
      clear: () => ({ exts: [] }),
    })
  }
  if (f.value.onlyGray) chips.push({ key: 'gray', text: '只看灰阶', clear: () => ({ onlyGray: false }) })
  if (f.value.onlyDuplicated)
    chips.push({ key: 'dup', text: '只看重复', clear: () => ({ onlyDuplicated: false }) })
  if (f.value.onlyDecodeError)
    chips.push({ key: 'bad', text: '只看读不出的', clear: () => ({ onlyDecodeError: false }) })
  return chips
})

onMounted(() => {
  if (!store.histPixels || !store.histSize) void store.loadHistograms()
  if (!store.formats.length) void store.loadFormats()
})
</script>

<template>
  <aside class="filters">
    <section class="facet">
      <header>
        <b>清晰度</b>
        <span class="val num">
          {{ isAllPixels ? '不限' : `${fmtPixels(pixelsRange[0])} – ${pixelsRange[1] === PIX_MAX ? '不限' : fmtPixels(pixelsRange[1])}` }}
        </span>
      </header>
      <RangeSlider
        :model-value="pixelsRange"
        :min="PIX_MIN"
        :max="PIX_MAX"
        :step="5"
        scale="log"
        label="清晰度"
        :buckets="store.histPixels?.buckets ?? []"
        :edges="store.histPixels?.edges ?? []"
        :format="fmtPixels"
        @update:model-value="onPixelsDrag"
        @change="onPixelsDrag"
      />
      <div class="inputs">
        <input
          type="number"
          min="0"
          :value="wan(f.minPixels)"
          placeholder="不限"
          aria-label="清晰度下限（万像素）"
          @change="setPixelsMin(($event.target as HTMLInputElement).value)"
        />
        <span class="dim">到</span>
        <input
          type="number"
          min="0"
          :value="wan(f.maxPixels)"
          placeholder="不限"
          aria-label="清晰度上限（万像素）"
          @change="setPixelsMax(($event.target as HTMLInputElement).value)"
        />
        <span class="unit">万像素</span>
        <button
          class="btn link sm"
          :disabled="isAllPixels"
          aria-label="重置清晰度条件"
          @click="store.resetFacet({ minPixels: null, maxPixels: null })"
        >
          重置
        </button>
      </div>
      <p class="hint">按整张图的总像素数（宽 × 高）算，灰柱是全库分布</p>
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
      <header>
        <b>格式</b>
        <span class="val">{{ isAllExts ? '全部' : `${f.exts.length} 种` }}</span>
      </header>
      <div class="chips">
        <button class="chip" :class="{ on: isAllExts }" @click="showAllExts">全部</button>
        <button
          v-for="x in store.formats"
          :key="x.ext"
          class="chip"
          :class="{ on: extOn(x.ext) }"
          :title="`单击去掉/加回，双击只看 ${x.ext.toUpperCase()}`"
          @click="toggleExt(x.ext)"
          @dblclick="onlyExt(x.ext)"
        >
          {{ x.ext.toUpperCase() }} <span class="dim tiny num">{{ fmtCount(x.count) }}</span>
        </button>
      </div>
      <p class="hint">按你库里实际有的格式生成（带张数）：单击去掉/加回一种，双击只看这一种</p>
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
        命中 <b>{{ fmtCount(store.total) }}</b> 张
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
  width: 256px;
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
  width: 56px;
  padding: 3px 6px;
  font-size: 12px;
}
.unit {
  font-size: 11px;
  color: var(--dim);
  white-space: nowrap;
}
.inputs :deep(.btn),
.inputs .btn {
  white-space: nowrap;
  flex: none;
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
