/**
 * 展示格式化函数单测。
 *
 * 使用固定时钟与边界值锁定字节单位换算、时间戳与相对时间的展示契约，
 * 避免这些纯函数在页面重构时悄悄改变输出。
 */
import { afterEach, describe, expect, it, setSystemTime } from 'bun:test'
import { formatBytes, formatRelative, formatTime, shortId } from './format'

describe('formatBytes', () => {
  it('treats zero, negative and non-finite values as zero', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(-1024)).toBe('0 B')
    expect(formatBytes(Number.NaN)).toBe('0 B')
    expect(formatBytes(Number.POSITIVE_INFINITY)).toBe('0 B')
  })

  it('keeps raw bytes without a decimal point and scales larger units', () => {
    expect(formatBytes(1023)).toBe('1023 B')
    expect(formatBytes(1024)).toBe('1.0 KB')
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(1024 ** 3)).toBe('1.0 GB')
    expect(formatBytes(1024 ** 4)).toBe('1.0 TB')
  })

  it('stops at TB for very large values', () => {
    expect(formatBytes(1024 ** 5)).toBe('1024.0 TB')
  })
})

describe('formatTime', () => {
  it('renders a dash for missing values', () => {
    expect(formatTime(0)).toBe('—')
  })

  it('renders the locale string for real timestamps', () => {
    const value = Date.UTC(2026, 0, 2, 3, 4, 5)
    expect(formatTime(value, 'zh-CN')).toBe(new Date(value).toLocaleString('zh-CN'))
  })
})

describe('formatRelative', () => {
  afterEach(() => {
    setSystemTime()
  })

  it('renders 从未 for a missing timestamp', () => {
    expect(formatRelative(0)).toBe('从未')
  })

  it('buckets recent timestamps', () => {
    const now = Date.UTC(2026, 0, 1, 12, 0, 0)
    setSystemTime(new Date(now))
    const relative = new Intl.RelativeTimeFormat('zh-CN', { numeric: 'auto' })

    expect(formatRelative(now - 30_000)).toBe('刚刚')
    expect(formatRelative(now - 5 * 60_000)).toBe(relative.format(-5, 'minute'))
    expect(formatRelative(now - 2 * 3_600_000)).toBe(relative.format(-2, 'hour'))
    expect(formatRelative(now - 2 * 86_400_000)).toBe(
      new Date(now - 2 * 86_400_000).toLocaleString('zh-CN'),
    )
  })

  it('uses English labels when the locale is English', () => {
    const now = Date.UTC(2026, 0, 1, 12, 0, 0)
    setSystemTime(new Date(now))
    const relative = new Intl.RelativeTimeFormat('en-US', { numeric: 'auto' })

    expect(formatRelative(0, 'en-US')).toBe('Never')
    expect(formatRelative(now - 30_000, 'en-US')).toBe('just now')
    expect(formatRelative(now - 5 * 60_000, 'en-US')).toBe(relative.format(-5, 'minute'))
    expect(formatTime(now, 'en-US')).toBe(new Date(now).toLocaleString('en-US'))
  })
})

describe('shortId', () => {
  it('leaves short ids alone and truncates long ones', () => {
    expect(shortId('abc')).toBe('abc')
    expect(shortId('0123456789abcdef')).toBe('01234567…')
  })
})
