<script setup lang="ts">
import { onMounted, ref } from 'vue'
import {
  fetchThumbUrl,
  listQuarantineBatches,
  purgeBatch,
  quarantineBatchFiles,
  restoreBatch,
} from '../api'
import { baseName, fmtBytes, fmtDateTime } from '../format'
import type { FileRecord, QuarantineBatch } from '../types'

const emit = defineEmits<{ (e: 'changed'): void }>()

const batches = ref<QuarantineBatch[]>([])
const msg = ref('')
const error = ref('')
const expanded = ref<string | null>(null)
const batchFiles = ref<Map<string, FileRecord[]>>(new Map())
const thumbs = ref<Map<number, string>>(new Map())
const confirming = ref<string | null>(null)

const totalBytes = () => batches.value.reduce((s, b) => s + b.bytes, 0)
const totalCount = () => batches.value.reduce((s, b) => s + b.count, 0)

const fmt = (bytes: number) => fmtBytes(bytes)
const fmtTime = (secs: number) => fmtDateTime(secs)

async function load() {
  error.value = ''
  try {
    batches.value = await listQuarantineBatches()
    if (expanded.value && !batches.value.some((b) => b.batchId === expanded.value)) {
      expanded.value = null
    }
  } catch (e) {
    error.value = String(e)
  }
}

async function toggleExpand(b: QuarantineBatch) {
  if (expanded.value === b.batchId) {
    expanded.value = null
    return
  }
  expanded.value = b.batchId
  if (!batchFiles.value.has(b.batchId)) {
    try {
      const files = await quarantineBatchFiles(b.batchId)
      const nextFiles = new Map(batchFiles.value)
      nextFiles.set(b.batchId, files)
      batchFiles.value = nextFiles
      const nextThumbs = new Map(thumbs.value)
      for (const f of files.slice(0, 60)) {
        if (nextThumbs.has(f.id)) continue
        try {
          nextThumbs.set(f.id, await fetchThumbUrl(f.id))
        } catch {
          /* 读不出就留空位 */
        }
      }
      thumbs.value = nextThumbs
    } catch (e) {
      error.value = String(e)
    }
  }
}

async function doRestore(b: QuarantineBatch) {
  error.value = ''
  try {
    const r = await restoreBatch(b.batchId)
    msg.value = `已搬回 ${r.moved} 张${r.failed ? `，失败 ${r.failed} 张` : ''}`
    await load()
    emit('changed')
  } catch (e) {
    error.value = String(e)
  }
}

async function doPurge(b: QuarantineBatch) {
  error.value = ''
  try {
    const r = await purgeBatch(b.batchId)
    msg.value = `已永久删除 ${r.purged} 张，释放 ${fmt(r.bytes)}`
    confirming.value = null
    expanded.value = null
    await load()
    emit('changed')
  } catch (e) {
    error.value = String(e)
  }
}

/** 清空前把文件名摆出来：确认框里只写「N 个文件」是不够的 */
function namesPreview(b: QuarantineBatch): string {
  const files = batchFiles.value.get(b.batchId) ?? []
  if (!files.length) return ''
  const shown = files.slice(0, 8).map((f) => baseName(f.path))
  const rest = files.length - shown.length
  return shown.join('、') + (rest > 0 ? ` 等 ${files.length} 个文件` : '')
}

onMounted(load)
</script>

