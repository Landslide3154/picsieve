<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { fetchThumbUrl } from '../api'
import type { FileRecord } from '../types'

const props = defineProps<{
  file: FileRecord
  selected: boolean
  focused: boolean
  dupCount: number
  grayThreshold: number
}>()

const emit = defineEmits<{
  (e: 'select', id: number, mods: { shift: boolean; ctrl: boolean }): void
  (e: 'open', file: FileRecord): void
  (e: 'menu', file: FileRecord, ev: MouseEvent): void
  (e: 'hover', file: FileRecord, el: HTMLElement): void
  (e: 'leave'): void
}>()

const url = ref('')
const failed = ref(false)

watch(
  () => props.file.id,
  async () => {
    if (url.value) {
      URL.revokeObjectURL(url.value)
      url.value = ''
    }
    failed.value = false
    try {
      url.value = await fetchThumbUrl(props.file.id)
    } catch {
      failed.value = true
    }
  },
  { immediate: true },
)

onUnmounted(() => {
  if (url.value) URL.revokeObjectURL(url.value)
})

const isGray = () => (props.file.grayScore ?? 999) <= props.grayThreshold

function onClick(e: MouseEvent) {
  emit('select', props.file.id, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey })
}
</script>

<template>
  <div
    class="cell"
    :class="{ sel: props.selected, focused: props.focused }"
    role="option"
    :aria-selected="props.selected"
    :aria-label="props.file.path"
    :tabindex="props.focused ? 0 : -1"
    @click="onClick"
    @dblclick="emit('open', props.file)"
    @contextmenu.prevent="emit('menu', props.file, $event)"
    @mouseenter="emit('hover', props.file, $event.currentTarget as HTMLElement)"
    @mouseleave="emit('leave')"
  >
    <div class="frame">
      <img v-if="url" :src="url" alt="" loading="lazy" draggable="false" />
      <div v-else class="ph">{{ failed ? '读不出' : '…' }}</div>
      <span v-if="props.dupCount > 1" class="badge dup num">×{{ props.dupCount }}</span>
      <span v-if="isGray()" class="badge gray">灰</span>
      <span v-if="props.file.decodeError" class="badge bad" title="这个文件读不出来">坏</span>
    </div>
    <span v-if="props.selected" class="tick" aria-hidden="true">✓</span>
  </div>
</template>

<style scoped>
.cell {
  position: relative;
  aspect-ratio: 1 / 1.32;
  border-radius: 8px;
  cursor: pointer;
  transform-origin: center;
  /* 只动 transform 和 box-shadow：这两样走合成层，不会引起整页重排 */
  transition:
    transform 140ms cubic-bezier(0.2, 0.7, 0.3, 1),
    box-shadow 140ms ease;
}
.frame {
  position: absolute;
  inset: 0;
  border-radius: 8px;
  overflow: hidden;
  background: var(--card);
  outline: 1px solid rgba(150, 160, 175, 0.14);
  outline-offset: -1px;
}
img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  user-select: none;
}
.ph {
  display: grid;
  place-items: center;
  height: 100%;
  font-size: 11px;
  color: var(--dimmer);
}

/* 悬停：轻轻抬起 */
.cell:hover {
  transform: translateY(-3px) scale(1.015);
  box-shadow: 0 10px 22px rgba(0, 0, 0, 0.5);
}

/* 选中：放大 + 投影「凸出来」，描边画在格子内部。
   之前描边画在格子外面（box-shadow 外扩 5px），两张挨着的图会互相压住对方的描边；
   改成 inset 之后无论选多少张、怎么相邻都不会重叠。 */
.cell.sel {
  transform: scale(1.045);
  z-index: 3;
  /* 一层柔光（模糊的，相邻时会自然融合不会硬遮挡）+ 一层投影做「凸出来」 */
  box-shadow:
    0 0 14px rgba(108, 176, 255, 0.5),
    0 14px 28px rgba(0, 0, 0, 0.55);
}
.cell.sel .frame {
  box-shadow:
    inset 0 0 0 4px var(--accent-bright),
    inset 0 0 0 5.5px rgba(255, 255, 255, 0.92);
}
.cell.sel .frame::after {
  content: '';
  position: absolute;
  inset: 0;
  background: linear-gradient(180deg, rgba(108, 176, 255, 0.12), rgba(0, 0, 0, 0.18));
}
.cell.focused {
  z-index: 2;
}
.cell.focused .frame {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}
.cell:focus-visible {
  outline: none;
}

.tick {
  position: absolute;
  right: 6px;
  top: 6px;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: var(--accent-bright);
  color: #10233b;
  font-weight: 800;
  font-size: 14px;
  display: grid;
  place-items: center;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.55);
  z-index: 4;
  pointer-events: none;
}

.badge {
  position: absolute;
  left: 6px;
  font-size: 11px;
  line-height: 1.5;
  padding: 0 6px;
  border-radius: 5px;
  background: rgba(0, 0, 0, 0.68);
  color: #fff;
  z-index: 4;
  pointer-events: none;
}
.badge.dup {
  top: 6px;
}
.badge.gray {
  top: 28px;
  color: #cfe3ff;
}
.badge.bad {
  top: 28px;
  background: var(--danger);
}
.badge.gray + .badge.bad {
  top: 50px;
}
</style>
