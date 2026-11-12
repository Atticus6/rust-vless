/**
 * 会话存储纯函数测试。
 *
 * 使用内存实现代替 localStorage：覆盖读写、登出清理、损坏 JSON 容错，
 * 以及存储本身抛错时不得向上传播。
 */
import { describe, expect, it } from 'bun:test'
import { clearAuth, getAdmin, getToken, setAuth, type AuthStorage } from './auth'

const admin = { id: 'admin-1', email: 'admin@example.com', role: 'admin' as const, createdAt: 1 }

/** 内存版存储。 */
function memoryStorage(): AuthStorage & { dump(): Record<string, string> } {
  const data = new Map<string, string>()
  return {
    getItem: (key) => data.get(key) ?? null,
    setItem: (key, value) => {
      data.set(key, value)
    },
    removeItem: (key) => {
      data.delete(key)
    },
    dump: () => Object.fromEntries(data),
  }
}

/** 任何访问都抛错的存储（模拟禁用站点存储）。 */
const throwing: AuthStorage = {
  getItem: () => {
    throw new Error('SecurityError')
  },
  setItem: () => {
    throw new Error('SecurityError')
  },
  removeItem: () => {
    throw new Error('SecurityError')
  },
}

describe('auth storage', () => {
  it('round-trips the session token and account', () => {
    const storage = memoryStorage()
    setAuth('token-123', admin, storage)

    expect(getToken(storage)).toBe('token-123')
    expect(getAdmin(storage)).toEqual(admin)
  })

  it('clears both entries on logout', () => {
    const storage = memoryStorage()
    setAuth('token-123', admin, storage)
    clearAuth(storage)

    expect(getToken(storage)).toBeNull()
    expect(getAdmin(storage)).toBeNull()
    expect(storage.dump()).toEqual({})
  })

  it('ignores corrupted account JSON instead of throwing', () => {
    const storage = memoryStorage()
    storage.setItem('rv_token', 'token-123')
    storage.setItem('rv_admin', '{not json')

    expect(getToken(storage)).toBe('token-123')
    expect(getAdmin(storage)).toBeNull()
  })

  it('returns null and does not throw when storage is unavailable', () => {
    expect(getToken(null)).toBeNull()
    expect(getAdmin(null)).toBeNull()
    expect(() => setAuth('token', admin, null)).not.toThrow()
    expect(() => clearAuth(null)).not.toThrow()
  })

  it('swallows storage errors', () => {
    expect(getToken(throwing)).toBeNull()
    expect(getAdmin(throwing)).toBeNull()
    expect(() => setAuth('token', admin, throwing)).not.toThrow()
    expect(() => clearAuth(throwing)).not.toThrow()
  })
})
