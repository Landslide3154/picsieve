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
