import { createFileRoute, redirect } from '@tanstack/react-router'
import { getToken } from '@/lib/auth'
import { requireInstalled } from '@/lib/guards'
import { LoginPage } from '@/pages/login'

/**
 * 登录页：未安装时先去安装；已登录用户直接回到总览。
 */
export const Route = createFileRoute('/login')({
  beforeLoad: async () => {
    await requireInstalled()
    if (getToken()) throw redirect({ to: '/' })
  },
  component: LoginPage,
})
