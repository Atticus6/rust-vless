import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createRouter, RouterProvider } from '@tanstack/react-router'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import '@xyflow/react/dist/style.css'
import './index.css'
import { I18nProvider } from './i18n'
import { routeTree } from './routeTree.gen'

/**
 * 路由实例由 `routes/` 目录自动生成，新增页面只需添加文件，
 * 不再维护手写的路由树。
 */
const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}

/**
 * RPC 查询缓存。
 *
 * 与 tRPC 默认行为对齐：失败重试 1 次、窗口聚焦不自动重取（管理台数据由
 * mutation 主动失效），避免频繁请求主节点。
 */
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
      staleTime: 5_000,
    },
  },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <I18nProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </I18nProvider>
  </StrictMode>,
)
