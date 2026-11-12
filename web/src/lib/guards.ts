/**
 * 统一的鉴权与安装状态守卫。
 *
 * 所有「会话过期怎么办」的逻辑只在这里：`connect.ts` 与页面只抛
 * `RpcError`，是否跳转由路由决定。这样登录页、安装页与业务页各自
 * 有明确的入口条件，不会出现某个调用点偷偷跳转、另一个静默失败。
 */
import { isRedirect, redirect } from '@tanstack/react-router'
import { Code } from '@connectrpc/connect'
import { clearAuth, getToken } from './auth'
import { RpcError, client } from './connect'

/**
 * 确保主节点已完成首次安装。
 *
 * 未安装时跳转 `/setup`。网络错误时放行，让页面自己展示连接失败，
 * 避免主节点短暂不可用时整个前端无法渲染。
 */
export async function requireInstalled(): Promise<void> {
  try {
    const status = await client.getSetupStatus({})
    if (!status.installed) throw redirect({ to: '/setup' })
  } catch (error) {
    if (isRedirect(error)) throw error
  }
}

/**
 * 确保已登录；未登录直接跳登录页。
 *
 * 只检查本地 token 是否存在——token 有效性由首个 RPC 调用验证，
 * 过期时由 [`guardAuthError`] 统一处理。
 */
export function requireSession(): void {
  if (!getToken()) throw redirect({ to: '/login' })
}

/**
 * 把「会话过期」转换成路由跳转。
 *
 * 业务页的 `beforeLoad` / `loader` 捕获到 `RpcError(Unauthenticated)` 时调用：
 * 清理本地会话并跳转登录页。其他错误原样抛出，由页面展示。
 */
export function guardAuthError(error: unknown): never {
  if (error instanceof RpcError && error.code === Code.Unauthenticated) {
    clearAuth()
    throw redirect({ to: '/login' })
  }
  throw error
}

/**
 * 包装一次需要登录的异步操作：会话过期时跳登录，其余错误交给调用方。
 *
 * 用于路由 `loader` 里需要先取数据的场景。
 */
export async function withSession<T>(operation: () => Promise<T>): Promise<T> {
  requireSession()
  try {
    return await operation()
  } catch (error) {
    return guardAuthError(error)
  }
}
