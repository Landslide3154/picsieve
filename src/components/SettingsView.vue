<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { getSettings, pickFolder, saveSettings, thumbCacheStats, trimThumbCache } from '../api'
import type { Settings } from '../types'

const s = ref<Settings | null>(null)
const msg = ref('')
const err = ref('')
const cacheBytes = ref(0)

function fmt(bytes: number): string {
  if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(2) + ' GB'
  return (bytes / 1048576).toFixed(1) + ' MB'
}

/** 数字框可能被清空或被填成非数字，先在这里拦成人话，别让后端抛原始错误。 */
function num(v: unknown): number | null {
  const n = typeof v === 'number' ? v : Number(v)
  return Number.isFinite(n) ? n : null
}

function validate(): { ok: true; fixed: Settings } | { ok: false; msg: string } {
  const cur = s.value
  if (!cur) return { ok: false, msg: '设置还没加载好' }

  const threads = num(cur.threads)
  if (threads === null || !Number.isInteger(threads) || threads < 1 || threads > 64) {
    return { ok: false, msg: '线程数要填 1 到 64 之间的整数' }
  }
  const similar = num(cur.similarThreshold)
  if (similar === null || similar < 0 || similar > 16) {
    return { ok: false, msg: '相似判定阈值要在 0 到 16 之间' }
  }
  const gray = num(cur.grayThreshold)
  if (gray === null || gray < 0 || gray > 100) {
    return { ok: false, msg: '灰阶判定宽松度要在 0 到 100 之间' }
  }
  const edge = num(cur.thumbMaxEdge)
  if (edge === null || !Number.isInteger(edge) || edge < 32 || edge > 2048) {
    return { ok: false, msg: '缩略图长边要填 32 到 2048 之间的整数' }
  }
  const cacheMb = num(cur.thumbCacheLimitMb)
  if (cacheMb === null || !Number.isInteger(cacheMb) || cacheMb < 16) {
    return { ok: false, msg: '缩略图缓存上限要填不小于 16 的整数（MB）' }
  }
  if (!cur.quarantineDir.trim()) {
    return { ok: false, msg: '隔离区目录不能为空' }
  }
  return {
    ok: true,
    fixed: {
      ...cur,
      threads,
      similarThreshold: similar,
      grayThreshold: gray,
      thumbMaxEdge: edge,
      thumbCacheLimitMb: cacheMb,
      quarantineDir: cur.quarantineDir.trim(),
    },
  }
}

onMounted(async () => {
  s.value = await getSettings()
  cacheBytes.value = await thumbCacheStats()
})

async function chooseQuarantine() {
  const dir = await pickFolder()
  if (dir && s.value) s.value.quarantineDir = dir
}

async function save() {
  if (!s.value) return
  err.value = ''
  msg.value = ''
  const checked = validate()
  if (!checked.ok) {
    err.value = checked.msg
    return
  }
  try {
    s.value = checked.fixed
    await saveSettings(checked.fixed)
    const t = await trimThumbCache()
    cacheBytes.value = t.remaining
    msg.value =
      t.removed > 0
        ? `已保存；顺手回收了 ${t.removed} 个旧缩略图，释放 ${fmt(t.freed)}`
        : '已保存'
  } catch (e) {
    err.value = String(e)
  }
}
</script>

<template>
  <section v-if="s" class="wrap">
    <h2>设置</h2>

    <div class="row">
      <span class="k">线程数</span>
      <input type="number" min="1" max="64" v-model.number="s.threads" />
      <em>默认「逻辑核心数 − 2」，给你的电脑留出响应余量</em>
    </div>

    <div class="row">
      <span class="k">相似判定阈值</span>
      <input type="range" min="0" max="16" v-model.number="s.similarThreshold" />
      <b>{{ s.similarThreshold }}</b>
      <em>越小越严：越不容易误判，也越可能漏掉；越大越宽，误判会变多</em>
    </div>

    <div class="row">
      <span class="k">灰阶判定宽松度</span>
      <input type="range" min="0" max="30" v-model.number="s.grayThreshold" />
      <b>{{ s.grayThreshold }}</b>
      <em>越小只认纯黑白灰；越大越容易把偏灰的彩图也当成灰阶</em>
    </div>

    <div class="row">
      <span class="k">缩略图长边</span>
      <input type="number" min="80" max="640" v-model.number="s.thumbMaxEdge" />
      <em>像素。越大越清楚，缓存也越占地方</em>
    </div>

    <div class="row">
      <span class="k">缩略图缓存上限</span>
      <input type="number" min="256" max="20480" v-model.number="s.thumbCacheLimitMb" />
      <em>
        MB。当前占用 {{ fmt(cacheBytes) }}；超过上限时会在启动和保存设置时自动删掉最旧的缩略图
      </em>
    </div>

    <div class="row">
      <span class="k">隔离区目录</span>
      <input class="wide" v-model="s.quarantineDir" />
      <button @click="chooseQuarantine">选择…</button>
      <em>必须放在扫描目录之外，否则下次扫描会把待删文件又捞回来</em>
    </div>

    <div class="row">
      <span class="k">扫描目录</span>
      <div class="roots">
        <div v-for="r in s.roots" :key="r" class="root">
          <code>{{ r }}</code>
          <button @click="s.roots = s.roots.filter((x) => x !== r)">移除</button>
        </div>
        <p v-if="!s.roots.length" class="empty">还没有添加扫描目录，请去「扫描」页添加</p>
      </div>
    </div>

    <div class="actions">
      <button class="primary" @click="save">保存</button>
      <span v-if="msg" class="ok">{{ msg }}</span>
      <span v-if="err" class="error">{{ err }}</span>
    </div>
  </section>
</template>

<style scoped>
.wrap {
  padding: 20px 24px;
  max-width: 900px;
  overflow-y: auto;
}
h2 {
  font-size: 16px;
  margin: 0 0 8px;
}
.row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 0;
  border-bottom: 1px solid var(--line);
  flex-wrap: wrap;
}
.k {
  width: 130px;
  flex: none;
}
.row input[type='range'] {
  width: 240px;
}
.row input[type='number'] {
  width: 90px;
}
.wide {
  flex: 1;
  min-width: 260px;
}
em {
  opacity: 0.6;
  font-size: 12px;
  flex-basis: 100%;
}
input {
  background: rgba(128, 128, 128, 0.12);
  border: 1px solid var(--line);
  color: inherit;
  border-radius: 4px;
  padding: 4px 8px;
}
button {
  border: 1px solid var(--line);
  background: transparent;
  color: inherit;
  border-radius: 4px;
  padding: 4px 12px;
  cursor: pointer;
}
.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.actions {
  display: flex;
  align-items: center;
  gap: 14px;
  margin-top: 18px;
}
.ok {
  color: var(--accent);
}
.error {
  color: #e05c4b;
}
.roots {
  flex: 1;
  min-width: 300px;
}
.root {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 0;
}
.root code {
  flex: 1;
  font-size: 12px;
  word-break: break-all;
}
.empty {
  opacity: 0.6;
  font-size: 12px;
  margin: 0;
}
</style>
