<script setup lang="ts">
import { useLibrary } from '../stores/library'

const store = useLibrary()
defineEmits<{ (e: 'move-to-quarantine'): void }>()
</script>

<template>
  <footer class="bar">
    <span>
      已选 <strong>{{ store.selectedCount }}</strong> 张（{{
        (store.selectedBytes / 1048576).toFixed(1)
      }}
      MB）
    </span>
    <button v-if="store.selectedCount" class="link" @click="store.clearSelection()">
      取消选择
    </button>
    <span class="flex" />
    <button
      class="danger"
      :disabled="!store.selectedCount"
      @click="$emit('move-to-quarantine')"
    >
      移到隔离区
    </button>
  </footer>
</template>

<style scoped>
.bar {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 8px 14px;
  border-top: 1px solid var(--line);
}
.link {
  background: none;
  border: none;
  color: var(--accent);
  cursor: pointer;
}
.flex {
  flex: 1;
}
.danger {
  background: var(--danger);
  color: #fff;
  border: none;
  border-radius: 4px;
  padding: 5px 16px;
  cursor: pointer;
}
.danger:disabled {
  opacity: 0.4;
  cursor: default;
}
</style>
