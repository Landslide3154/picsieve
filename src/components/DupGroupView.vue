<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  countDupGroups,
  fetchThumbUrl,
  listDupGroups,
  openExternal,
  rebuildGroups,
  setKeeper,
} from '../api'
import { fmtBytes, fmtCount } from '../format'
import type { FileRecord, GroupView } from '../types'

const emit = defineEmits<{ (e: 'move-to-quarantine', ids: number[]): void }>()

const groups = ref<GroupView[]>([])
const groupTotal = ref(0)
const index = ref(0)
const kind = ref<'exact' | 'similar'>('exact')
const focus = ref(-1)
const loading = ref(false)
const msg = ref('')
const error = ref('')
const thumbs = ref<Map<number, string>>(new Map())

const current = computed(() => groups.value[index.value])
const removable = computed(() => (current.value ? current.value.members.map((m) => m.id) : []))

function fmt(bytes: number): string {
  return fmtBytes(bytes)
}

/** 汉明距离换算成「多少像」，比「偏差 4」好懂 */
function similarity(distance: number | undefined): string {
  if (distance === undefined) return ''
  const pct = Math.max(0, Math.min(100, Math.round((1 - distance / 64) * 100)))
  return pct + '% 像'
}

async function loadThumbs(files: FileRecord[]) {
  const next = new Map(thumbs.value)
  for (const f of files) {
    if (next.has(f.id)) continue
    try {
      next.set(f.id, await fetchThumbUrl(f.id))
    } catch {
      /* 读不出的图就显示占位 */
    }
  }
  thumbs.value = next
}

function onGroupChange() {
  focus.value = -1
  const g = current.value
  if (g) void loadThumbs([g.keep, ...g.members])
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [list, total] = await Promise.all([
      listDupGroups(kind.value, 0, 500),
      countDupGroups(kind.value),
    ])
    groups.value = list
    groupTotal.value = total
    index.value = 0
    onGroupChange()
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
  }
}

async function rebuild() {
  loading.value = true
  error.value = ''
  msg.value = '正在重建分组…'
  try {
    const r = await rebuildGroups()
    msg.value = `一模一样 ${r.exact} 组 · 看着像 ${r.similar} 组`
    await load()
  } catch (e) {
    error.value = String(e)
    msg.value = ''
  } finally {
    loading.value = false
  }
}

function next() {
  if (index.value < groups.value.length - 1) {
    index.value++
    onGroupChange()
  }
}
function prev() {
  if (index.value > 0) {
    index.value--
    onGroupChange()
  }
}

async function keepAs(fileId: number) {
  const g = current.value
  if (!g || g.keep.id === fileId) return
  await setKeeper(g.groupId, fileId)
  const old = g.keep
  const picked = g.members.find((m) => m.id === fileId)
  if (picked) {
    g.keep = picked
    g.members = [old, ...g.members.filter((m) => m.id !== fileId)]
    if (g.distances.length) g.distances = [0, ...g.distances.filter((_, i) => g.members[i + 1]?.id !== old.id)]
  }
  focus.value = -1
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowRight') next()
  else if (e.key === 'ArrowLeft') prev()
  else if (e.key === 'ArrowDown') {
    const n = removable.value.length
    if (n) focus.value = Math.min(n - 1, focus.value + 1)
  } else if (e.key === 'ArrowUp') {
    focus.value = Math.max(-1, focus.value - 1)
  } else if (e.key === ' ') {
    e.preventDefault()
    if (focus.value >= 0) {
      const m = removable.value[focus.value]
      if (m !== undefined) void keepAs(m)
    }
  }
}

onMounted(() => {
  void load()
  window.addEventListener('keydown', onKey)
})
onUnmounted(() => window.removeEventListener('keydown', onKey))

// 换类型时重载
watch(kind, () => void load())
</script>

