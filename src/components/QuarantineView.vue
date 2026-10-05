<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { listQuarantineBatches, purgeBatch, restoreBatch } from '../api'
import type { QuarantineBatch } from '../types'

const batches = ref<QuarantineBatch[]>([])
const msg = ref('')
const error = ref('')
const confirming = ref<string | null>(null)

function fmt(bytes: number): string {
  if (bytes >= 1073741824) return (bytes / 1073741824).toFixed(2) + ' GB'
  return (bytes / 1048576).toFixed(1) + ' MB'
}

function fmtTime(secs: number): string {
  if (!secs) return '—'
  return new Date(secs * 1000).toLocaleString('zh-CN', { hour12: false })
}

async function load() {
  error.value = ''
  try {
    batches.value = await listQuarantineBatches()
  } catch (e) {
    error.value = String(e)
  }
}

async function doRestore(b: QuarantineBatch) {
  error.value = ''
  try {
    const r = await restoreBatch(b.batchId)
    msg.value = `已搬回 ${r.moved} 张${r.failed ? `，失败 ${r.failed} 张` : ''}`
    await load()
  } catch (e) {
    error.value = String(e)
  }
}

/** 彻底清空要二次确认：先切到确认态，再点一次才真的删。 */
async function doPurge(b: QuarantineBatch) {
  error.value = ''
  try {
    const r = await purgeBatch(b.batchId)
    msg.value = `已永久删除 ${r.purged} 张，释放 ${fmt(r.bytes)}`
    confirming.value = null
    await load()
  } catch (e) {
    error.value = String(e)
  }
}

onMounted(load)
</script>

<template>
  <section class="wrap">
    <header>
      <h2>隔离区</h2>
      <button class="ghost" @click="load">刷新</button>
    </header>
    <p class="hint">
      这里的文件只是被搬过来了，随时可以搬回原位。只有点了「彻底清空」才真的删除，而且要点两次。
    </p>
    <p v-if="msg" class="msg">{{ msg }}</p>
    <p v-if="error" class="error">{{ error }}</p>

    <table>
      <thead>
        <tr>
          <th>批次</th>
          <th>张数</th>
          <th>体积</th>
          <th>移入时间</th>
          <th>操作</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="b in batches" :key="b.batchId">
          <td><code>{{ b.batchId.slice(0, 8) }}</code></td>
          <td>{{ b.count }}</td>
          <td>{{ fmt(b.bytes) }}</td>
          <td class="time">{{ fmtTime(b.movedAt) }}</td>
          <td class="ops">
            <button @click="doRestore(b)">搬回来</button>
            <template v-if="confirming !== b.batchId">
              <button class="danger" @click="confirming = b.batchId">
                彻底清空
              </button>
            </template>
            <template v-else>
              <span class="warn">
                将永久删除 {{ b.count }} 个文件，释放约 {{ fmt(b.bytes) }}，不可撤销
              </span>
              <button class="danger" @click="doPurge(b)">确定永久删除</button>
              <button class="ghost" @click="confirming = null">取消</button>
            </template>
          </td>
        </tr>
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
.hint {
  opacity: 0.65;
  font-size: 13px;
}
.msg {
  color: var(--accent);
}
.error {
  color: #e05c4b;
}
.time {
  opacity: 0.7;
  font-size: 12px;
}
table {
  width: 100%;
  border-collapse: collapse;
  margin-top: 14px;
}
th,
td {
  text-align: left;
  padding: 8px;
  border-bottom: 1px solid var(--line);
  vertical-align: middle;
}
.ops {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
}
button {
  border: 1px solid var(--line);
  background: transparent;
  color: inherit;
  border-radius: 4px;
  padding: 3px 10px;
  cursor: pointer;
}
.danger {
  background: var(--danger);
  border-color: var(--danger);
  color: #fff;
}
.ghost {
  opacity: 0.8;
}
.warn {
  color: var(--danger);
  font-size: 12px;
}
.empty {
  opacity: 0.5;
  text-align: center;
}
</style>
