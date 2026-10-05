<script setup lang="ts">
import { computed } from 'vue'
import { useLibrary } from '../stores/library'

const store = useLibrary()
const f = computed(() => store.filter)

// 分辨率与体积用「每档一个像素」的整数滑块，避免浮点比较问题
const SIZE_STEPS = [0, 50, 100, 200, 500, 1024, 2048, 5120, 10240, 51200] // KB
const EXTS = ['jpg', 'png', 'gif', 'webp', 'bmp']

function setMinShort(v: number) {
  store.patchFilter({ minShortSide: v || null })
}
function setMaxShort(v: number) {
  store.patchFilter({ maxShortSide: v || null })
}
function setMinSize(i: number) {
  const kb = SIZE_STEPS[i] ?? 0
  store.patchFilter({ minSize: kb > 0 ? kb * 1024 : null })
}
function setMaxSize(i: number) {
  const kb = SIZE_STEPS[i] ?? 0
  store.patchFilter({ maxSize: kb > 0 ? kb * 1024 : null })
}
function toggleExt(e: string) {
  const cur = new Set(f.value.exts)
  if (cur.has(e)) cur.delete(e)
  else cur.add(e)
  store.patchFilter({ exts: [...cur] })
}
</script>

<template>
  <aside class="filters">
    <label class="label">分辨率（短边）</label>
    <input
      type="range"
      min="0"
      max="3000"
      step="50"
      :value="f.minShortSide ?? 0"
      @input="setMinShort(+($event.target as HTMLInputElement).value)"
    />
    <input
      type="range"
      min="0"
      max="3000"
      step="50"
      :value="f.maxShortSide ?? 3000"
      @input="setMaxShort(+($event.target as HTMLInputElement).value)"
    />
    <p class="hint">{{ f.minShortSide ?? 0 }} – {{ f.maxShortSide ?? '不限' }} px</p>

    <label class="label">文件体积</label>
    <input
      type="range"
      min="0"
      max="9"
      step="1"
      :value="1"
      @input="setMinSize(+($event.target as HTMLInputElement).value)"
    />
    <input
      type="range"
      min="0"
      max="9"
      step="1"
      :value="9"
      @input="setMaxSize(+($event.target as HTMLInputElement).value)"
    />
    <p class="hint">拖动后自动重新查询</p>

    <label class="label">格式</label>
    <div class="chips">
      <button
        v-for="e in EXTS"
        :key="e"
        :class="{ on: f.exts.includes(e) }"
        @click="toggleExt(e)"
      >
        {{ e.toUpperCase() }}
      </button>
    </div>

    <label class="label">只看</label>
    <label class="check">
      <input
        type="checkbox"
        :checked="f.onlyGray"
        @change="store.patchFilter({ onlyGray: !f.onlyGray })"
      />
      灰阶图（黑白灰）
    </label>
    <label class="check">
      <input
        type="checkbox"
        :checked="f.onlyDuplicated"
        @change="store.patchFilter({ onlyDuplicated: !f.onlyDuplicated })"
      />
      有重复的
    </label>
    <label class="check">
      <input
        type="checkbox"
        :checked="f.onlyDecodeError"
        @change="store.patchFilter({ onlyDecodeError: !f.onlyDecodeError })"
      />
      读不出的
    </label>

    <button class="reset" @click="store.resetFilter()">重置条件</button>
    <p class="count">命中 {{ store.total }} 张</p>
    <p v-if="store.error" class="error">{{ store.error }}</p>
  </aside>
</template>

<style scoped>
.filters {
  width: 208px;
  flex: none;
  padding: 12px;
  border-right: 1px solid var(--line);
  font-size: 12px;
  overflow-y: auto;
}
.label {
  display: block;
  margin: 14px 0 6px;
  opacity: 0.6;
  font-size: 11px;
  text-transform: uppercase;
}
input[type='range'] {
  width: 100%;
}
.hint {
  margin: 4px 0 0;
  opacity: 0.6;
  font-size: 11px;
}
.chips {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}
.chips button {
  padding: 2px 8px;
  border: 1px solid var(--line);
  border-radius: 3px;
  background: transparent;
  color: inherit;
  cursor: pointer;
}
.chips button.on {
  background: rgba(74, 144, 217, 0.28);
}
.check {
  display: block;
  margin: 4px 0;
}
.reset {
  margin-top: 14px;
  background: transparent;
  border: 1px solid var(--line);
  color: inherit;
  border-radius: 4px;
  padding: 4px 10px;
  cursor: pointer;
}
.count {
  margin-top: 10px;
  font-weight: 600;
}
.error {
  color: #e05c4b;
}
</style>
