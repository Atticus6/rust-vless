import { createRootRoute, createRoute, createRouter, Outlet, redirect } from '@tanstack/react-router'
import { AppLayout } from '@/components/layout'
import { getToken } from '@/lib/auth'
import { ConnectionsPage } from '@/pages/connections'
import { DashboardPage } from '@/pages/dashboard'
import { LoginPage } from '@/pages/login'
import { NodesPage } from '@/pages/nodes'
import { TeamPage } from '@/pages/team'
import { TrafficPage } from '@/pages/traffic'
import { UsersPage } from '@/pages/users'

const rootRoute = createRootRoute({
  component: () => <Outlet />,
  notFoundComponent: () => (
    <div className="grid min-h-screen place-items-center text-sm text-muted-foreground">页面不存在</div>
  ),
})

const loginRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/login',
  component: LoginPage,
  beforeLoad: () => {
    if (getToken()) throw redirect({ to: '/' })
  },
})

const appRoute = createRoute({
  getParentRoute: () => rootRoute,
  id: 'app',
  component: AppLayout,
  beforeLoad: () => {
    if (!getToken()) throw redirect({ to: '/login' })
  },
})

const indexRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/',
  component: DashboardPage,
})

const nodesRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/nodes',
  component: NodesPage,
})

const connectionsRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/connections',
  component: ConnectionsPage,
})

const usersRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/users',
  component: UsersPage,
})

const teamRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/team',
  component: TeamPage,
})

const trafficRoute = createRoute({
  getParentRoute: () => appRoute,
  path: '/traffic',
  component: TrafficPage,
})

const routeTree = rootRoute.addChildren([
  loginRoute,
  appRoute.addChildren([
    indexRoute,
    nodesRoute,
    connectionsRoute,
    usersRoute,
    teamRoute,
    trafficRoute,
  ]),
])

export const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
})

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
