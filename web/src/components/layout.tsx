import { Button, cn } from '@heroui/react'
import { Link, Outlet, useNavigate, useRouterState } from '@tanstack/react-router'
import {
  Activity,
  LayoutDashboard,
  LogOut,
  Network,
  Server,
  Users,
  UsersRound,
} from 'lucide-react'
import { Toaster } from 'sonner'
import { clearAuth, getAdmin } from '@/lib/auth'
import { logout } from '@/lib/rpc'

const nav = [
  { to: '/', label: '总览', icon: LayoutDashboard, exact: true, adminOnly: false },
  { to: '/nodes', label: '节点', icon: Server, exact: false, adminOnly: true },
  { to: '/connections', label: '连接', icon: Network, exact: false, adminOnly: true },
  { to: '/users', label: '代理用户', icon: Users, exact: false, adminOnly: false },
  { to: '/team', label: '团队', icon: UsersRound, exact: false, adminOnly: true },
  { to: '/traffic', label: '流量', icon: Activity, exact: false, adminOnly: false },
]

export function AppLayout() {
  const admin = getAdmin()
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })

  async function signOut() {
    try {
      await logout()
    } catch {
      // The local session is cleared even if the server is unreachable.
    }
    clearAuth()
    void navigate({ to: '/login' })
  }

  return (
    <div className="flex min-h-screen">
      <aside className="hidden w-56 shrink-0 flex-col border-r border-border bg-surface p-4 md:flex">
        <Link to="/" className="mb-8 flex items-center gap-2 px-2">
          <div className="grid h-9 w-9 place-items-center rounded-lg bg-accent text-accent-foreground">
            <Network className="h-5 w-5" />
          </div>
          <div>
            <div className="text-sm font-semibold">rust-vless</div>
            <div className="text-[11px] text-muted">control plane</div>
          </div>
        </Link>

        <nav className="space-y-1">
          {nav.filter((item) => !item.adminOnly || admin?.role === 'admin').map((item) => {
            const active = item.exact ? pathname === item.to : pathname.startsWith(item.to)
            return (
              <Link
                key={item.to}
                to={item.to}
                className={cn(
                  'flex items-center gap-2 rounded-md px-3 py-2 text-sm text-muted transition-colors hover:bg-surface-secondary hover:text-foreground',
                  active && 'bg-accent/10 text-accent hover:bg-accent/10 hover:text-accent',
                )}
              >
                <item.icon className="h-4 w-4" />
                {item.label}
              </Link>
            )
          })}
        </nav>

        <div className="mt-auto space-y-3 border-t border-border pt-4">
          <div className="px-2 text-xs text-muted">
            <div className="truncate">{admin?.email ?? '未登录'}</div>
            <div className="mt-0.5 uppercase tracking-wide">{admin?.role ?? ''}</div>
          </div>
          <Button
            variant="ghost"
            className="w-full justify-start gap-2"
            onPress={() => void signOut()}
          >
            <LogOut className="h-4 w-4" />
            退出登录
          </Button>
        </div>
      </aside>

      <main className="min-w-0 flex-1 p-4 md:p-8">
        <div className="mx-auto max-w-6xl">
          <Outlet />
        </div>
      </main>
      <Toaster richColors position="top-right" />
    </div>
  )
}