<template>
  <section class="wrap">
    <header class="head">
      <strong v-if="groups.length" class="num">
        第 {{ index + 1 }} 组 / 共 {{ fmtCount(groupTotal) }} 组
      </strong>
      <span v-else-if="!loading" class="dim">还没有重复组，先跑一次指纹再点「重建分组」</span>
      <span v-else class="dim">加载中…</span>

      <span v-if="current" class="save num">
        这一组能省 {{ fmt(current.savings) }}
      </span>

      <span class="grow" />
      <span v-if="msg" class="muted small">{{ msg }}</span>
      <select v-model="kind" aria-label="重复组类型">
        <option value="exact">一模一样</option>
        <option value="similar">看着像</option>
      </select>
      <button class="btn" :disabled="loading" @click="rebuild">重建分组</button>
    </header>

    <p v-if="error" class="error-text pad">{{ error }}</p>

    <div v-if="current" class="row">
      <article class="card keep" :class="{ focus: focus === -1 }">
        <img :src="thumbs.get(current.keep.id) ?? ''" alt="" @dblclick="openExternal(current.keep.path)" />
        <div class="meta">
          <strong class="num">{{ current.keep.width }} × {{ current.keep.height }}</strong>
          <span class="num">{{ fmt(current.keep.size) }} · {{ (current.keep.ext ?? '').toUpperCase() }}</span>
          <code class="truncate" :title="current.keep.path">{{ current.keep.path }}</code>
          <em>✓ 保留这张</em>
          <span v-if="current.keepReason" class="reason">建议理由：{{ current.keepReason }}</span>
        </div>
      </article>

      <article
        v-for="(m, i) in current.members"
        :key="m.id"
        class="card"
        :class="{ focus: focus === i }"
      >
        <img :src="thumbs.get(m.id) ?? ''" alt="" @dblclick="openExternal(m.path)" />
        <div class="meta">
          <strong class="num">{{ m.width }} × {{ m.height }}</strong>
          <span class="num">{{ fmt(m.size) }} · {{ (m.ext ?? '').toUpperCase() }}</span>
          <span v-if="current.distances[i] !== undefined" class="dist num">
            {{ similarity(current.distances[i]) }}
          </span>
          <code class="truncate" :title="m.path">{{ m.path }}</code>
          <button class="btn ghost sm" @click="keepAs(m.id)">改留这张</button>
        </div>
      </article>
    </div>

    <footer class="foot">
      <button class="btn" :disabled="index === 0" @click="prev">← 上一组</button>
      <button class="btn" :disabled="index >= groups.length - 1" @click="next">下一组 →</button>
      <span class="dim tiny">← → 换组 · ↑ ↓ 选成员 · 空格改留这张 · 双击图片用系统程序打开</span>
      <span class="grow" />
      <button
        class="btn danger"
        :disabled="!removable.length"
        @click="emit('move-to-quarantine', removable)"
      >
        保留这张，其余 {{ removable.length }} 张移入隔离区
      </button>
    </footer>
  </section>
</template>

<style scoped>
.wrap {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.head {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--line);
}
.grow {
  flex: 1;
}
.save {
  font-size: 12px;
  color: var(--ok);
}
.pad {
  padding: 8px 16px;
}
.row {
  display: flex;
  gap: 14px;
  padding: 16px;
  overflow-x: auto;
  flex: 1;
  align-items: flex-start;
}
.card {
  width: 268px;
  flex: none;
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
  overflow: hidden;
  background: var(--card);
  transition:
    box-shadow var(--speed) ease,
    transform var(--speed) ease;
}
.card.keep {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-bright);
}
.card.focus {
  box-shadow: 0 0 0 2px var(--warn);
}
.card img {
  width: 100%;
  height: 320px;
  object-fit: contain;
  background: rgba(0, 0, 0, 0.28);
  display: block;
  cursor: zoom-in;
}
.meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 9px 10px;
  font-size: 12px;
}
.meta code {
  color: var(--dim);
  font-size: 11px;
}
.meta em {
  color: var(--accent-bright);
  font-style: normal;
  font-weight: 600;
}
.reason {
  color: var(--dim);
  font-size: 11px;
}
.dist {
  color: var(--dim);
}
.foot {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 16px;
  border-top: 1px solid var(--line);
}
</style>
