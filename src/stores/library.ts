import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { countFiles, queryFiles } from '../api'
import type { FileRecord, Filter } from '../types'

function emptyFilter(): Filter {
  return {
    minShortSide: null,
    maxShortSide: null,
    minSize: null,
    maxSize: null,
    exts: ['jpg', 'png', 'gif'],
    onlyGray: false,
    onlyDuplicated: false,
    onlyDecodeError: false,
    search: null,
    sort: 'sizeDesc',
    limit: 300,
    offset: 0,
  }
}

export const useLibrary = defineStore('library', () => {
  const filter = ref<Filter>(emptyFilter())
  const files = ref<FileRecord[]>([])
  const total = ref(0)
  const selected = ref<Set<number>>(new Set())
  const loading = ref(false)
  const error = ref('')

  const selectedCount = computed(() => selected.value.size)
  const selectedBytes = computed(() =>
    files.value.filter((f) => selected.value.has(f.id)).reduce((s, f) => s + f.size, 0),
  )

  async function refresh() {
    loading.value = true
    error.value = ''
    try {
      const [list, n] = await Promise.all([queryFiles(filter.value), countFiles(filter.value)])
      files.value = list
      total.value = n
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  function toggle(id: number) {
    const next = new Set(selected.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selected.value = next
  }

  function clearSelection() {
    selected.value = new Set()
  }

  function patchFilter(patch: Partial<Filter>) {
    filter.value = { ...filter.value, ...patch, offset: 0 }
    return refresh()
  }

  function resetFilter() {
    filter.value = emptyFilter()
    selected.value = new Set()
    return refresh()
  }

  return {
    filter,
    files,
    total,
    selected,
    loading,
    error,
    selectedCount,
    selectedBytes,
    refresh,
    toggle,
    clearSelection,
    patchFilter,
    resetFilter,
  }
})
