<script setup lang="ts">
import { fmtBytes, fmtCount } from '../format'
import { useLibrary } from '../stores/library'

const store = useLibrary()
defineEmits<{
  (e: 'move-to-quarantine'): void
  (e: 'select-all'): void
}>()
</script>

<template>
  <footer class="bar">
    <span class="num">
      已选 <b>{{ fmtCount(store.selectedCount) }}</b> 张 ·
      <b>{{ fmtBytes(store.selectedBytes) }}</b>
    </span>
    <button class="btn ghost sm" @click="$emit('select-all')">全选已加载</button>
    <button class="btn ghost sm" :disabled="!store.selectedCount" @click="store.clearSelection()">
      取消选择
    </button>
    <span class="grow" />
    <span class="dim tiny">单击选中 · Shift 连选 · Ctrl+A 全选 · 空格预览 · Delete 移入隔离区</span>
    <button class="btn danger" :disabled="!store.selectedCount" @click="$emit('move-to-quarantine')">
      移到隔离区
    </button>
  </footer>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  border-top: 1px solid var(--line);
  background: var(--panel);
}
.grow {
  flex: 1;
}
</style>
