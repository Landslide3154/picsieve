import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  FileRecord,
  Filter,
  GroupView,
  RebuildResult,
  ScanProgress,
  ScanStats,
  Settings,
} from './types'

export const getSettings = () => invoke<Settings>('get_settings')
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings })
export const startScan = () => invoke<ScanStats>('start_scan')
export const cancelScan = () => invoke<void>('cancel_scan')
export const startFingerprint = () => invoke<unknown>('start_fingerprint')

export const queryFiles = (filter: Filter) => invoke<FileRecord[]>('query_files_cmd', { filter })
export const countFiles = (filter: Filter) => invoke<number>('count_files_cmd', { filter })

export const listDupGroups = (kind: string, offset: number, limit: number) =>
  invoke<GroupView[]>('list_dup_groups', { kind, offset, limit })
export const setKeeper = (groupId: number, fileId: number) =>
  invoke<void>('set_keeper', { groupId, fileId })
export const rebuildGroups = () => invoke<RebuildResult>('rebuild_groups')

/** 取缩略图并转成可直接放进 <img src> 的 Blob URL。调用方负责在不用时 revoke。 */
export async function fetchThumbUrl(fileId: number): Promise<string> {
  const raw = await invoke<ArrayBuffer | number[]>('get_thumb', { fileId })
  const bytes = raw instanceof ArrayBuffer ? new Uint8Array(raw) : new Uint8Array(raw)
  return URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }))
}

export function onScanProgress(cb: (p: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>('scan://progress', (e) => cb(e.payload))
}

export async function pickFolder(): Promise<string | null> {
  const picked = await open({ directory: true, multiple: false, title: '选择要扫描的文件夹' })
  return typeof picked === 'string' ? picked : null
}
