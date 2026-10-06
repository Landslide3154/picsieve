import { defineStore } from 'pinia'
import { computed, ref, shallowRef } from 'vue'
import { countFiles as apiCount, formatStats, histogram, libraryStats, queryFiles } from '../api'
import type { FileRecord, Filter, FormatStat, Histogram, LibraryStats } from '../types'

/** 一次取多少张。滚到底会自动再取一批，见 loadMore。 */
export const PAGE = 300

export const SORT_OPTIONS: { value: string; label: string }[] = [
  { value: 'sizeDesc', label: '体积：大到小' },
  { value: 'sizeAsc', label: '体积：小到大' },
  { value: 'pixelsDesc', label: '清晰度：高到低' },
  { value: 'pixelsAsc', label: '清晰度：低到高' },
  { value: 'pathAsc', label: '路径' },
]

export function emptyFilter(): Filter {
  return {
    minPixels: null,
    maxPixels: null,
    minSize: null,
    maxSize: null,
    // 空数组 = 不限格式：库里有什么格式就都能看到，不写死列表
    exts: [],
    onlyGray: false,
    onlyDuplicated: false,
    onlyDecodeError: false,
    search: null,
    sort: 'sizeDesc',
    limit: PAGE,
    offset: 0,
  }
}

export const useLibrary = defineStore('library', () => {
  const filter = ref<Filter>(emptyFilter())
  // 文件列表用 shallowRef：十万级的记录如果每条都包成响应式代理，
  // 内存和访问开销都翻好几倍。这里整表替换（不是原地改），shallowRef 完全够用。
  const files = shallowRef<FileRecord[]>([])
  const total = ref(0)
  const selected = shallowRef<Set<number>>(new Set())
  const stats = ref<LibraryStats>({ total: 0, bytes: 0, lastScanAt: 0 })
  const histPixels = ref<Histogram | null>(null)
  const histSize = ref<Histogram | null>(null)
  /** 库里实际存在的格式（按张数从多到少），格式选项由它生成 */
  const formats = ref<FormatStat[]>([])
  const loading = ref(false)
  const loadingMore = ref(false)
  const error = ref('')

  /** 已经取到的还不到命中总数，就说明下面还有 */
  const hasMore = computed(() => files.value.length < total.value)
  const selectedFiles = computed(() => files.value.filter((f) => selected.value.has(f.id)))
  const selectedCount = computed(() => selected.value.size)
  const selectedBytes = computed(() => selectedFiles.value.reduce((s, f) => s + f.size, 0))

  // 查询序号：只有最新一次查询的结果才允许落到界面上，
  // 否则拖滑块时先发的慢查询后到，会把新结果覆盖回旧的。
  let listSeq = 0
  let countSeq = 0
  let countTimer: number | undefined
  let refreshTimer: number | undefined

  /** 选中项只在「看得见」的范围内保留：筛选一变，看不见的选中就丢掉，避免误删。 */
  function pruneSelection() {
    const visible = new Set(files.value.map((f) => f.id))
    let changed = false
    const next = new Set<number>()
    for (const id of selected.value) {
      if (visible.has(id)) next.add(id)
      else changed = true
    }
    if (changed) selected.value = next
  }

  /** 立刻整表刷新（换排序、改搜索、勾选筛选条件、首次进入时用） */
  async function refreshNow() {
    if (refreshTimer !== undefined) {
      clearTimeout(refreshTimer)
      refreshTimer = undefined
    }
    const my = ++listSeq
    loading.value = true
    error.value = ''
    try {
      const listPromise = queryFiles({ ...filter.value, offset: 0, limit: PAGE })
      const countPromise = apiCount(filter.value)
      const list = await listPromise
      if (my !== listSeq) return
      files.value = list
      pruneSelection()
      const n = await countPromise
      if (my !== listSeq) return
      total.value = n
    } catch (e) {
      if (my === listSeq) error.value = String(e)
    } finally {
      // 同上：复位标记不能加「序号没变」的条件，否则一次被打断就永久卡住
      loading.value = false
    }
  }

  /** 滚到底时接着取下一批。batch 默认一屏的量；End「一路到底」时用更大的批，少跑几趟 */
  async function loadMore(batch = PAGE) {
    if (loading.value || loadingMore.value || !hasMore.value) return
    const my = listSeq
    loadingMore.value = true
    try {
      const list = await queryFiles({
        ...filter.value,
        offset: files.value.length,
        limit: batch,
      })
      if (my !== listSeq) {
        // 取的过程中筛选条件变了，这批作废；稍后按新条件重试一次，
        // 否则用户正好在刷新时滚到底，就会卡在「正在加载更多」
        window.setTimeout(() => void loadMore(), 300)
        return
      }
      const seen = new Set(files.value.map((f) => f.id))
      files.value = files.value.concat(list.filter((f) => !seen.has(f.id)))
    } catch (e) {
      if (my === listSeq) error.value = String(e)
    } finally {
      // 必须无条件复位：若期间筛选变了（my !== listSeq）而不复位，
      // 这个标记会一直是 true，之后 loadMore 的入口守卫永远直接 return，
      // 表现就是「滚到底再也不加载了」（实测踩到过）。
      loadingMore.value = false
    }
  }

  /** 拖动滑块时用：只更新「命中 N 张」，不重画图片墙 */
  function scheduleCount(delay = 120) {
    if (countTimer !== undefined) clearTimeout(countTimer)
    countTimer = window.setTimeout(async () => {
      const my = ++countSeq
      try {
        const n = await apiCount(filter.value)
        if (my === countSeq) total.value = n
      } catch {
        /* 计数失败不值得打断用户 */
      }
    }, delay)
  }

  /** 拖动滑块时用：停手一小会儿再整表刷新 */
  function scheduleRefresh(delay = 250) {
    if (refreshTimer !== undefined) clearTimeout(refreshTimer)
    refreshTimer = window.setTimeout(() => {
      refreshTimer = undefined
      void refreshNow()
    }, delay)
  }

  function patchFilter(patch: Partial<Filter>) {
    filter.value = { ...filter.value, ...patch, offset: 0 }
  }

  /** 立即生效（勾选、点标签、换排序） */
  function applyFilter(patch: Partial<Filter>) {
    patchFilter(patch)
    // 立即生效的改动不保留选中，避免「选中的其实已经被筛掉了」
    return refreshNow()
  }

  /** 拖动中：命中数实时变，图片墙等停手 */
  function dragFilter(patch: Partial<Filter>) {
    patchFilter(patch)
    scheduleCount()
    scheduleRefresh()
  }

  function resetFacet(patch: Partial<Filter>) {
    return applyFilter(patch)
  }

  function resetAll() {
    filter.value = emptyFilter()
    selected.value = new Set()
    return refreshNow()
  }

  // ---------- 选中 ----------
  function toggle(id: number) {
    const next = new Set(selected.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selected.value = next
  }

  function setSelected(id: number, on: boolean) {
    const next = new Set(selected.value)
    if (on) next.add(id)
    else next.delete(id)
    selected.value = next
  }

  /** 全选「已经加载出来的」结果（没加载的还没看过，不该被选中） */
  function selectAllLoaded() {
    selected.value = new Set(files.value.map((f) => f.id))
  }

  function selectRange(fromIndex: number, toIndex: number) {
    const [a, b] = fromIndex <= toIndex ? [fromIndex, toIndex] : [toIndex, fromIndex]
    const next = new Set(selected.value)
    for (let i = a; i <= b; i++) {
      const f = files.value[i]
      if (f) next.add(f.id)
    }
    selected.value = next
  }

  function clearSelection() {
    selected.value = new Set()
  }

  // ---------- 统计与分布 ----------
  async function loadStats() {
    try {
      stats.value = await libraryStats()
    } catch {
      /* 顶栏统计失败不影响主流程 */
    }
  }

  async function loadHistograms() {
    try {
      const [p, z] = await Promise.all([histogram('pixels'), histogram('size')])
      histPixels.value = p
      histSize.value = z
    } catch {
      /* 画不出分布图也能用 */
    }
  }

  /** 格式选项按库里实际有的来（扫描/隔离后要重新取） */
  async function loadFormats() {
    try {
      formats.value = await formatStats()
    } catch {
      /* 取不到就退回「不限格式」 */
    }
  }

  return {
    filter,
    files,
    total,
    selected,
    stats,
    histPixels,
    histSize,
    formats,
    loading,
    loadingMore,
    error,
    hasMore,
    selectedFiles,
    selectedCount,
    selectedBytes,
    refreshNow,
    loadMore,
    scheduleCount,
    scheduleRefresh,
    patchFilter,
    applyFilter,
    dragFilter,
    resetFacet,
    resetAll,
    toggle,
    setSelected,
    selectAllLoaded,
    selectRange,
    clearSelection,
    loadStats,
    loadHistograms,
    loadFormats,
  }
})
