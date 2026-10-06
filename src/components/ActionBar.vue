<script setup lang="ts">
import { computed } from 'vue'
import { fmtBytes, fmtCount, fmtRelative } from '../format'
import { useGroups } from '../stores/groups'
import { useLibrary } from '../stores/library'
import type { TabKey } from './TopBar.vue'

const props = defineProps<{ tab: TabKey; roots: string[] }>()
const store = useLibrary()
const groups = useGroups()
const emit = defineEmits<{
  (e: 'move-to-quarantine'): void
  (e: 'select-all'): void
  /** 重复组页：把勾选的（要删的）一次性搬走 */
  (e: 'move-groups'): void
}>()

const isLibrary = computed(() => props.tab === 'library')
const isGroups = computed(() => props.tab === 'groups')
const lastScanText = computed(() => fmtRelative(store.stats.lastScanAt, '扫描'))
const rootsTip = computed(() =>
  props.roots.length ? '扫描目录：\n' + props.roots.join('\n') : '还没有添加扫描目录',
)
</script>

<template>
  <!-- 底部一条：左边是选中/勾选情况，中间是库内信息，右边是提示与主操作。
       中间那栏永远居中，所以切页签、选中变化时它都不会左右跑。 -->
  <footer class="bar">
    <div class="side left">
      <template v-if="isLibrary">
        <span class="num">
          已选 <b>{{ fmtCount(store.selectedCount) }}</b> 张 ·
          <b>{{ fmtBytes(store.selectedBytes) }}</b>
        </span>
        <button class="btn ghost sm" @click="$emit('select-all')">全选已加载</button>
        <button
          class="btn ghost sm"
          :disabled="!store.selectedCount"
          @click="store.clearSelection()"
        >
          取消选择
        </button>
      </template>
      <template v-else-if="isGroups">
        <span class="num">
          已选 <b>{{ fmtCount(groups.checkedCount) }}</b> 张 ·
          <b>{{ fmtBytes(groups.checkedBytes) }}</b>
        </span>
        <button class="btn ghost sm" @click="groups.checkAll()">全选已加载</button>
        <button class="btn ghost sm" :disabled="!groups.checkedCount" @click="groups.clearChecked()">
          取消选择
        </button>
        <button class="btn ghost sm" @click="groups.applyDefaults()">按建议重选</button>
      </template>
    </div>

    <div class="mid num" :title="rootsTip">
      库内 {{ fmtCount(store.stats.total) }} 张 · {{ fmtBytes(store.stats.bytes) }} ·
      {{ lastScanText }}
    </div>

    <div class="side right">
      <template v-if="isLibrary">
        <span class="dim tiny hint">Ctrl+A 全选 · 空格预览 · PgUp/PgDn 翻页 · End 一路到底 · Delete 移入隔离区</span>
        <button
          class="btn danger"
          :disabled="!store.selectedCount"
          @click="$emit('move-to-quarantine')"
        >
          移到隔离区
        </button>
      </template>
      <template v-else-if="isGroups">
        <span class="dim tiny hint">点和图库一样：单击选中/取消 · 双击打开 · 默认已选好要删的那几张</span>
        <button
          class="btn danger"
          :disabled="!groups.checkedCount"
          @click="$emit('move-groups')"
        >
          移到隔离区
        </button>
      </template>
    </div>
  </footer>
</template>

<style scoped>
.bar {
  display: grid;
  grid-template-columns: 1fr auto 1fr;
  align-items: center;
  gap: 12px;
  padding: 8px 14px;
  border-top: 1px solid var(--line);
  background: var(--panel);
  flex: none;
}
.side {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.left {
  justify-content: flex-start;
}
.right {
  justify-content: flex-end;
}
.mid {
  font-size: 12px;
  color: var(--dim);
  white-space: nowrap;
  text-align: center;
}
.hint {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
</style>
