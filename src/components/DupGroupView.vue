<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { fetchThumbUrl, openExternal, rebuildGroups } from '../api'
import { fmtBytes, fmtCount } from '../format'
import { useGroups, type GroupTile } from '../stores/groups'
import type { FileRecord } from '../types'

const props = withDefaults(defineProps<{ refreshKey?: number }>(), { refreshKey: 0 })
const store = useGroups()

const msg = ref('')
/** 一组里最多画几张，多的折成「+N」 */
const MAX_CARDS = 8

const scroller = ref<HTMLElement | null>(null)
const thumbs = ref<Map<number, string>>(new Map())

/** 一屏能看到多少组（给顶部提示用） */
const shown = computed(() => store.tiles.length)

function fmt(bytes: number): string {
  return fmtBytes(bytes)
}

function similarity(distance: number | null): string {
  if (distance === null) return ''
  const pct = Math.max(0, Math.min(100, Math.round((1 - distance / 64) * 100)))
  return pct === 100 ? '一样' : pct + '% 像'
}

/** 一组里要画出来的图（建议保留项放最前，便于对照） */
function cardsOf(t: GroupTile): { file: FileRecord; distance: number | null }[] {
  return [
    { file: t.keep, distance: 0 },
    ...t.members.map((m) => ({ file: m.file, distance: m.distance })),
  ].slice(0, MAX_CARDS)
}

// ---------- 缩略图：每批一到就取（并发由 api 闸门控制） ----------
const loadedThumbs = new Set<number>()
let pageAlive = true

function loadThumbsFor(list: GroupTile[]) {
  const queue: FileRecord[] = []
  for (const t of list) for (const c of cardsOf(t)) queue.push(c.file)
  const todo = queue.filter((f) => !loadedThumbs.has(f.id))
  if (!todo.length) return
  for (const f of todo) loadedThumbs.add(f.id)
  void (async () => {
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
          /* 读不出的图留空位 */
        }
      }),
    )
  })()
}

async function load(reset = true) {
  const before = store.tiles.length
  await store.load(reset)
  const added = store.tiles.slice(before)
  if (added.length) loadThumbsFor(added)
  if (reset && scroller.value) {
    await nextTick()
    scroller.value.scrollTop = 0
  }
}

async function rebuild() {
  msg.value = '正在重建分组…'
  try {
    const r = await rebuildGroups()
    msg.value = `一模一样 ${r.exact} 组 · 看着像 ${r.similar} 组`
    store.clearChecked()
    await load(true)
  } catch (e) {
    store.error = String(e)
    msg.value = ''
  }
}

