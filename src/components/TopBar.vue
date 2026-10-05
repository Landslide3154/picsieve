<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { fmtBytes, fmtCount, fmtRelative } from '../format'
import { SORT_OPTIONS, useLibrary } from '../stores/library'

const props = defineProps<{ roots: string[] }>()
const store = useLibrary()

const search = ref(store.filter.search ?? '')
let timer: number | undefined

// 搜索防抖 300ms：边打字边查会一秒发好几次（每次两条 SQL）
watch(search, (v) => {
  if (timer !== undefined) clearTimeout(timer)
  timer = window.setTimeout(() => {
    const cur = store.filter.search ?? ''
    const next = v.trim() || null
    if (cur !== next) void store.applyFilter({ search: next })
  }, 300)
})

function clearSearch() {
  search.value = ''
  if (store.filter.search) void store.applyFilter({ search: null })
}

const rootName = computed(() => {
  const r = props.roots[0]
  if (!r) return '还没有添加扫描目录'
  const parts = r.split(/[\\/]/).filter(Boolean)
  const tail = parts[parts.length - 1] ?? r
  return props.roots.length > 1 ? `${tail} 等 ${props.roots.length} 个目录` : tail
})

/** 用 Intl 算相对时间，别自己拼字符串（规范：日期时间一律走 Intl） */
const lastScanText = computed(() => fmtRelative(store.stats.lastScanAt, '扫描'))
</script>

<template>
  <header class="top">
    <span class="brand">图筛 PicSieve</span>

    <label class="search" title="按文件名或路径搜索">
      <span aria-hidden="true">🔍</span>
      <input
        v-model="search"
        type="search"
        placeholder="搜文件名或路径…"
        aria-label="搜索文件名或路径"
        @keydown.esc="clearSearch"
      />
      <button v-if="search" class="clear" aria-label="清空搜索" @click="clearSearch">×</button>
    </label>

    <label class="sort">
      <span class="dim small">排序</span>
      <select
        :value="store.filter.sort"
        aria-label="排序方式"
        @change="store.applyFilter({ sort: ($event.target as HTMLSelectElement).value })"
      >
        <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">{{ o.label }}</option>
      </select>
    </label>

    <span class="grow" />

    <span class="meta num" :title="props.roots.join('\n')">
      {{ rootName }} · 库内 {{ fmtCount(store.stats.total) }} 张 ·
      {{ fmtBytes(store.stats.bytes) }} · {{ lastScanText }}
    </span>
  </header>
</template>

<style scoped>
.top {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 8px 14px;
  background: var(--panel);
  border-bottom: 1px solid var(--line);
}
.brand {
  font-weight: 700;
  letter-spacing: 0.02em;
  white-space: nowrap;
}
.search {
  display: flex;
  align-items: center;
  gap: 6px;
  background: var(--sunken);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  padding: 4px 8px;
  width: 280px;
}
.search:hover {
  border-color: var(--line-strong);
}
.search input {
  border: none;
  background: none;
  padding: 2px 0;
  width: 100%;
}
.search input:focus-visible {
  outline: none;
}
.clear {
  border: none;
  background: none;
  color: var(--dim);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  padding: 0 2px;
}
.clear:hover {
  color: var(--fg);
}
.sort {
  display: flex;
  align-items: center;
  gap: 6px;
}
.meta {
  font-size: 12px;
  color: var(--dim);
  white-space: nowrap;
}
.grow {
  flex: 1;
}
</style>
