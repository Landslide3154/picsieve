import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'
import { countDupGroups, listDupGroups } from '../api'
import type { FileRecord, GroupView } from '../types'

/** 一次取多少组 */
export const GROUP_BATCH = 20

export interface GroupMember {
  file: FileRecord
  /** 到基准图的汉明距离；null = 不知道（换过基准或手动改过） */
  distance: number | null
}

export interface GroupTile {
  groupId: number
  /** 后端建议保留的那张（只作为默认勾选的依据，不代表最终结果） */
  keep: FileRecord
  /** 这一组里除建议保留项以外的成员，默认全部勾选（= 要删的） */
  members: GroupMember[]
  savings: number
  reason: string
}

/**
 * 重复组页的状态。
 *
 * 勾选语义：**打勾 = 要删（移入隔离区）**，没打勾 = 留下。
 * 默认把「除建议保留项以外」的全部勾上，所以两张相同的组默认勾 1 张、
 * 三张相同的组默认勾 2 张；用户想留两张就把勾去掉，想都删就都勾上。
 */
export const useGroups = defineStore('groups', () => {
  const tiles = shallowRef<GroupTile[]>([])
  const total = ref(0)
  const kind = ref<'exact' | 'similar'>('exact')
  /** 勾选的 = 要删的 */
  const checked = shallowRef<Set<number>>(new Set())
  const loading = ref(false)
  const loadingMore = ref(false)
  const error = ref('')

  const hasMore = computed(() => tiles.value.length < total.value)
  const checkedIds = computed(() => [...checked.value])
  const checkedCount = computed(() => checked.value.size)

  /** 勾选的总字节数（顺便给「能省多少」用） */
  const checkedBytes = computed(() => {
    let sum = 0
    for (const t of tiles.value) {
      if (t.members.some((m) => checked.value.has(m.file.id))) {
        for (const m of t.members) if (checked.value.has(m.file.id)) sum += m.file.size
      }
      if (checked.value.has(t.keep.id)) sum += t.keep.size
    }
    return sum
  })

  function toTile(g: GroupView): GroupTile {
    return {
      groupId: g.groupId,
      keep: g.keep,
      members: g.members.map((f, i) => ({ file: f, distance: g.distances[i] ?? null })),
      savings: g.savings,
      reason: g.keepReason,
    }
  }

  /** 把一组里所有图片（含建议保留项）列出来，方便点选 */
  function allFiles(t: GroupTile): FileRecord[] {
    return [t.keep, ...t.members.map((m) => m.file)]
  }

  function isChecked(id: number): boolean {
    return checked.value.has(id)
  }

  function toggle(id: number) {
    const next = new Set(checked.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    checked.value = next
  }

  /** 默认勾选：除「建议保留的那张」以外全勾（两张组 → 1 张，三张组 → 2 张） */
  function applyDefaults(list: GroupTile[] = tiles.value) {
    const next = new Set(checked.value)
    for (const t of list) {
      for (const m of t.members) next.add(m.file.id)
      next.delete(t.keep.id)
    }
    checked.value = next
  }

  function clearChecked() {
    checked.value = new Set()
  }

  function checkAll() {
    const next = new Set(checked.value)
    for (const t of tiles.value) for (const f of allFiles(t)) next.add(f.id)
    checked.value = next
  }

  async function load(reset = true) {
    if (reset) {
      loading.value = true
      error.value = ''
    } else {
      if (loadingMore.value || !hasMore.value) return
      loadingMore.value = true
    }
    try {
      const offset = reset ? 0 : tiles.value.length
      const list = await listDupGroups(kind.value, offset, GROUP_BATCH)
      const next = list.map(toTile)
      tiles.value = reset ? next : tiles.value.concat(next)
      applyDefaults(next)
      if (reset) total.value = await countDupGroups(kind.value)
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
      loadingMore.value = false
    }
  }

  async function setKind(k: 'exact' | 'similar') {
    kind.value = k
    checked.value = new Set()
    tiles.value = []
    total.value = 0
    await load(true)
  }

  return {
    tiles,
    total,
    kind,
    checked,
    loading,
    loadingMore,
    error,
    hasMore,
    checkedIds,
    checkedCount,
    checkedBytes,
    isChecked,
    toggle,
    applyDefaults,
    clearChecked,
    checkAll,
    allFiles,
    load,
    setKind,
  }
})
