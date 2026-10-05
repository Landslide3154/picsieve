<script setup lang="ts">
import { ref } from 'vue'

const ready = ref(false)
</script>

<template>
  <main class="boot">
    <h1>图筛 PicSieve</h1>
    <p>骨架已就绪，扫描与图库界面正在接入。</p>
    <p class="hint">当前状态：{{ ready ? '已连接后端' : '等待后端命令接入' }}</p>
  </main>
</template>

<style scoped>
.boot {
  padding: 40px;
}
.hint {
  opacity: 0.6;
}
</style>