<template>
  <section class="wrap">
    <header>
      <h2>隔离区</h2>
      <span class="dim small num">
        {{ totalCount().toLocaleString('zh-CN') }} 张 · {{ fmt(totalBytes()) }}
      </span>
      <span class="grow" />
      <button class="btn ghost sm" @click="load">刷新</button>
    </header>
    <p class="hint">
      这里的文件只是被搬过来了（同盘搬动是瞬时的，不占额外空间），随时可以搬回原位。
      只有点了「彻底清空」才真的删除，而且要点两次。
    </p>
    <p v-if="msg" class="ok-text">{{ msg }}</p>
    <p v-if="error" class="error-text">{{ error }}</p>

    <table>
      <thead>
        <tr>
          <th>批次</th>
          <th>张数</th>
          <th>体积</th>
          <th>移入时间</th>
          <th class="ops-col">操作</th>
        </tr>
      </thead>
      <tbody>
        <template v-for="b in batches" :key="b.batchId">
          <tr>
            <td><code>{{ b.batchId.slice(0, 8) }}</code></td>
            <td class="num">{{ b.count }}</td>
            <td class="num">{{ fmt(b.bytes) }}</td>
            <td class="num time">{{ fmtTime(b.movedAt) }}</td>
            <td class="ops">
              <button class="btn ghost sm" @click="toggleExpand(b)">
                {{ expanded === b.batchId ? '收起 ▴' : '看是哪几张 ▾' }}
              </button>
              <button class="btn ghost sm" @click="doRestore(b)">搬回来</button>
              <template v-if="confirming !== b.batchId">
                <button class="btn danger sm" @click="confirming = b.batchId">彻底清空</button>
              </template>
              <template v-else>
                <button class="btn danger sm" @click="doPurge(b)">确定永久删除</button>
                <button class="btn ghost sm" @click="confirming = null">取消</button>
              </template>
            </td>
          </tr>
          <tr v-if="confirming === b.batchId" class="confirm-row">
            <td colspan="5">
              <span class="warn">
                将永久删除 {{ b.count }} 个文件，释放约 {{ fmt(b.bytes) }}，此操作不可撤销。
              </span>
              <span v-if="namesPreview(b)" class="dim small">（{{ namesPreview(b) }}）</span>
            </td>
          </tr>
          <tr v-if="expanded === b.batchId" class="detail-row">
            <td colspan="5">
              <div class="files">
                <div v-for="f in batchFiles.get(b.batchId) ?? []" :key="f.id" class="file">
                  <img v-if="thumbs.get(f.id)" :src="thumbs.get(f.id)" alt="" />
                  <div v-else class="ph">…</div>
                  <span class="fname truncate" :title="f.path">{{ baseName(f.path) }}</span>
                  <span class="dim tiny num">{{ fmt(f.size) }}</span>
                </div>
              </div>
            </td>
          </tr>
        </template>
        <tr v-if="!batches.length">
          <td colspan="5" class="empty">隔离区是空的</td>
        </tr>
      </tbody>
    </table>
  </section>
</template>

<style scoped>
.wrap {
  padding: 20px 24px;
  overflow-y: auto;
}
header {
  display: flex;
  align-items: center;
  gap: 12px;
}
h2 {
  margin: 0;
  font-size: 16px;
}
.grow {
  flex: 1;
}
.hint {
  color: var(--dim);
  font-size: 13px;
  max-width: 760px;
}
.time {
  font-size: 12px;
  color: var(--dim);
}
table {
  width: 100%;
  border-collapse: collapse;
  margin-top: 12px;
}
th,
td {
  text-align: left;
  padding: 8px 8px 8px 0;
  border-bottom: 1px solid var(--line);
  vertical-align: middle;
}
th {
  font-size: 12px;
  color: var(--dim);
  font-weight: 500;
}
.ops {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.ops-col {
  width: 46%;
}
.confirm-row td {
  background: var(--danger-soft);
  padding: 8px 10px;
}
.warn {
  color: #f0a08c;
  font-size: 12px;
}
.detail-row td {
  padding: 10px 0 14px;
}
.files {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 8px;
}
.file {
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--card);
  border: 1px solid var(--line);
  border-radius: var(--radius);
  padding: 4px 6px;
}
.file img,
.file .ph {
  width: 28px;
  height: 36px;
  object-fit: cover;
  border-radius: 3px;
  background: #333941;
}
.file .ph {
  display: grid;
  place-items: center;
  color: var(--dimmer);
}
.fname {
  flex: 1;
  font-size: 12px;
}
.empty {
  text-align: center;
  color: var(--dim);
  padding: 26px 0;
}
</style>
