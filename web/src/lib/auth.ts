import type { AdminView } from './types'

/**
 * 会话存储。
 *
 * 所有函数都接受可选的 `storage`，默认使用 `localStorage`：
 * - 单元测试可以注入内存实现，不依赖 DOM；
 * - 浏览器禁用站点存储时 `localStorage` 访问可能抛错，这里统一兜底，
 *   保证调用方（路由守卫、RPC 拦截器）不会因为存储异常而崩溃。
 */
/** 登录账号；与 RPC 返回的 `AdminView` 是同一形状，复用避免重复定义。 */
export type Admin = AdminView

const TOKEN_KEY = 'rv_token'
const ADMIN_KEY = 'rv_admin'

/** 会话存储接口；只依赖读写删除，便于测试注入。 */
export type AuthStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>

/** 安全获取 localStorage；不可用时返回 `null`。 */
function defaultStorage(): AuthStorage | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}

function resolve(storage?: AuthStorage | null): AuthStorage | null {
  return storage === undefined ? defaultStorage() : storage
}

export function getToken(storage?: AuthStorage | null): string | null {
  try {
    return resolve(storage)?.getItem(TOKEN_KEY) ?? null
  } catch {
    return null
  }
}

export function getAdmin(storage?: AuthStorage | null): Admin | null {
  let raw: string | null = null
  try {
    raw = resolve(storage)?.getItem(ADMIN_KEY) ?? null
  } catch {
    return null
  }
  if (!raw) return null
  try {
    return JSON.parse(raw) as Admin
  } catch {
    return null
  }
}

export function setAuth(token: string, admin: Admin, storage?: AuthStorage | null): void {
  try {
    const target = resolve(storage)
    target?.setItem(TOKEN_KEY, token)
    target?.setItem(ADMIN_KEY, JSON.stringify(admin))
  } catch {
    // 存储不可用时仍允许本次会话在内存中工作。
  }
}

export function clearAuth(storage?: AuthStorage | null): void {
  try {
    const target = resolve(storage)
    target?.removeItem(TOKEN_KEY)
    target?.removeItem(ADMIN_KEY)
  } catch {
    // 忽略清理失败。
  }
}
