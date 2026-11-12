import { createFileRoute, isRedirect, redirect } from '@tanstack/react-router'
import { getToken } from '@/lib/auth'
import { client } from '@/lib/connect'
import { SetupPage } from '@/pages/setup'

/**
 * 首次安装页：已安装时不再允许访问，直接回到登录页或总览。
 */
export const Route = createFileRoute('/setup')({
  beforeLoad: async () => {
    try {
      const status = await client.getSetupStatus({})
      if (status.installed) {
        throw redirect({ to: getToken() ? '/' : '/login' })
      }
    } catch (error) {
      if (isRedirect(error)) throw error
    }
  },
  component: SetupPage,
})
