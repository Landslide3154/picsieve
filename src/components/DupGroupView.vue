<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
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

const props = withDefaults(defineProps<{ refreshKey?: number }>(), { refreshKey: 0 })
const emit = defineEmits<{ (e: 'move-to-quarantine', ids: number[]): void }>()

/** 一次取多少组。密集平铺，滚到底自动再取一批 */
const BATCH = 40
/** 一组里最多画几张，多的折成「+N」 */
const MAX_CARDS = 6

interface Member {
  file: FileRecord
  /** 到基准图的汉明距离；换过保留项后是 null（不再假装知道） */
  distance: number | null
}

interface Tile {
  groupId: number
  keep: FileRecord
  members: Member[]
  savings: number
  reason: string
  /** 用户手动改过保留项 */
  manual: boolean
}

const tiles = ref<Tile[]>([])
const groupTotal = ref(0)
const kind = ref<'exact' | 'similar'>('exact')
const loading = ref(false)
const loadingMore = ref(false)
const msg = ref('')
const error = ref('')
const thumbs = ref<Map<number, string>>(new Map())
const scroller = ref<HTMLElement | null>(null)

const hasMore = computed(() => tiles.value.length < groupTotal.value)

function fmt(bytes: number): string {
  return fmtBytes(bytes)
}

/** 汉明距离换算成「多少像」，比「偏差 4」好懂 */
function similarity(distance: number | null): string {
  if (distance === null) return ''
  const pct = Math.max(0, Math.min(100, Math.round((1 - distance / 64) * 100)))
  return pct === 100 ? '一样' : pct + '% 像'
}

function visibleMembers(t: Tile): Member[] {
  return t.members.slice(0, MAX_CARDS)
}

function toTile(g: GroupView): Tile {
  return {
    groupId: g.groupId,
    keep: g.keep,
    members: g.members.map((f, i) => ({ file: f, distance: g.distances[i] ?? null })),
    savings: g.savings,
    reason: g.keepReason,
    manual: false,
  }
}

// ---------- 缩略图：每批一到就按顺序取（并发由 api 里的闸门控制，最多 6 张） ----------
const loadedThumbs = new Set<number>()
/** 离开这一页时还没排到的请求直接作废 */
let pageAlive = true

function loadThumbsFor(list: Tile[]) {
  const queue: FileRecord[] = []
  for (const t of list) {
    queue.push(t.keep, ...t.members.slice(0, MAX_CARDS).map((m) => m.file))
  }
  const todo = queue.filter((f) => !loadedThumbs.has(f.id))
  if (!todo.length) return
  for (const f of todo) loadedThumbs.add(f.id)
  void (async () => {
    // 每张到了就更新界面，不等整批
    await Promise.all(
      todo.map(async (f) => {
        try {
          const url = await fetchThumbUrl(f.id, () => !pageAlive)
          if (!url) return
          if (!pageAlive) {
            URL.revokeObjectURL(url)
            return
          }
          const next = new Map(thumbs.value)
          next.set(f.id, url)
          thumbs.value = next
        } catch {
          /* 读不出的图留个空位 */
        }
      }),
    )
  })()
}

