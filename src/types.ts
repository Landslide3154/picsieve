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
  /** 清晰度：总像素数（宽 × 高） */
  minPixels: number | null
  maxPixels: number | null
  minSize: number | null
  maxSize: number | null
  /** 空数组 = 不限格式 */
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
  elapsedMs: number
  perSecond: number
}

export interface ScanStats {
  seen: number
  inserted: number
  updated: number
  skipped: number
  failed: number
  cancelled: boolean
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
  /** 这一组处理掉能省下的字节数 */
  savings: number
  /** 为什么建议留这张 */
  keepReason: string
}

/** 隔离区里的一批文件 */
export interface QuarantineBatch {
  batchId: string
  count: number
  bytes: number
  movedAt: number
}

export interface LibraryStats {
  total: number
  bytes: number
  lastScanAt: number
}

export interface Histogram {
  /** 档位边界：buckets[i] 对应 [edges[i], edges[i+1])，最后一档上不封顶 */
  edges: number[]
  buckets: number[]
  max: number
}

/** 库里实际存在的格式与张数 */
export interface FormatStat {
  ext: string
  count: number
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

export interface TrimReport {
  removed: number
  freed: number
  remaining: number
}

export interface FingerprintResult {
  content: { hashed: number; failed: number; cancelled: boolean }
  visual: { done: number; failed: number; skipped: number; cancelled: boolean }
}
