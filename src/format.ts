/** 统一的数字格式化。
 *
 * 注意别用 `1 << 30` / `1 << 40` 这类位移：JS 的位运算只有 32 位，
 * `1 << 40` 实际等于 256，之前顶栏就是这么把 176 GB 显示成 742160061.80 TB 的。
 */

const KB = 1024
const MB = 1024 ** 2
const GB = 1024 ** 3
const TB = 1024 ** 4

/** 字节数转人话：1234567 → 1.2 MB */
export function fmtBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined || !Number.isFinite(bytes) || bytes <= 0) return '0 MB'
  if (bytes >= TB) return (bytes / TB).toFixed(2) + ' TB'
  if (bytes >= GB) {
    const gb = bytes / GB
    return (gb >= 100 ? gb.toFixed(0) : gb.toFixed(2)) + ' GB'
  }
  if (bytes >= MB) {
    const mb = bytes / MB
    return (mb >= 100 ? mb.toFixed(0) : mb.toFixed(1)) + ' MB'
  }
  return Math.round(bytes / KB) + ' KB'
}

/** 张数带千分位 */
export function fmtCount(n: number): string {
  return n.toLocaleString('zh-CN')
}

/** 总像素数转人话：1,920,000 → 192 万像素 */
export function fmtPixels(px: number | null | undefined): string {
  if (!px || !Number.isFinite(px) || px <= 0) return '0 像素'
  if (px >= 100_000_000) return (px / 100_000_000).toFixed(2) + ' 亿像素'
  if (px >= 10_000) return fmtCount(Math.round(px / 10_000)) + ' 万像素'
  return fmtCount(px) + ' 像素'
}

/** 秒数转「几分钟」这类说法（预计剩余时间用） */
export function fmtEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return ''
  if (seconds < 1) return '马上好'
  if (seconds < 60) return `还要 ${Math.round(seconds)} 秒`
  if (seconds < 3600) return `还要约 ${Math.round(seconds / 60)} 分钟`
  return `还要约 ${(seconds / 3600).toFixed(1)} 小时`
}

/** 相对时间（用 Intl，别自己拼字符串） */export function fmtRelative(secs: number, suffix = ''): string {
  if (!secs) return '还没有记录'
  const rtf = new Intl.RelativeTimeFormat('zh-CN', { numeric: 'auto' })
  const diffMs = Date.now() - secs * 1000
  const min = Math.round(diffMs / 60000)
  if (Math.abs(min) < 1) return '刚刚' + suffix
  if (Math.abs(min) < 60) return rtf.format(-min, 'minute') + suffix
  const hour = Math.round(min / 60)
  if (Math.abs(hour) < 24) return rtf.format(-hour, 'hour') + suffix
  return rtf.format(-Math.round(hour / 24), 'day') + suffix
}

/** 绝对时间（隔离区批次用） */
export function fmtDateTime(secs: number): string {
  if (!secs) return '—'
  return new Intl.DateTimeFormat('zh-CN', {
    dateStyle: 'short',
    timeStyle: 'short',
    hour12: false,
  }).format(new Date(secs * 1000))
}

/** 只取文件名 */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path
}
