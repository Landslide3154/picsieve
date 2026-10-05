<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { fetchThumbUrl, listDupGroups, rebuildGroups, setKeeper } from '../api'
import type { FileRecord, GroupView } from '../types'

const emit = defineEmits<{ (e: 'move-to-quarantine', ids: number[]): void }>()

const groups = ref<GroupView[]>([])
const index = ref(0)
const kind = ref<'exact' | 'similar'>('exact')
const focus = ref(-1) // -1 = 保留项，0..n-1 = 第 n 个待删成员
const loading = ref(false)
const msg = ref('')
const error = ref('')
const thumbs = ref<Map<number, string>>(new Map())

const current = computed(() => groups.value[index.value])

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
    groups.value = await listDupGroups(kind.value, 0, 300)
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

/** 当前组里除保留项以外的成员 id —— 也就是「这 N 张」 */
const removable = computed(() => (current.value ? current.value.members.map((m) => m.id) : []))

async function keepAs(fileId: number) {
  const g = current.value
  if (!g || g.keep.id === fileId) return
  await setKeeper(g.groupId, fileId)
  const old = g.keep
  const picked = g.members.find((m) => m.id === fileId)
  if (picked) {
    g.keep = picked
    g.members = [old, ...g.members.filter((m) => m.id !== fileId)]
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
</script>

<template>
  <section class="wrap">
    <header class="head">
      <strong v-if="groups.length">第 {{ index + 1 }} 组 / 共 {{ groups.length }} 组</strong>
      <span v-else-if="!loading">还没有重复组，先跑一次指纹再点「重建分组」</span>
      <span v-else>加载中…</span>
      <span class="flex" />
      <span v-if="msg" class="msg">{{ msg }}</span>
      <select v-model="kind" @change="load">
        <option value="exact">一模一样</option>
        <option value="similar">看着像</option>
      </select>
      <button :disabled="loading" @click="rebuild">重建分组</button>
    </header>

    <p v-if="error" class="error">{{ error }}</p>

    <div v-if="current" class="row">
      <article class="card keep" :class="{ focus: focus === -1 }">
        <img :src="thumbs.get(current.keep.id) ?? ''" alt="" />
        <div class="meta">
          <strong>{{ current.keep.width }} × {{ current.keep.height }}</strong>
          <span>{{ (current.keep.size / 1048576).toFixed(2) }} MB</span>
          <code>{{ current.keep.path }}</code>
          <em>✓ 保留这张</em>
        </div>
      </article>

      <article
        v-for="(m, i) in current.members"
        :key="m.id"
        class="card"
        :class="{ focus: focus === i }"
      >
        <img :src="thumbs.get(m.id) ?? ''" alt="" />
        <div class="meta">
          <strong>{{ m.width }} × {{ m.height }}</strong>
          <span>{{ (m.size / 1048576).toFixed(2) }} MB</span>
          <span v-if="current.distances[i] !== undefined" class="dist">
            相似度偏差 {{ current.distances[i] }}
          </span>
          <code>{{ m.path }}</code>
          <button @click="keepAs(m.id)">改留这张</button>
        </div>
      </article>
    </div>

    <footer class="foot">
      <button :disabled="index === 0" @click="prev">← 上一组</button>
      <button :disabled="index >= groups.length - 1" @click="next">下一组 →</button>
      <span class="hint">← → 换组 · ↑ ↓ 选成员 · 空格改留这张</span>
      <span class="flex" />
      <button
        class="danger"
        :disabled="!removable.length"
        @click="emit('move-to-quarantine', removable)"
      >
        这 {{ removable.length }} 张移入隔离区
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
.flex {
  flex: 1;
}
.msg {
  font-size: 12px;
  color: var(--accent);
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
  width: 260px;
  flex: none;
  border: 1px solid var(--line);
  border-radius: 6px;
  overflow: hidden;
  background: rgba(128, 128, 128, 0.08);
}
.card.keep {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px var(--accent);
}
.card.focus {
  outline: 2px dashed var(--accent);
  outline-offset: 2px;
}
.card img {
  width: 100%;
  height: 300px;
  object-fit: contain;
  background: rgba(0, 0, 0, 0.25);
  display: block;
}
.meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px;
  font-size: 12px;
}
.meta code {
  word-break: break-all;
  opacity: 0.7;
  font-size: 11px;
}
.meta em {
  color: var(--accent);
  font-style: normal;
  font-weight: 600;
}
.dist {
  opacity: 0.6;
}
.foot {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  border-top: 1px solid var(--line);
}
button {
  border: 1px solid var(--line);
  background: transparent;
  color: inherit;
  border-radius: 4px;
  padding: 4px 12px;
  cursor: pointer;
}
button:disabled {
  opacity: 0.4;
  cursor: default;
}
.danger {
  background: var(--danger);
  border-color: var(--danger);
  color: #fff;
}
.hint {
  font-size: 11px;
  opacity: 0.55;
}
.error {
  color: #e05c4b;
  padding: 0 16px;
}
</style>
