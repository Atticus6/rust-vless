/**
 * 路由守卫纯函数测试。
 *
 * 守卫是唯一的鉴权跳转点：会话过期必须清理本地状态并抛出重定向，
 * 其他错误原样抛出，未登录时直接跳登录页。
 */
import { describe, expect, it } from 'bun:test'
import { isRedirect } from '@tanstack/react-router'
import { Code } from '@connectrpc/connect'
import { clearAuth, getToken, setAuth } from './auth'
import { RpcError } from './connect'
import { guardAuthError, requireSession, withSession } from './guards'

const admin = { id: 'admin-1', email: 'admin@example.com', role: 'admin' as const, createdAt: 1 }

/** 捕获调用抛出的值，便于断言是重定向还是原始错误。 */
function capture(fn: () => unknown): unknown {
  try {
    fn()
    return undefined
  } catch (error) {
    return error
  }
}

describe('requireSession', () => {
  it('throws a redirect when there is no token', () => {
    clearAuth()
    const thrown = capture(() => requireSession())
    expect(isRedirect(thrown)).toBe(true)
  })

  it('passes when a token exists', () => {
    setAuth('token-123', admin)
    expect(capture(() => requireSession())).toBeUndefined()
  })
})

describe('guardAuthError', () => {
  it('clears the session and redirects on Unauthenticated', () => {
    setAuth('expired', admin)
    const thrown = capture(() =>
      guardAuthError(new RpcError('unauthorized', Code.Unauthenticated)),
    )
    expect(isRedirect(thrown)).toBe(true)
    expect(getToken()).toBeNull()
  })

  it('rethrows other RPC errors unchanged', () => {
    setAuth('valid', admin)
    const error = new RpcError('forbidden', Code.PermissionDenied)
    expect(capture(() => guardAuthError(error))).toBe(error)
    // 非鉴权错误不能清理会话。
    expect(getToken()).toBe('valid')
  })

  it('rethrows non-RPC errors unchanged', () => {
    const error = new Error('boom')
    expect(capture(() => guardAuthError(error))).toBe(error)
  })
})

describe('withSession', () => {
  it('returns the operation result when the session is valid', async () => {
    setAuth('token-123', admin)
    await expect(withSession(async () => 'ok')).resolves.toBe('ok')
  })

  it('redirects before running the operation when unauthenticated', async () => {
    clearAuth()
    let ran = false
    const thrown = await withSession(async () => {
      ran = true
      return 'ok'
    }).catch((error: unknown) => error)
    expect(ran).toBe(false)
    expect(isRedirect(thrown)).toBe(true)
  })

  it('redirects when the operation reports an expired session', async () => {
    setAuth('expired', admin)
    const thrown = await withSession(async () => {
      throw new RpcError('unauthorized', Code.Unauthenticated)
    }).catch((error: unknown) => error)
    expect(isRedirect(thrown)).toBe(true)
    expect(getToken()).toBeNull()
  })
})
