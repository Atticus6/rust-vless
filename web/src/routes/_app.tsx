import { createFileRoute } from '@tanstack/react-router'
import { AppLayout } from '@/components/layout'
import { requireInstalled, requireSession } from '@/lib/guards'

/**
 * 受保护布局：所有业务页面都挂在 `_app` 下。
 *
 * 这里是整个应用的鉴权入口：
 * 1. 没有会话 token → 跳登录页；
 * 2. 主节点未完成安装 → 跳安装页。
 *
 * 业务页里的 RPC 调用不自行跳转；会话在调用过程中过期时，由页面用
 * `guardAuthError`（或 `withSession`）抛出重定向，统一回到登录页。
 */
export const Route = createFileRoute('/_app')({
  beforeLoad: async () => {
    requireSession()
    await requireInstalled()
  },
  component: AppLayout,
})
