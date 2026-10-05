<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import {
  cancelJob,
  getSettings,
  onScanProgress,
  pickFolder,
  saveSettings,
  startFingerprint,
  startScan,
} from '../api'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { ScanProgress, ScanStats, Settings } from '../types'

const emit = defineEmits<{ (e: 'changed'): void }>()

const settings = ref<Settings | null>(null)
const scanning = ref(false)
const fingerprinting = ref(false)
const progress = ref<ScanProgress | null>(null)
const stats = ref<ScanStats | null>(null)
const scanSeconds = ref(0)
const fpPhase = ref<'content' | 'visual' | ''>('')
const fpText = ref('')
const fpDone = ref(0)
const fpTotal = ref(0)
const fpCancelled = ref(false)
const error = ref('')

let unlistenScan: UnlistenFn | null = null
let unlistenFp: UnlistenFn | null = null

const percent = computed(() => {
  const p = progress.value
  if (!p || !p.totalHint) return 0
  return Math.min(100, Math.round((p.seen / p.totalHint) * 100))
})

const eta = computed(() => {
  const p = progress.value
  if (!p || !p.perSecond || !p.totalHint) return ''
  const left = Math.max(0, p.totalHint - p.seen) / p.perSecond
  if (left < 1) return '马上好'
  if (left < 60) return `还要 ${Math.round(left)} 秒`
  return `还要约 ${Math.round(left / 60)} 分钟`
})

onMounted(async () => {
  settings.value = await getSettings()
  unlistenScan = await onScanProgress((p) => {
    progress.value = p
  })
  unlistenFp = await listen<{
    phase: 'content' | 'visual'
    stats: Record<string, number | boolean>
  }>('fingerprint://progress', (e) => {
    const s = e.payload.stats
    fpPhase.value = e.payload.phase
    fpCancelled.value = s.cancelled === true
    if (e.payload.phase === 'content') {
      fpText.value = `第一遍：内容指纹已算 ${s.hashed ?? 0} 张（大小重复的才需要读内容）`
    } else {
      fpDone.value = Number(s.done ?? 0)
      fpTotal.value = Number(s.total ?? 0)
      fpText.value = `第二遍：视觉指纹已完成 ${fpDone.value.toLocaleString('zh-CN')} / ${fpTotal.value.toLocaleString('zh-CN')} 张，失败 ${s.failed ?? 0} 张`
    }
  })
})

onUnmounted(() => {
  unlistenScan?.()
  unlistenFp?.()
})

async function persist() {
  if (!settings.value) return
  error.value = ''
  try {
    await saveSettings(settings.value)
  } catch (e) {
    error.value = String(e)
  }
}

async function addRoot() {
  if (!settings.value) return
  const dir = await pickFolder()
  if (!dir || settings.value.roots.includes(dir)) return
  settings.value.roots = [...settings.value.roots, dir]
  await persist()
}

async function removeRoot(dir: string) {
  if (!settings.value) return
  settings.value.roots = settings.value.roots.filter((r) => r !== dir)
  await persist()
}

async function run() {
  error.value = ''
  stats.value = null
  progress.value = null
  scanning.value = true
  const t0 = performance.now()
  try {
    stats.value = await startScan()
    scanSeconds.value = (performance.now() - t0) / 1000
    emit('changed')
  } catch (e) {
    error.value = String(e)
  } finally {
    scanning.value = false
  }
}

async function runFingerprint() {
  error.value = ''
  fpText.value = '正在准备…'
  fpDone.value = 0
  fpTotal.value = 0
  fpCancelled.value = false
  fingerprinting.value = true
  try {
    const r = await startFingerprint()
    if (r.visual.cancelled || r.content.cancelled) {
      fpText.value = `已取消：视觉指纹完成 ${r.visual.done.toLocaleString('zh-CN')} 张，下次继续算`
    } else {
      fpText.value = `算完了：视觉指纹 ${r.visual.done.toLocaleString('zh-CN')} 张，失败 ${r.visual.failed} 张`
    }
    fpCancelled.value = r.visual.cancelled || r.content.cancelled
    emit('changed')
  } catch (e) {
    error.value = String(e)
    fpText.value = ''
  } finally {
    fingerprinting.value = false
  }
}

async function stop() {
  try {
    await cancelJob()
  } catch (e) {
    error.value = String(e)
  }
}
</script>

