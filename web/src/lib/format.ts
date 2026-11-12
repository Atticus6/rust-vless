/** 把字节数格式化为易读单位；非有限值或负数按 0 处理。 */
export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let n = value
  let i = 0
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024
    i += 1
  }
  return `${n.toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

/** 按指定 locale 格式化绝对时间；0 表示未知。 */
export function formatTime(ms: number, locale = 'zh-CN'): string {
  if (!ms) return '—'
  return new Date(ms).toLocaleString(locale)
}

/**
 * 按指定 locale 格式化相对时间。
 *
 * 一分钟内与“从未”使用本地化文案，其余交给 `Intl.RelativeTimeFormat`；
 * 超过一天显示绝对时间，避免“3 天前”这类信息在日志里失去精度。
 */
export function formatRelative(ms: number, locale = 'zh-CN'): string {
  const zh = locale.toLowerCase().startsWith('zh')
  if (!ms) return zh ? '从未' : 'Never'
  const diff = Date.now() - ms
  if (diff < 60_000) return zh ? '刚刚' : 'just now'
  const relative = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  if (diff < 3_600_000) return relative.format(-Math.floor(diff / 60_000), 'minute')
  if (diff < 86_400_000) return relative.format(-Math.floor(diff / 3_600_000), 'hour')
  return new Date(ms).toLocaleString(locale)
}

/** 长 ID 只保留前 8 位，便于表格展示。 */
export function shortId(id: string): string {
  return id.length > 10 ? `${id.slice(0, 8)}…` : id
}
