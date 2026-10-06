<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'
import { getSettings, moveToQuarantine, showMainWindow } from './api'
import ActionBar from './components/ActionBar.vue'
import DupGroupView from './components/DupGroupView.vue'
import FilterPanel from './components/FilterPanel.vue'
import QuarantineView from './components/QuarantineView.vue'
import ScanProgress from './components/ScanProgress.vue'
import SettingsView from './components/SettingsView.vue'
import ThumbGrid from './components/ThumbGrid.vue'
import TopBar, { type TabKey } from './components/TopBar.vue'
import { useGroups } from './stores/groups'
import { useLibrary } from './stores/library'
import type { Settings } from './types'

const tab = ref<TabKey>('library')
const settings = ref<Settings | null>(null)
const notice = ref('')
/** 搬过文件/重新扫描后自增，让重复组页知道该重新加载 */
const dataVersion = ref(0)
const store = useLibrary()
const groups = useGroups()
let noticeTimer: number | undefined

function showNotice(text: string, ms = 5000) {
  notice.value = text
  if (noticeTimer !== undefined) clearTimeout(noticeTimer)
  noticeTimer = window.setTimeout(() => (notice.value = ''), ms)
}

async function loadSettings() {
  try {
    settings.value = await getSettings()
  } catch {
    /* 设置读不到时用默认值兜底 */
  }
}

onMounted(async () => {
  // 先让窗口露脸，再慢慢加载数据：窗口在配置里是隐藏的，
  // 等 window-state 插件恢复完上次的位置/大小，第一帧画好就显示，避免闪一下。
  requestAnimationFrame(() => {
    void showMainWindow().catch(() => {})
  })
  await loadSettings()
  await Promise.all([
    store.refreshNow(),
    store.loadStats(),
    store.loadHistograms(),
    store.loadFormats(),
  ])
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
  const moved = new Set(ids)
  const before = new Set(store.files.map((f) => f.id))
  try {
    const r = await moveToQuarantine(ids)
    store.clearSelection()
    await store.refreshNow()
    await store.loadStats()
    dataVersion.value++

    // 只搬走传进来的那几张。但「只看重复」这类条件会让别的图也跟着消失：
    // 移走 A 之后，和它一模一样的那张 B 就不再是重复图，于是从列表里掉出去——
    // 文件其实没动。这里必须说清楚，否则会被当成「把两张都搬走了」。
    const after = new Set(store.files.map((f) => f.id))
    const dropped = [...before].filter((id) => !after.has(id) && !moved.has(id)).length
    const tail = dropped
      ? `。另有 ${dropped} 张因为同伴被搬走、不再是重复图，从当前列表掉出去了（文件没动，去掉「有重复的」就能看到）`
      : ''
    showNotice(
      `已移入隔离区 ${r.moved} 张${r.failed ? `，失败 ${r.failed} 张` : ''}${tail}。随时可搬回`,
      dropped ? 11000 : 5000,
    )
  } catch (e) {
    showNotice('移入失败：' + String(e))
  }
}

function moveSelected() {
  void move([...store.selected])
}

/** 重复组页：把勾选的那些（要删的）一次性搬走 */
function moveCheckedGroups() {
  const ids = groups.checkedIds
  if (!ids.length) {
    showNotice('还没有勾选任何图片')
    return
  }
  void move(ids)
}

function afterScan() {
  // 扫描/指纹结束后：分布图、格式列表、库内统计都变了
  void store.loadHistograms()
  void store.loadFormats()
  void store.loadStats()
  void store.refreshNow()
  dataVersion.value++
}
</script>

<template>
  <TopBar :tab="tab" @update:tab="tab = $event" />

  <div class="view">
    <ScanProgress v-if="tab === 'scan'" @changed="afterScan" />
    <SettingsView v-else-if="tab === 'settings'" @saved="loadSettings" />
    <QuarantineView v-else-if="tab === 'quarantine'" @changed="afterScan" />
    <DupGroupView
      v-else-if="tab === 'groups'"
      :refresh-key="dataVersion"
      :gray-threshold="settings?.grayThreshold ?? 8"
    />
    <template v-else>
      <div class="body">
        <FilterPanel />
        <ThumbGrid
          :gray-threshold="settings?.grayThreshold ?? 8"
          @notice="showNotice"
          @quarantine="move"
        />
      </div>
    </template>
  </div>

  <!-- 底部一条常驻：中间显示库内信息，图库页带选中操作、重复组页带勾选搬家 -->
  <ActionBar
    :tab="tab"
    :roots="settings?.roots ?? []"
    @move-to-quarantine="moveSelected"
    @move-groups="moveCheckedGroups"
    @select-all="store.selectAllLoaded()"
  />

  <p v-if="notice" class="toast" role="status" aria-live="polite">{{ notice }}</p>
</template>

<style scoped>
.view {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.body {
  display: flex;
  flex: 1;
  min-height: 0;
}
</style>
