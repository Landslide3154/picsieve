<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { getSettings, moveToQuarantine } from './api'
import ActionBar from './components/ActionBar.vue'
import DupGroupView from './components/DupGroupView.vue'
import FilterPanel from './components/FilterPanel.vue'
import QuarantineView from './components/QuarantineView.vue'
import ScanProgress from './components/ScanProgress.vue'
import SettingsView from './components/SettingsView.vue'
import ThumbGrid from './components/ThumbGrid.vue'
import TopBar from './components/TopBar.vue'
import { useLibrary } from './stores/library'
import type { Settings } from './types'

const tab = ref<'library' | 'groups' | 'quarantine' | 'scan' | 'settings'>('library')
const settings = ref<Settings | null>(null)
const notice = ref('')
const store = useLibrary()
let noticeTimer: number | undefined

function showNotice(text: string) {
  notice.value = text
  if (noticeTimer !== undefined) clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => (notice.value = ''), 5000)
}

async function loadSettings() {
  try {
    settings.value = await getSettings()
  } catch {
    /* 设置读不到时用默认值兜底 */
  }
}

onMounted(async () => {
  await loadSettings()
  await Promise.all([store.refreshNow(), store.loadStats(), store.loadHistograms()])
})

// 切回图库时补一次统计（扫描/隔离之后数据变了）
watch(tab, (t) => {
  if (t === 'library') {
    void store.loadStats()
    if (!store.files.length) void store.refreshNow()
  }
})

async function move(ids: number[]) {
  if (!ids.length) return
  try {
    const r = await moveToQuarantine(ids)
    showNotice(`已移入隔离区 ${r.moved} 张${r.failed ? `，失败 ${r.failed} 张` : ''}，可随时搬回`)
    store.clearSelection()
    await store.refreshNow()
    await store.loadStats()
  } catch (e) {
    showNotice('移入失败：' + String(e))
  }
}

function moveSelected() {
  void move([...store.selected])
}

function afterScan() {
  // 扫描/指纹结束后：分布图、库内统计都变了
  void store.loadHistograms()
  void store.loadStats()
  void store.refreshNow()
}
</script>

<template>
  <TopBar v-if="tab === 'library'" :roots="settings?.roots ?? []" />

  <nav class="tabs">
    <button :class="{ on: tab === 'library' }" @click="tab = 'library'">图库</button>
    <button :class="{ on: tab === 'groups' }" @click="tab = 'groups'">重复组</button>
    <button :class="{ on: tab === 'quarantine' }" @click="tab = 'quarantine'">隔离区</button>
    <button :class="{ on: tab === 'scan' }" @click="tab = 'scan'">扫描</button>
    <button :class="{ on: tab === 'settings' }" @click="tab = 'settings'">设置</button>
    <span class="grow" />
    <span v-if="tab !== 'library'" class="meta num">
      库内 {{ store.stats.total.toLocaleString('zh-CN') }} 张 · 已选 {{ store.selectedCount }}
    </span>
  </nav>

  <ScanProgress v-if="tab === 'scan'" @changed="afterScan" />
  <SettingsView v-else-if="tab === 'settings'" @saved="loadSettings" />
  <QuarantineView v-else-if="tab === 'quarantine'" @changed="afterScan" />
  <DupGroupView v-else-if="tab === 'groups'" @move-to-quarantine="move" />
  <template v-else>
    <div class="body">
      <FilterPanel />
      <ThumbGrid :gray-threshold="settings?.grayThreshold ?? 8" @notice="showNotice" @quarantine="move" />
    </div>
    <ActionBar @move-to-quarantine="moveSelected" @select-all="store.selectAllLoaded()" />
  </template>

  <p v-if="notice" class="toast" role="status" aria-live="polite">{{ notice }}</p>
</template>

<style scoped>
.tabs {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 14px;
  border-bottom: 1px solid var(--line);
  background: var(--bg);
}
.tabs button {
  border: none;
  background: none;
  color: var(--dim);
  font: inherit;
  padding: 5px 11px;
  border-radius: var(--radius);
  cursor: pointer;
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
.grow {
  flex: 1;
}
.meta {
  font-size: 12px;
  color: var(--dim);
}
.body {
  display: flex;
  flex: 1;
  min-height: 0;
}
</style>
