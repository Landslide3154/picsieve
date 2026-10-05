<script setup lang="ts">
import { ref } from 'vue'
import ActionBar from './components/ActionBar.vue'
import FilterPanel from './components/FilterPanel.vue'
import ThumbGrid from './components/ThumbGrid.vue'
import ScanProgress from './components/ScanProgress.vue'

const tab = ref<'library' | 'scan'>('library')
const notice = ref('')

function moveToQuarantine() {
  // 隔离区在任务 17/18 接入，这里先给出明确反馈，不做任何文件操作
  notice.value = '隔离区功能还在接入中，暂时不会移动任何文件'
  window.setTimeout(() => (notice.value = ''), 3000)
}
</script>

<template>
  <header class="top">
    <strong>图筛 PicSieve</strong>
    <nav>
      <button :class="{ on: tab === 'library' }" @click="tab = 'library'">图库</button>
      <button :class="{ on: tab === 'scan' }" @click="tab = 'scan'">扫描</button>
    </nav>
    <span class="flex" />
    <span v-if="notice" class="notice">{{ notice }}</span>
  </header>

  <ScanProgress v-if="tab === 'scan'" />
  <template v-else>
    <div class="body">
      <FilterPanel />
      <ThumbGrid />
    </div>
    <ActionBar @move-to-quarantine="moveToQuarantine" />
  </template>
</template>

<style scoped>
.top {
  display: flex;
  align-items: center;
  gap: 18px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--line);
}
nav button {
  background: none;
  border: none;
  color: inherit;
  opacity: 0.6;
  cursor: pointer;
  padding: 4px 8px;
}
nav button.on {
  opacity: 1;
  border-bottom: 2px solid var(--accent);
}
.flex {
  flex: 1;
}
.notice {
  font-size: 12px;
  color: var(--accent);
}
.body {
  display: flex;
  flex: 1;
  min-height: 0;
}
</style>