// ---------- 加载 ----------
async function load(reset = true) {
  if (reset) {
    loading.value = true
    error.value = ''
  } else {
    if (loadingMore.value || !hasMore.value) return
    loadingMore.value = true
  }
  try {
    const offset = reset ? 0 : tiles.value.length
    const list = await listDupGroups(kind.value, offset, BATCH)
    const next = list.map(toTile)
    tiles.value = reset ? next : tiles.value.concat(next)
    loadThumbsFor(next)
    if (reset) {
      groupTotal.value = await countDupGroups(kind.value)
      await nextTick()
      if (scroller.value) scroller.value.scrollTop = 0
    }
  } catch (e) {
    error.value = String(e)
  } finally {
    loading.value = false
    loadingMore.value = false
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

/** 点某一张 = 就留它（同组其余的一起搬走时它会被留下） */
function keepAs(t: Tile, fileId: number) {
  if (t.keep.id === fileId) return
  const idx = t.members.findIndex((m) => m.file.id === fileId)
  if (idx < 0) return
  const oldKeep = t.keep
  const picked = t.members[idx].file
  const next = t.members.slice()
  // 被点的那张上位移成「保留」，原来的保留项退回成员
  next.splice(idx, 1, { file: oldKeep, distance: null })
  t.keep = picked
  t.members = next
  t.savings = next.reduce((s, m) => s + m.file.size, 0)
  t.manual = true
  void setKeeper(t.groupId, fileId).catch((e) => {
    error.value = String(e)
  })
}

function moveGroup(t: Tile) {
  const ids = t.members.map((m) => m.file.id)
  if (ids.length) emit('move-to-quarantine', ids)
}

// ---------- 滚动 / 键盘 ----------
function onScroll() {
  const el = scroller.value
  if (!el) return
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 600) void load(false)
}

let autoScroll = false
function interrupt() {
  autoScroll = false
}

function onKey(e: KeyboardEvent) {
  if (e.key !== 'End') interrupt()
  const el = scroller.value
  if (!el) return
  if (e.key === 'PageDown' || e.key === 'PageUp') {
    e.preventDefault()
    el.scrollTop += (e.key === 'PageDown' ? 1 : -1) * Math.max(120, el.clientHeight - 100)
  } else if (e.key === 'Home') {
    e.preventDefault()
    el.scrollTop = 0
  } else if (e.key === 'End') {
    e.preventDefault()
    void jumpToEnd()
  }
}

async function jumpToEnd() {
  const el = scroller.value
  if (!el) return
  autoScroll = true
  const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
  try {
    let idle = 0
    while (autoScroll && hasMore.value) {
      const before = tiles.value.length
      await load(false)
      if (!autoScroll) return
      await nextTick()
      el.scrollTop = el.scrollHeight
      if (tiles.value.length === before) {
        idle++
        if (idle >= 3) break
        await sleep(150)
      } else {
        idle = 0
      }
    }
    if (!autoScroll) return
    await nextTick()
    el.scrollTop = el.scrollHeight
  } finally {
    autoScroll = false
  }
}

onMounted(() => {
  void load()
})
onUnmounted(() => {
  pageAlive = false
})

watch(kind, () => void load())
watch(
  () => props.refreshKey,
  () => void load(),
)
</script>

<template>
  <section class="wrap">
    <header class="head">
      <strong class="num">共 {{ fmtCount(groupTotal) }} 组</strong>
      <span class="dim small num">已平铺 {{ fmtCount(tiles.length) }} 组</span>
      <span class="grow" />
      <span v-if="msg" class="muted small">{{ msg }}</span>
      <select v-model="kind" aria-label="重复组类型">
        <option value="exact">一模一样</option>
        <option value="similar">看着像</option>
      </select>
      <button class="btn" :disabled="loading" @click="rebuild">重建分组</button>
    </header>

    <p v-if="error" class="error-text pad">{{ error }}</p>
    <p class="tip pad dim small">
      点哪张就留哪张（默认按作品 ID → 路径更短 → 时间更早推荐），再按卡片上的按钮把其余的张搬进隔离区。
      PgUp/PgDn 翻页 · Home 顶部 · End 一路到底（中途滚滑轮或点鼠标即可打断）
    </p>

    <div
      ref="scroller"
      class="gscroll"
      tabindex="0"
      role="list"
      aria-label="重复组列表"
      @scroll.passive="onScroll"
      @keydown="onKey"
      @wheel.passive="interrupt"
      @mousedown="interrupt"
    >
      <p v-if="!tiles.length && !loading" class="empty">
        还没有重复组，先跑一次指纹再点「重建分组」
      </p>

      <div class="ggrid">
        <article v-for="(t, i) in tiles" :key="t.groupId" class="gcard" role="listitem">
          <header class="ghead">
            <span class="gno num">#{{ i + 1 }}</span>
            <span class="dim num">{{ t.keep.width }}×{{ t.keep.height }}</span>
            <span class="save num">省 {{ fmt(t.savings) }}</span>
          </header>

          <div class="thumbs">
            <div
              class="cell keep"
              :title="`保留：${t.keep.path}`"
              @click="keepAs(t, t.keep.id)"
            >
              <img
                :src="thumbs.get(t.keep.id) ?? ''"
                alt=""
                @dblclick="openExternal(t.keep.path)"
              />
              <span class="badge">✓</span>
            </div>
            <div
              v-for="m in visibleMembers(t)"
              :key="m.file.id"
              class="cell"
              :title="`改留这张：${m.file.path}`"
              @click="keepAs(t, m.file.id)"
            >
              <img :src="thumbs.get(m.file.id) ?? ''" alt="" @dblclick="openExternal(m.file.path)" />
              <span v-if="!t.manual && similarity(m.distance)" class="badge sim">
                {{ similarity(m.distance) }}
              </span>
            </div>
            <span v-if="t.members.length > MAX_CARDS" class="more dim tiny">
              +{{ t.members.length - MAX_CARDS }}
            </span>
          </div>

          <footer class="gfoot">
            <span class="reason dim tiny" :title="t.reason">
              {{ t.manual ? '已手动指定保留这张' : t.reason }}
            </span>
            <button
              class="btn danger sm"
              :disabled="!t.members.length"
              @click="moveGroup(t)"
            >
              其余 {{ t.members.length }} 张移入隔离区
            </button>
          </footer>
        </article>
      </div>

      <p v-if="loadingMore" class="foot">正在加载更多组…</p>
      <p v-else-if="!hasMore && tiles.length" class="foot">
        全部 {{ fmtCount(tiles.length) }} 组都在这儿了
      </p>
      <p v-else-if="loading" class="foot">加载中…</p>
    </div>
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
  padding: 0 16px;
}
.tip {
  margin: 8px 0 2px;
}
.error-text.pad {
  padding: 8px 16px;
}
.gscroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 10px 14px 24px;
  outline: none;
}
/* 密集平铺：一屏尽量多放几组（宽屏一般 4~5 列 × 5 行） */
.ggrid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(288px, 1fr));
  gap: 10px;
}
.gcard {
  border: 1px solid var(--line);
  border-radius: var(--radius);
  background: var(--panel);
  padding: 7px 8px 6px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.ghead {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 11px;
}
.gno {
  color: var(--dim);
  font-weight: 700;
}
.thumbs {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  align-items: center;
}
.cell {
  position: relative;
  width: 66px;
  height: 66px;
  border-radius: 6px;
  overflow: hidden;
  background: rgba(0, 0, 0, 0.3);
  cursor: pointer;
  flex: none;
  transition:
    transform var(--speed) ease,
    box-shadow var(--speed) ease;
}
.cell:hover {
  transform: scale(1.06);
  box-shadow: 0 6px 14px rgba(0, 0, 0, 0.5);
}
.cell img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
}
.cell.keep {
  box-shadow:
    inset 0 0 0 3px var(--sel),
    inset 0 0 0 4.5px rgba(255, 255, 255, 0.9);
}
.badge {
  position: absolute;
  left: 2px;
  top: 2px;
  width: 16px;
  height: 16px;
  border-radius: 50%;
  background: var(--sel);
  color: #2a1a00;
  font-size: 11px;
  font-weight: 800;
  display: grid;
  place-items: center;
}
.badge.sim {
  left: auto;
  right: 2px;
  width: auto;
  height: auto;
  padding: 0 5px;
  border-radius: 999px;
  background: rgba(0, 0, 0, 0.62);
  color: #e6edf6;
  font-weight: 600;
  font-size: 10px;
}
.more {
  align-self: center;
}
.gfoot {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: auto;
}
.reason {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.gfoot :deep(.btn) {
  padding: 3px 8px;
  font-size: 11px;
}
.empty,
.foot {
  text-align: center;
  color: var(--dim);
  font-size: 12px;
  margin: 18px 0 26px;
}
.empty {
  margin-top: 60px;
  font-size: 13px;
}
</style>
