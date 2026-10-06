import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  FileRecord,
  Filter,
  FingerprintResult,
  FormatStat,
  GroupView,
  Histogram,
  LibraryStats,
  MoveReport,
  PurgeReport,
  QuarantineBatch,
  RebuildResult,
  ScanProgress,
  ScanStats,
  Settings,
  TrimReport,
} from './types'

// ---------- 设置与扫描 ----------
export const getSettings = () => invoke<Settings>('get_settings')
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings })
export const startScan = () => invoke<ScanStats>('start_scan')
export const startFingerprint = () => invoke<FingerprintResult>('start_fingerprint')
/** 取消正在跑的扫描或指纹计算 */
export const cancelJob = () => invoke<void>('cancel_job')

export function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>('scan://progress', (e) => cb(e.payload))
}

export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, title: '选择要扫描的文件夹' })
  return typeof picked === 'string' ? picked : null
}

// ---------- 图库 ----------
export const queryFiles = (filter: Filter) => invoke<FileRecord[]>('query_files_cmd', { filter })
export const countFiles = (filter: Filter) => invoke<number>('count_files_cmd', { filter })
export const libraryStats = () => invoke<LibraryStats>('library_stats')
export const histogram = (kind: 'size' | 'pixels') =>
  invoke<Histogram>('histogram_cmd', { kind })
/** 库里实际有哪些格式、各多少张 */
export const formatStats = () => invoke<FormatStat[]>('format_stats_cmd')
/** 第一帧画好后显示主窗口（窗口在配置里是隐藏的，等恢复了上次的位置大小再露脸） */
export const showMainWindow = () => invoke<void>('show_main_window')

// ---------- 缩略图与预览 ----------
/**
 * 缩略图并发闸门：最多同时 4 张。
 *
 * 尖峰来自「一屏几十张格子同时要图」和「重复组一屏几百张」——一张 4000×6000 的图
 * 解码后是几十 MB，几十张同时解码会把内存顶到几个 GB。
 * 放在这里排队而不是后端：后端每张排队的请求都占一条阻塞线程，实测会把线程池拉爆并卡死。
 */
const MAX_PARALLEL_THUMBS = 6
let thumbRunning = 0

interface ThumbJob {
  start: () => void
  skip: () => void
}
const thumbQueue: ThumbJob[] = []

function pumpThumbs() {
  while (thumbRunning < MAX_PARALLEL_THUMBS && thumbQueue.length) {
    thumbQueue.shift()!.start()
  }
}

/** 取缩略图并转成可直接放进 <img src> 的 Blob URL。调用方负责在不用时 revoke。 */
export function fetchThumbUrl(fileId: number, cancelled?: () => boolean): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const skip = () => resolve('')
    const start = () => {
      if (cancelled?.()) return skip()
      thumbRunning++
      void (async () => {
        try {
          const raw = await invoke<ArrayBuffer | number[]>('get_thumb', { fileId })
          // 请求期间格子可能已经滚出画面了，那就别再建 Blob 占用内存
          if (cancelled?.()) return skip()
          resolve(toBlobUrl(raw))
        } catch (e) {
          reject(e)
        } finally {
          thumbRunning--
          pumpThumbs()
        }
      })()
    }
    thumbQueue.push({ start, skip })
    // 快速滚动时队列会被灌满（每个划过的格子都会来要一张）：
    // 超出上限的老请求直接作废，反正它们早就滚出屏幕了。
    if (thumbQueue.length > 400) {
      const stale = thumbQueue.splice(0, thumbQueue.length - 400)
      for (const j of stale) j.skip()
    }
    pumpThumbs()
  })
}

/** 空格预览用的大图（长边 1600，单独一份缓存） */
export async function fetchPreviewUrl(fileId: number): Promise<string> {
  const raw = await invoke<ArrayBuffer | number[]>('get_preview', { fileId })
  return toBlobUrl(raw)
}

function toBlobUrl(raw: ArrayBuffer | number[]): string {
  const bytes = raw instanceof ArrayBuffer ? new Uint8Array(raw) : new Uint8Array(raw)
  return URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }))
}

// ---------- 用系统程序打开 ----------
export const openExternal = (path: string) => invoke<void>('open_external', { path })
export const revealInExplorer = (path: string) => invoke<void>('reveal_in_explorer', { path })

// ---------- 重复组 ----------
export const listDupGroups = (kind: string, offset: number, limit: number) =>
  invoke<GroupView[]>('list_dup_groups', { kind, offset, limit })
export const countDupGroups = (kind: string) => invoke<number>('count_dup_groups', { kind })
export const setKeeper = (groupId: number, fileId: number) =>
  invoke<void>('set_keeper', { groupId, fileId })
export const rebuildGroups = () => invoke<RebuildResult>('rebuild_groups')

// ---------- 隔离区 ----------
export const moveToQuarantine = (fileIds: number[]) =>
  invoke<MoveReport>('move_to_quarantine', { fileIds })
export const restoreBatch = (batch: string) => invoke<MoveReport>('restore_batch', { batch })
export const purgeBatch = (batch: string) => invoke<PurgeReport>('purge_batch', { batch })
export const listQuarantineBatches = () => invoke<QuarantineBatch[]>('list_quarantine_batches')
export const quarantineBatchFiles = (batch: string) =>
  invoke<FileRecord[]>('quarantine_batch_files', { batch })

// ---------- 缩略图缓存 ----------
export const thumbCacheStats = () => invoke<number>('thumb_cache_stats')
export const trimThumbCache = () => invoke<TrimReport>('trim_thumb_cache')
