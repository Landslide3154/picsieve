/**
 * 界面偏好的本地记忆（页签、筛选、缩略图档位）。
 *
 * 刻意不用 settings.json：
 * 1) 这些是「看起来怎样」的界面状态，不是用户配置，混进去会让设置文件越来越杂；
 * 2) 后端 Settings 是强类型反序列化，少一个字段整份文件就解析失败、配置全丢。
 * localStorage 读坏了最多回到默认值，不影响任何业务数据。
 */
const PREFIX = 'picsieve.'

export function loadPref<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(PREFIX + key)
    if (!raw) return fallback
    const v = JSON.parse(raw) as T
    return v === null || v === undefined ? fallback : v
  } catch {
    return fallback
  }
}

export function savePref(key: string, value: unknown): void {
  try {
    localStorage.setItem(PREFIX + key, JSON.stringify(value))
  } catch {
    /* 存储不可用（隐私模式/配额满）时静默放弃，界面照常能用 */
  }
}
