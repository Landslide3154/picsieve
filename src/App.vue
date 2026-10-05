<script setup lang="ts">
import { ref } from 'vue'
import { moveToQuarantine } from './api'
import ActionBar from './components/ActionBar.vue'
import DupGroupView from './components/DupGroupView.vue'
import FilterPanel from './components/FilterPanel.vue'
import QuarantineView from './components/QuarantineView.vue'
import ThumbGrid from './components/ThumbGrid.vue'
import ScanProgress from './components/ScanProgress.vue'
import { useLibrary } from './stores/library'

const tab = ref<'library' | 'groups' | 'quarantine' | 'scan'>('library')
const notice = ref('')
const store = useLibrary()

function showNotice(text: string) {
  notice.value = text
  window.setTimeout(() => (notice.value = ''), 5000)
}

async function move(ids: number[]) {
  if (!ids.length) return
  try {
    const r = await moveToQuarantine(ids)
    showNotice(`已移入隔离区 ${r.moved} 张${r.failed ? `，失败 ${r.failed} 张` : ''}`)
    store.clearSelection()
    await store.refresh()
  } catch (e) {
    showNotice('移入失败：' + String(e))
  }
}

function moveSelected() {
  void move([...store.selected])
}
</script>

<template>
  <header class="top">
    <strong>图筛 PicSieve</strong>
    <nav>
      <button :class="{ on: tab === 'library' }" @click="tab = 'library'">图库</button>
      <button :class="{ on: tab === 'groups' }" @click="tab = 'groups'">重复组</button>
      <button :class="{ on: tab === 'quarantine' }" @click="tab = 'quarantine'">隔离区</button>
      <button :class="{ on: tab === 'scan' }" @click="tab = 'scan'">扫描</button>
    </nav>
    <span class="flex" />
    <span v-if="notice" class="notice">{{ notice }}</span>
  </header>

  <ScanProgress v-if="tab === 'scan'" />
  <QuarantineView v-else-if="tab === 'quarantine'" />
  <DupGroupView v-else-if="tab === 'groups'" @move-to-quarantine="move" />
  <template v-else>
    <div class="body">
      <FilterPanel />
      <ThumbGrid />
    </div>
    <ActionBar @move-to-quarantine="moveSelected" />
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
