export interface FileRecord {
  id: number
  path: string
  root: string
  size: number
  mtime: number
  ext: string | null
  format: string | null
  width: number | null
  height: number | null
  shortSide: number | null
  pid: number | null
  artist: string | null
  contentHash: string | null
  phash: string | null
  grayScore: number | null
  decodeError: string | null
  status: string
}

export interface Filter {
  minShortSide: number | null
  maxShortSide: number | null
  minSize: number | null
  maxSize: number | null
  exts: string[]
  onlyGray: boolean
  onlyDuplicated: boolean
  onlyDecodeError: boolean
  search: string | null
  sort: string
  limit: number
  offset: number
}

export interface ScanProgress {
  seen: number
  totalHint: number
  current: string
}

export interface ScanStats {
  seen: number
  inserted: number
  updated: number
  skipped: number
  failed: number
}

export interface Settings {
  threads: number
  similarThreshold: number
  grayThreshold: number
  thumbMaxEdge: number
  thumbCacheLimitMb: number
  quarantineDir: string
  roots: string[]
}

/** 重复组的界面视图：保留项 + 其余成员 + 各成员到基准的距离 */
export interface GroupView {
  groupId: number
  kind: string
  keep: FileRecord
  members: FileRecord[]
  distances: number[]
}

/** 隔离区里的一批文件 */
export interface QuarantineBatch {
  batchId: string
  count: number
  bytes: number
  movedAt: number
}

export interface RebuildResult {
  exact: number
  similar: number
}

export interface MoveReport {
  moved: number
  failed: number
  bytes: number
}

export interface PurgeReport {
  purged: number
  bytes: number
}
