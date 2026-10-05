<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'

export interface MenuItem {
  key: string
  label: string
  danger?: boolean
  disabled?: boolean
}

const props = defineProps<{ x: number; y: number; items: MenuItem[] }>()
const emit = defineEmits<{ (e: 'pick', key: string): void; (e: 'close'): void }>()

const el = ref<HTMLElement | null>(null)
const pos = ref({ left: props.x + 'px', top: props.y + 'px' })

onMounted(() => {
  // 先渲染再量一次，防止菜单超出窗口
  const r = el.value?.getBoundingClientRect()
  if (r) {
    const left = r.right > window.innerWidth - 8 ? Math.max(8, window.innerWidth - r.width - 8) : r.left
    const top = r.bottom > window.innerHeight - 8 ? Math.max(8, window.innerHeight - r.height - 8) : r.top
    pos.value = { left: left + 'px', top: top + 'px' }
  }
  window.addEventListener('pointerdown', onOutside, true)
  window.addEventListener('keydown', onKey, true)
})
onUnmounted(() => {
  window.removeEventListener('pointerdown', onOutside, true)
  window.removeEventListener('keydown', onKey, true)
})

function onOutside(e: PointerEvent) {
  if (el.value && !el.value.contains(e.target as Node)) emit('close')
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    emit('close')
  }
}

function pick(item: MenuItem) {
  if (item.disabled) return
  emit('pick', item.key)
  emit('close')
}
</script>

<template>
  <div ref="el" class="menu" :style="pos" role="menu">
    <button
      v-for="item in props.items"
      :key="item.key"
      class="item"
      :class="{ danger: item.danger }"
      role="menuitem"
      :disabled="item.disabled"
      @click="pick(item)"
    >
      {{ item.label }}
    </button>
  </div>
</template>

<style scoped>
.menu {
  position: fixed;
  z-index: 70;
  min-width: 176px;
  background: #14171b;
  border: 1px solid var(--line-strong);
  border-radius: 8px;
  padding: 4px;
  box-shadow: var(--shadow-2);
}
.item {
  display: block;
  width: 100%;
  text-align: left;
  border: none;
  background: none;
  color: var(--fg);
  font: inherit;
  padding: 6px 10px;
  border-radius: var(--radius);
  cursor: pointer;
}
.item:hover:not(:disabled) {
  background: rgba(150, 160, 175, 0.16);
}
.item:disabled {
  color: var(--dimmer);
  cursor: default;
}
.item.danger {
  color: #f0a08c;
}
</style>