// ---------- 滚动 / 键盘 ----------
function onScroll() {
  const el = scroller.value
  if (!el) return
  if (el.scrollTop + el.clientHeight >= el.scrollHeight - 800) void load(false)
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
    while (autoScroll && store.hasMore) {
      const before = store.tiles.length
      await load(false)
      if (!autoScroll) return
      await nextTick()
      el.scrollTop = el.scrollHeight
      if (store.tiles.length === before) {
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
  void load(true)
})
onUnmounted(() => {
  pageAlive = false
})

watch(
  () => props.refreshKey,
  () => {
    store.clearChecked()
    void load(true)
  },
)
</script>

<template>
  <section class="wrap">
    <header class="head">
      <strong class="num">共 {{ fmtCount(store.total) }} 组</strong>
      <span class="dim small num">已平铺 {{ fmtCount(shown) }} 组</span>
      <span class="grow" />
      <span v-if="msg" class="muted small">{{ msg }}</span>
      <select
        :value="store.kind"
        aria-label="重复组类型"
        @change="store.setKind(($event.target as HTMLSelectElement).value as 'exact' | 'similar')"
      >
        <option value="exact">一模一样</option>
        <option value="similar">看着像</option>
      </select>
      <button class="btn" :disabled="store.loading" @click="rebuild">重建分组</button>
    </header>

    <p class="tip pad">
      <b>点图上的勾</b>：打勾＝要删掉，取消勾＝留下。
      两张相同的默认勾 1 张、三张的默认勾 2 张（也就是默认每组留下建议的那张）。
      勾好以后按<b>最下面那个按钮</b>一次把勾上的全搬进隔离区。
      <span class="dim">
        PgUp/PgDn 翻页 · Home 顶部 · End 一路到底（滚滑轮或点鼠标即打断）
      </span>
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
      <p v-if="!store.tiles.length && !store.loading" class="empty">
        还没有重复组，先跑一次指纹再点「重建分组」
      </p>

      <div class="ggrid">
        <article v-for="(t, i) in store.tiles" :key="t.groupId" class="gcard" role="listitem">
          <header class="ghead">
            <span class="gno num">#{{ i + 1 }}</span>
            <span class="dim num">{{ t.keep.width }}×{{ t.keep.height }}</span>
            <span class="dim">·</span>
            <span class="dim tiny">{{ t.reason }}</span>
          </header>

          <div class="thumbs">
            <button
              v-for="c in cardsOf(t)"
              :key="c.file.id"
              class="pic"
              :class="{ del: store.isChecked(c.file.id) }"
              :title="`${c.file.path}\n点一下：打勾=删掉 / 取消=留下`"
              @click="store.toggle(c.file.id)"
            >
              <img :src="thumbs.get(c.file.id) ?? ''" alt="" @dblclick="openExternal(c.file.path)" />
              <span v-if="!thumbs.get(c.file.id)" class="ph dim tiny">生成中…</span>
              <span class="mark">
                <span v-if="store.isChecked(c.file.id)" class="del-tag">删</span>
                <span v-else class="keep-tag">留</span>
              </span>
              <span v-if="similarity(c.distance)" class="sim num">{{ similarity(c.distance) }}</span>
              <span class="sz num">{{ fmt(c.file.size) }}</span>
            </button>
            <span v-if="t.members.length + 1 > MAX_CARDS" class="more dim tiny">
              +{{ t.members.length + 1 - MAX_CARDS }}
            </span>
          </div>
        </article>
      </div>

      <p v-if="store.loadingMore" class="foot">正在加载更多组…</p>
      <p v-else-if="!store.hasMore && store.tiles.length" class="foot">
        全部 {{ fmtCount(store.tiles.length) }} 组都在这儿了
      </p>
      <p v-else-if="store.loading" class="foot">加载中…</p>
    </div>

    <p v-if="store.error" class="error-text pad">{{ store.error }}</p>
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
.pad {
  padding: 0 16px;
}
.tip {
  margin: 8px 0 2px;
  font-size: 12px;
  color: var(--fg);
  line-height: 1.6;
}
.tip b {
  color: var(--sel);
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
/* 平铺但要看得清：卡片大一点，宽屏 3 列 */
.ggrid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(560px, 1fr));
  gap: 12px;
}
.gcard {
  border: 1px solid var(--line);
  border-radius: var(--radius-lg);
  background: var(--panel);
  padding: 8px 10px 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.ghead {
  display: flex;
  align-items: baseline;
  gap: 8px;
  font-size: 12px;
}
.gno {
  color: var(--dim);
  font-weight: 700;
}
.thumbs {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: flex-start;
}
.pic {
  position: relative;
  width: 168px;
  height: 200px;
  padding: 0;
  border: none;
  border-radius: 8px;
  overflow: hidden;
  background: rgba(0, 0, 0, 0.3);
  cursor: pointer;
  flex: none;
  transition:
    transform var(--speed) ease,
    box-shadow var(--speed) ease;
}
.pic:hover {
  transform: translateY(-2px);
  box-shadow: 0 8px 18px rgba(0, 0, 0, 0.5);
}
.pic img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: filter var(--speed) ease;
}
/* 打勾 = 要删：红框 + 提亮，一眼看清哪些会被删 */
.pic.del {
  box-shadow:
    inset 0 0 0 3px rgba(255, 92, 92, 0.95),
    inset 0 0 0 5px rgba(255, 255, 255, 0.85);
}
.pic.del img {
  filter: brightness(1.18) saturate(1.05);
}
.pic:not(.del) {
  box-shadow: inset 0 0 0 3px rgba(255, 164, 36, 0.9);
}
.mark {
  position: absolute;
  left: 6px;
  top: 6px;
}
.del-tag,
.keep-tag {
  display: inline-block;
  padding: 0 7px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
}
.del-tag {
  background: #ff5c5c;
  color: #2a0000;
}
.keep-tag {
  background: var(--sel);
  color: #2a1a00;
}
.sim {
  position: absolute;
  right: 6px;
  top: 6px;
  padding: 0 6px;
  border-radius: 999px;
  background: rgba(0, 0, 0, 0.62);
  color: #e6edf6;
  font-size: 10px;
}
.sz {
  position: absolute;
  left: 6px;
  bottom: 6px;
  padding: 0 6px;
  border-radius: 4px;
  background: rgba(0, 0, 0, 0.55);
  color: #dfe7f2;
  font-size: 10px;
}
/* 大图生成缩略图要几秒，这里明确写着「生成中…」，别让人以为是坏了 */
.ph {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
}
.more {
  align-self: center;
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
