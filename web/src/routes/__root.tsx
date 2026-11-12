import { Link, createRootRoute, Outlet } from '@tanstack/react-router'
import { useI18n } from '@/i18n'

/** 404 页面单独成组件，以便使用国际化文案。 */
function NotFound() {
  const { t } = useI18n()
  return (
    <div className="grid min-h-screen place-items-center text-sm text-muted">
      <div className="space-y-3 text-center">
        <p>{t('common.notFound')}</p>
        <Link to="/" className="inline-block text-accent hover:underline">
          {t('common.backHome')}
        </Link>
      </div>
    </div>
  )
}

/**
 * 根路由：只负责渲染出口与 404 页面。
 * 登录态判断放在 `_app` 布局与 `/login` 路由中，避免根路由承担业务逻辑。
 */
export const Route = createRootRoute({
  component: () => <Outlet />,
  notFoundComponent: NotFound,
})
