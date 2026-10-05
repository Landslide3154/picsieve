<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { cancelScan, onScanProgress, saveSettings, startFingerprint, startScan, getSettings, pickFolder } from '../api'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ScanStats, Settings } from '../types'

const settings = ref<Settings | null>(null)
const running = ref(false)
const fpRunning = ref(false)
const seen = ref(0)
const totalHint = ref(0)
const current = ref('')
const stats = ref<ScanStats | null>(null)
const fpText = ref('')
const error = ref('')

const percent = computed(() =>
  totalHint.value > 0 ? Math.min(100, Math.round((seen.value / totalHint.value) * 100)) : 0,
)

let unlisten: UnlistenFn | null = null
let unlistenFp: UnlistenFn | null = null

onMounted(async () => {
  settings.value = await getSettings()
  unlisten = await onScanProgress((p) => {
    seen.value = p.seen
    totalHint.value = p.totalHint
    current.value = p.current
  })
  unlistenFp = await listen<{ phase: string; stats: Record<string, number> }>(
    'fingerprint://progress',
    (e) => {
      const s = e.payload.stats
      fpText.value =
        e.payload.phase === 'content'
          ? `内容指纹：已算 ${s.hashed ?? 0} 张，涉及 ${s.groups ?? 0} 组`
          : `视觉指纹：已完成 ${s.done ?? 0} 张，失败 ${s.failed ?? 0} 张`
    },
  )
})

onUnmounted(() => {
  unlisten?.()
  unlistenFp?.()
})

async function addRoot() {
  if (!settings.value) return
  const dir = await pickFolder()
  if (!dir || settings.value.roots.includes(dir)) return
  settings.value.roots = [...settings.value.roots, dir]
  await saveSettings(settings.value)
}

async function removeRoot(dir: string) {
  if (!settings.value) return
  settings.value.roots = settings.value.roots.filter((r) => r !== dir)
  await saveSettings(settings.value)
}

async function run() {
  error.value = ''
  stats.value = null
  seen.value = 0
  totalHint.value = 0
  running.value = true
  try {
    stats.value = await startScan()
  } catch (e) {
    error.value = String(e)
  } finally {
    running.value = false
  }
}

async function runFingerprint() {
  error.value = ''
  fpText.value = '正在计算…'
  fpRunning.value = true
  try {
    await startFingerprint()
    fpText.value = '指纹计算完成'
  } catch (e) {
    error.value = String(e)
    fpText.value = ''
  } finally {
    fpRunning.value = false
  }
}
</script>

<template>
  <section class="panel">
    <h2>扫描文件夹</h2>

    <ul class="roots">
      <li v-for="r in settings?.roots ?? []" :key="r">
        <span class="path">{{ r }}</span>
        <button :disabled="running" @click="removeRoot(r)">移除</button>
      </li>
      <li v-if="!settings?.roots?.length" class="empty">还没有添加文件夹</li>
    </ul>

    <div class="row">
      <button :disabled="running" @click="addRoot">添加文件夹…</button>
      <button class="primary" :disabled="running || !settings?.roots?.length" @click="run">
        开始扫描
      </button>
      <button v-if="running" @click="cancelScan">取消</button>
    </div>

    <div v-if="running || stats" class="progress">
      <div class="bar"><div class="fill" :style="{ width: percent + '%' }" /></div>
      <p>{{ seen }} / {{ totalHint || '?' }}（{{ percent }}%）</p>
      <p class="path">{{ current }}</p>
    </div>

    <p v-if="stats" class="summary">
      新增 {{ stats.inserted }} · 更新 {{ stats.updated }} · 跳过 {{ stats.skipped }} · 失败
      {{ stats.failed }}
    </p>

    <h2>指纹计算</h2>
    <p class="path">
      第一遍只读「大小重复」的文件算内容指纹；第二遍解码缩略图算视觉指纹与灰度。中途退出可以续算。
    </p>
    <div class="row">
      <button class="primary" :disabled="fpRunning || running" @click="runFingerprint">
        开始计算指纹
      </button>
    </div>
    <p v-if="fpText" class="summary">{{ fpText }}</p>

    <p v-if="error" class="error">{{ error }}</p>
  </section>
</template>

<style scoped>
.panel {
  padding: 24px;
  max-width: 860px;
}
.roots {
  list-style: none;
  padding: 0;
}
.roots li {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 6px 0;
  border-bottom: 1px solid var(--line);
}
.path {
  font-size: 12px;
  opacity: 0.75;
  word-break: break-all;
}
.empty {
  opacity: 0.5;
}
.row {
  display: flex;
  gap: 10px;
  margin: 16px 0;
}
button {
  padding: 6px 14px;
  border-radius: 4px;
  border: 1px solid var(--line);
  background: transparent;
  color: inherit;
  cursor: pointer;
}
button:disabled {
  opacity: 0.4;
  cursor: default;
}
.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}
.bar {
  height: 8px;
  background: var(--line);
  border-radius: 4px;
  overflow: hidden;
}
.fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.15s;
}
.summary {
  font-weight: 600;
}
.error {
  color: #e05c4b;
}
</style>