<template>
  <section class="panel">
    <h2>扫描文件夹</h2>
    <p class="hint">
      扫描全程只读：只读文件大小、修改时间和图片头，不写、不移、不改名。已经扫过且没变的文件会跳过。
    </p>

    <ul class="roots">
      <li v-for="r in settings?.roots ?? []" :key="r">
        <span class="path truncate" :title="r">{{ r }}</span>
        <button class="btn ghost sm" :disabled="scanning" @click="removeRoot(r)">移除</button>
      </li>
      <li v-if="!settings?.roots?.length" class="dim">还没有添加文件夹</li>
    </ul>

    <div class="row">
      <button class="btn" :disabled="scanning || fingerprinting" @click="addRoot">添加文件夹…</button>
      <button
        class="btn primary"
        :disabled="scanning || fingerprinting || !settings?.roots?.length"
        @click="run"
      >
        开始扫描
      </button>
      <button v-if="scanning || fingerprinting" class="btn" @click="stop">取消</button>
    </div>

    <div v-if="scanning || stats" class="progress">
      <div class="bar"><div class="fill" :style="{ width: percent + '%' }" /></div>
      <p class="num">
        已处理 {{ progress?.seen?.toLocaleString('zh-CN') ?? 0 }} /
        {{ progress?.totalHint?.toLocaleString('zh-CN') ?? '?' }}（{{ percent }}%）
        <template v-if="progress?.perSecond"> · {{ Math.round(progress.perSecond) }} 张/秒 · {{ eta }}</template>
      </p>
      <p v-if="progress?.current" class="path truncate" :title="progress.current">
        {{ progress.current }}
      </p>
    </div>

    <p v-if="stats" class="summary num">
      <template v-if="stats.cancelled">已取消（下面的数字是取消前完成的）· </template>
      新增 {{ stats.inserted.toLocaleString('zh-CN') }} · 更新 {{ stats.updated }} · 跳过
      {{ stats.skipped.toLocaleString('zh-CN') }} · 失败 {{ stats.failed }} · 用时
      {{ scanSeconds.toFixed(1) }} 秒
    </p>

    <h2>指纹计算</h2>
    <p class="hint">
      第一遍只读「大小重复」的文件算内容指纹；第二遍把每张图解出来算视觉指纹与灰度。
      这一步最慢（本机十万张量级约二十分钟），但中途关掉或取消都会保留进度，下次接着算。
    </p>
    <div class="row">
      <button class="btn primary" :disabled="fingerprinting || scanning" @click="runFingerprint">
        开始计算指纹
      </button>
      <button v-if="fingerprinting" class="btn" @click="stop">取消</button>
    </div>
    <div v-if="fpPhase === 'visual' && fpTotal" class="progress">
      <div class="bar">
        <div class="fill" :style="{ width: Math.round((fpDone / fpTotal) * 100) + '%' }" />
      </div>
    </div>
    <p v-if="fpText" class="summary num" :class="{ cancelled: fpCancelled }">{{ fpText }}</p>

    <p v-if="error" class="error-text">{{ error }}</p>
  </section>
</template>

<style scoped>
.panel {
  padding: 20px 24px;
  max-width: 880px;
  overflow-y: auto;
}
h2 {
  font-size: 16px;
  margin: 22px 0 6px;
}
h2:first-child {
  margin-top: 0;
}
.hint {
  color: var(--dim);
  font-size: 13px;
  margin: 0 0 12px;
  line-height: 1.6;
}
.roots {
  list-style: none;
  padding: 0;
  margin: 0 0 12px;
}
.roots li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 0;
  border-bottom: 1px solid var(--line);
}
.path {
  flex: 1;
  font-size: 12px;
  color: var(--dim);
}
.row {
  display: flex;
  gap: 10px;
  margin: 14px 0;
}
.progress {
  margin: 10px 0;
}
.bar {
  height: 8px;
  background: rgba(150, 160, 175, 0.2);
  border-radius: 4px;
  overflow: hidden;
}
.fill {
  height: 100%;
  background: var(--accent);
  transition: width 150ms ease;
}
.progress p {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--dim);
}
.summary {
  font-weight: 600;
  font-size: 13px;
}
.summary.cancelled {
  color: var(--warn);
}
</style>
