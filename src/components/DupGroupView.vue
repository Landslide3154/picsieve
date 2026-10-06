<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { openExternal, rebuildGroups } from '../api'
import { fmtBytes, fmtCount } from '../format'
import { useGroups, type GroupTile } from '../stores/groups'
import ThumbCell from './ThumbCell.vue'
import type { FileRecord } from '../types'

const props = withDefaults(defineProps<{ refreshKey?: number; grayThreshold?: number }>(), {
  refreshKey: 0,
  grayThreshold: 8,
})
const groups = useGroups()

const msg = ref('')
const scroller = ref<HTMLElement | null>(null)
/** 一组里最多画几张，多的折成「+N」 */
const MAX_CARDS = 8

/** 一组里要画出来的图：建议保留的排在最前，方便对照 */
function cardsOf(t: GroupTile): FileRecord[] {
  return [t.keep, ...t.members.map((m) => m.file)].slice(0, MAX_CARDS)
}

async function load(reset = true) {
  await groups.load(reset)
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
    groups.clearChecked()
    await load(true)
  } catch (e) {
    groups.error = String(e)
    msg.value = ''
  }
}

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
    while (autoScroll && groups.hasMore) {
      const before = groups.tiles.length
      await load(false)
      if (!autoScroll) return
      await nextTick()
      el.scrollTop = el.scrollHeight
      if (groups.tiles.length === before) {
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

onMounted(() => void load(true))
onUnmounted(() => {
  autoScroll = false
})
watch(
  () => props.refreshKey,
  () => {
    groups.clearChecked()
    void load(true)
  },
)
</script>

<template>
  <section class="wrap">
    <header class="head">
      <strong class="num">共 {{ fmtCount(groups.total) }} 组</strong>
      <span class="dim small num">已平铺 {{ fmtCount(groups.tiles.length) }} 组</span>
      <span class="grow" />
      <span v-if="msg" class="muted small">{{ msg }}</span>
      <select
        :value="groups.kind"
        aria-label="重复组类型"
        @change="groups.setKind(($event.target as HTMLSelectElement).value as 'exact' | 'similar')"
      >
        <option value="exact">一模一样</option>
        <option value="similar">看着像</option>
      </select>
      <button class="btn" :disabled="groups.loading" @click="rebuild">重建分组</button>
    </header>

    <p class="tip pad dim small">
      操作跟图库一样：<b>单击选中</b>（要删的那几张默认已经选好）、<b>双击用系统看图程序打开</b>，
      选好后按最下面的「移到隔离区」一次搬走。想改就用点选增减。
      <span class="dim">PgUp/PgDn 翻页 · Home 顶部 · End 一路到底</span>
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
      <p v-if="!groups.tiles.length && !groups.loading" class="empty">
        还没有重复组，先跑一次指纹再点「重建分组」
      </p>

      <div class="ggrid">
        <article v-for="(t, i) in groups.tiles" :key="t.groupId" class="gcard" role="listitem">
          <header class="ghead">
            <span class="gno num">#{{ i + 1 }}</span>
            <span class="dim num">{{ t.keep.width }}×{{ t.keep.height }}</span>
            <span class="dim">·</span>
            <span class="dim tiny">{{ t.reason }}</span>
            <span class="grow" />
            <span class="dim tiny num">能省 {{ fmtBytes(t.members.reduce((s, m) => s + m.file.size, 0)) }}</span>
          </header>

          <div class="thumbs">
            <div v-for="f in cardsOf(t)" :key="f.id" class="slot">
              <ThumbCell
                :file="f"
                :selected="groups.isChecked(f.id)"
                :focused="false"
                :dup-count="cardsOf(t).length"
                :gray-threshold="props.grayThreshold"
                @select="groups.toggle(f.id)"
                @open="openExternal(f.path)"
              />
            </div>
            <span v-if="t.members.length + 1 > MAX_CARDS" class="more dim tiny">
              +{{ t.members.length + 1 - MAX_CARDS }} 张
            </span>
          </div>
        </article>
      </div>

      <p v-if="groups.loadingMore" class="foot">正在加载更多组…</p>
      <p v-else-if="!groups.hasMore && groups.tiles.length" class="foot">
        全部 {{ fmtCount(groups.tiles.length) }} 组都在这儿了
      </p>
      <p v-else-if="groups.loading" class="foot">加载中…</p>
    </div>

    <p v-if="groups.error" class="error-text pad">{{ groups.error }}</p>
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
/* 平铺：宽屏 3 列，缩略图 152px 宽（跟图库的格子同一套样式） */
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
  gap: 10px;
  align-items: flex-start;
}
.slot {
  width: 152px;
  flex: none;
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
