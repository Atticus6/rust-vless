/**
 * 会话存储单测。
 *
 * 覆盖 token/账号的读写、登出清理，以及 localStorage 中出现损坏 JSON 时的
 * 容错行为；这些值决定页面是否跳转登录。
 */
import { beforeEach, describe, expect, it } from 'vitest'
import { clearAuth, getAdmin, getToken, setAuth } from './auth'

const admin = { id: 'admin-1', email: 'admin@example.com', role: 'admin' as const, createdAt: 1 }

describe('auth storage', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('round-trips the session token and account', () => {
    setAuth('token-123', admin)

    expect(getToken()).toBe('token-123')
    expect(getAdmin()).toEqual(admin)
  })

  it('clears both entries on logout', () => {
    setAuth('token-123', admin)
    clearAuth()

    expect(getToken()).toBeNull()
    expect(getAdmin()).toBeNull()
  })

  it('ignores corrupted account JSON instead of throwing', () => {
    localStorage.setItem('rv_token', 'token-123')
    localStorage.setItem('rv_admin', '{not json')

    expect(getToken()).toBe('token-123')
    expect(getAdmin()).toBeNull()
  })
})
