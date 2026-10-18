<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'

const props = withDefaults(
  defineProps<{
    title: string
    body: string
    confirmText?: string
    cancelText?: string
    danger?: boolean
  }>(),
  { confirmText: '确认', cancelText: '取消', danger: false },
)
const emit = defineEmits<{ (e: 'confirm'): void; (e: 'cancel'): void }>()

// 捕获阶段拦：弹窗在的时候 Esc 只关弹窗，别让底下的图库也跟着响应
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('cancel')
  } else if (e.key === 'Enter') {
    e.preventDefault()
    e.stopPropagation()
    emit('confirm')
  }
}
onMounted(() => window.addEventListener('keydown', onKey, true))
onUnmounted(() => window.removeEventListener('keydown', onKey, true))
</script>

<template>
  <div class="mask" role="dialog" aria-modal="true" :aria-label="props.title" @click.self="emit('cancel')">
    <div class="box">
      <h3>{{ props.title }}</h3>
      <p class="body">{{ props.body }}</p>
      <div class="foot">
        <button class="btn ghost" @click="emit('cancel')">{{ props.cancelText }}（Esc）</button>
        <button class="btn" :class="props.danger ? 'danger' : 'primary'" @click="emit('confirm')">
          {{ props.confirmText }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 0;
  z-index: 90;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(2px);
}
.box {
  width: min(520px, 92vw);
  background: var(--panel);
  border: 1px solid var(--line-strong);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-2);
  padding: 18px 20px 14px;
}
h3 {
  margin: 0 0 8px;
  font-size: 15px;
  font-weight: 600;
}
.body {
  margin: 0 0 16px;
  color: var(--dim);
  font-size: 13px;
  line-height: 1.7;
}
.foot {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}
</style>
