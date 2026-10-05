<script setup lang="ts">
import { SORT_OPTIONS, useLibrary } from '../stores/library'

export type TabKey = 'library' | 'groups' | 'quarantine' | 'scan' | 'settings'

const props = defineProps<{ tab: TabKey }>()
const emit = defineEmits<{ (e: 'update:tab', tab: TabKey): void }>()

const store = useLibrary()

const TABS: { key: TabKey; label: string }[] = [
  { key: 'library', label: '图库' },
  { key: 'groups', label: '重复组' },
  { key: 'quarantine', label: '隔离区' },
  { key: 'scan', label: '扫描' },
  { key: 'settings', label: '设置' },
]
</script>

<template>
  <header class="top">
    <!-- 三栏布局：左右各占一份，中间那栏永远居中，切页签时按钮不会跑 -->
    <div class="side left">
      <span class="brand">图筛 PicSieve</span>
    </div>

    <nav class="tabs" role="tablist" aria-label="主功能切换">
      <button
        v-for="t in TABS"
        :key="t.key"
        role="tab"
        :aria-selected="props.tab === t.key"
        :class="{ on: props.tab === t.key }"
        @click="emit('update:tab', t.key)"
      >
        {{ t.label }}
      </button>
    </nav>

    <div class="side right">
      <label v-if="props.tab === 'library'" class="sort">
        <span class="dim small">排序</span>
        <select
          :value="store.filter.sort"
          aria-label="排序方式"
          @change="store.applyFilter({ sort: ($event.target as HTMLSelectElement).value })"
        >
          <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
      </label>
    </div>
  </header>
</template>

<style scoped>
.top {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  gap: 12px;
  padding: 7px 14px;
  background: var(--panel);
  border-bottom: 1px solid var(--line);
}
.side {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.left {
  justify-content: flex-start;
}
.right {
  justify-content: flex-end;
}
.brand {
  font-weight: 700;
  letter-spacing: 0.02em;
  white-space: nowrap;
}
.tabs {
  display: flex;
  gap: 4px;
  justify-content: center;
}
.tabs button {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  border: none;
  background: none;
  color: var(--dim);
  font: inherit;
  padding: 4px 12px;
  border-radius: var(--radius);
  cursor: pointer;
  white-space: nowrap;
  transition:
    background-color var(--speed) ease,
    color var(--speed) ease;
}
.tabs button:hover {
  color: var(--fg);
  background: rgba(150, 160, 175, 0.1);
}
.tabs button.on {
  color: var(--fg);
  background: var(--accent-soft);
  box-shadow: inset 0 -2px 0 var(--accent);
}
.sort {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
}
.sort select {
  padding: 3px 6px;
  font-size: 12px;
}
</style>
