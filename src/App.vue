<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { getSettings, moveToQuarantine, restoreBatch, showMainWindow } from './api'
import { fmtBytes } from './format'
import { loadPref, savePref } from './prefs'
import ActionBar from './components/ActionBar.vue'
import ConfirmDialog from './components/ConfirmDialog.vue'
import DupGroupView from './components/DupGroupView.vue'
import FilterPanel from './components/FilterPanel.vue'
import QuarantineView from './components/QuarantineView.vue'
import ScanProgress from './components/ScanProgress.vue'
import SettingsView from './components/SettingsView.vue'
import ThumbGrid from './components/ThumbGrid.vue'
import TopBar, { type TabKey } from './components/TopBar.vue'
import { useGroups } from './stores/groups'
import { PAGE, useLibrary } from './stores/library'
import type { Settings } from './types'

const TAB_KEYS: TabKey[] = ['library', 'groups', 'quarantine', 'scan', 'settings']

function initialTab(): TabKey {
  const saved = loadPref<string>('tab', 'library')
  return TAB_KEYS.includes(saved as TabKey) ? (saved as TabKey) : 'library'
}

const tab = ref<TabKey>(initialTab())
const settings = ref<Settings | null>(null)

interface ToastAction {
  label: string
  run: () => void
}
const toast = ref<{ text: string; action?: ToastAction } | null>(null)
let toastTimer: number | undefined

function showNotice(text: string, ms = 5000, action?: ToastAction) {
  toast.value = { text, action }
  if (toastTimer !== undefined) clearTimeout(toastTimer)
  toastTimer = window.setTimeout(() => (toast.value = null), ms)
}

/** 最近一次移入隔离区的批次：Ctrl+Z 或提示条上的「撤销」都用它 */
const lastUndo = ref<{ batchId: string; count: number } | null>(null)

const confirmBox = ref<null | {
  title: string
  body: string
  confirmText: string
  danger?: boolean
  run: () => void
}>(null)

/** 搬过文件/重新扫描后自增，让重复组页知道该重新加载 */
const dataVersion = ref(0)
const store = useLibrary()
const groups = useGroups()

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
  // 恢复的筛选可能把结果筛成空，必须给一句解释和一个一键清空的出口
  if (store.filterRestored) {
    showNotice('已恢复上次的筛选条件；看不到图就点右边清空', 12000, {
      label: '清空筛选',
      run: () => {
        void store.resetAll()
        showNotice('筛选已清空')
      },
    })
  }
})

// 切回图库时补一次统计（扫描/隔离之后数据变了）
watch(tab, (t) => {
  savePref('tab', t)
  if (t === 'library') {
    void store.loadStats()
    if (!store.files.length) void store.refreshNow()
  }
})

/** 搬完刷新时保留多少条：不把用户翻了半天的列表打回第一页（上限 3000，避免重查十万行） */
function keepLoaded(extra = 0) {
  return Math.min(3000, Math.max(PAGE, store.files.length + extra))
}

async function undoMove() {
  const u = lastUndo.value
  if (!u) return
  lastUndo.value = null
  try {
    const r = await restoreBatch(u.batchId)
    await store.refreshNow(keepLoaded(r.moved))
    await store.loadStats()
    dataVersion.value++
    showNotice(
      `已把 ${r.moved} 张搬回原位${r.failed ? `，另有 ${r.failed} 张没搬回（原位置可能已被别的文件占用）` : ''}`,
      6000,
    )
  } catch (e) {
    showNotice('撤销失败：' + String(e), 8000)
  }
}

async function move(ids: number[]) {
  if (!ids.length) return
  const totalBefore = store.total
  try {
    const r = await moveToQuarantine(ids)
    store.clearSelection()
    lastUndo.value = r.moved > 0 && r.batchId ? { batchId: r.batchId, count: r.moved } : null
    await store.refreshNow(keepLoaded())
    await store.loadStats()
    dataVersion.value++

    // 只搬走传进来的那几张。但「只看重复」这类条件会让别的图也跟着消失：
    // 移走 A 之后，和它一模一样的那张 B 就不再是重复图，于是从列表里掉出去——
    // 文件其实没动。这里必须说清楚，否则会被当成「把两张都搬走了」。
    // 用「总数减少量 - 实际搬走量」来算，比逐条比对列表更准（列表只重新取了前几千条）。
    // 只在图库页说这句话：在重复组页说「当前列表」会指错对象。
    const dropped =
      tab.value === 'library' ? Math.max(0, totalBefore - store.total - r.moved) : 0
    const tail = dropped
      ? `。另有 ${dropped} 张因为同伴被搬走、不再是重复图，从当前列表掉出去了（文件没动，去掉「有重复的」就能看到）`
      : ''
    showNotice(
      `已移入隔离区 ${r.moved} 张${r.failed ? `，失败 ${r.failed} 张` : ''}${tail}`,
      lastUndo.value ? 30000 : 6000,
      lastUndo.value ? { label: '撤销', run: () => void undoMove() } : undefined,
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
  // 「看着像」是判断题：默认不替用户勾，真要点走时再问一句
  if (groups.kind === 'similar') {
    confirmBox.value = {
      title: '这些是「看着像」的图，不是一模一样',
      body: `当前勾选了 ${ids.length} 张，合计 ${fmtBytes(groups.checkedBytes)}。移入隔离区不会真的删除，之后还能整批搬回；但请确认要移走的确实是这些。`,
      confirmText: `移走这 ${ids.length} 张`,
      danger: true,
      run: () => {
        showNotice(`正在移走 ${ids.length} 张…`, 3000)
        void move(ids)
      },
    }
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

function confirmYes() {
  const box = confirmBox.value
  confirmBox.value = null
  box?.run()
}

function runToastAction() {
  toast.value?.action?.run()
}

// Ctrl+Z 撤销最近一次搬运：预览浮层开着、焦点在网格上都好使
function onGlobalKey(e: KeyboardEvent) {
  const t = e.target as HTMLElement | null
  if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && lastUndo.value) {
    e.preventDefault()
    void undoMove()
  }
}
onMounted(() => window.addEventListener('keydown', onGlobalKey))
onUnmounted(() => window.removeEventListener('keydown', onGlobalKey))
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

  <p v-if="toast" class="toast" role="status" aria-live="polite">
    <span>{{ toast.text }}</span>
    <button
      v-if="toast.action"
      class="btn link sm"
      :title="toast.action.label === '撤销' ? '也可以按 Ctrl+Z' : ''"
      @click="runToastAction"
    >
      {{ toast.action.label }}
    </button>
  </p>

  <ConfirmDialog
    v-if="confirmBox"
    :title="confirmBox.title"
    :body="confirmBox.body"
    :confirm-text="confirmBox.confirmText"
    :danger="confirmBox.danger"
    @confirm="confirmYes"
    @cancel="confirmBox = null"
  />
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
