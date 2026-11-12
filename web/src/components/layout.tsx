import { Button, Label, ListBox, Select, cn } from '@heroui/react'
import { Link, Outlet, useNavigate, useRouterState } from '@tanstack/react-router'
import {
  Activity,
  LayoutDashboard,
  LogOut,
  Network,
  PackageCheck,
  Server,
  UserRound,
  Users,
  UsersRound,
} from 'lucide-react'
import { Toaster } from 'sonner'
import { clearAuth, getAdmin } from '@/lib/auth'
import { client } from '@/lib/trpc'
import { useI18n, type Lang, type MessageKey } from '@/i18n'

const nav = [
  { to: '/', label: 'nav.overview', icon: LayoutDashboard, exact: true, adminOnly: false },
  { to: '/me', label: 'nav.me', icon: UserRound, exact: false, adminOnly: false },
  { to: '/nodes', label: 'nav.nodes', icon: Server, exact: false, adminOnly: true },
  { to: '/connections', label: 'nav.connections', icon: Network, exact: false, adminOnly: true },
  { to: '/users', label: 'nav.users', icon: Users, exact: false, adminOnly: false },
  { to: '/team', label: 'nav.team', icon: UsersRound, exact: false, adminOnly: true },
  { to: '/versions', label: 'nav.versions', icon: PackageCheck, exact: false, adminOnly: true },
  { to: '/traffic', label: 'nav.traffic', icon: Activity, exact: false, adminOnly: false },
] satisfies Array<{
  to: string
  label: MessageKey
  icon: typeof LayoutDashboard
  exact: boolean
  adminOnly: boolean
}>

export function AppLayout() {
  const admin = getAdmin()
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const { t, lang, setLang } = useI18n()

  async function signOut() {
    try {
      await client.logout({})
    } catch {
      // 服务端不可达时仍然清空本地会话。
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
                {t(item.label)}
              </Link>
            )
          })}
        </nav>

        <div className="mt-auto space-y-3 border-t border-border pt-4">
          <div className="px-2 text-xs text-muted">
            <div className="truncate">{admin?.email ?? t('nav.notSignedIn')}</div>
            <div className="mt-0.5 uppercase tracking-wide">{admin?.role ?? ''}</div>
          </div>
          <Select
            aria-label={t('common.language')}
            selectedKey={lang}
            onSelectionChange={(key) => setLang(key as Lang)}
          >
            <Label className="text-xs">{t('common.language')}</Label>
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                <ListBox.Item id="zh" textValue={t('common.chinese')}>
                  {t('common.chinese')}
                  <ListBox.ItemIndicator />
                </ListBox.Item>
                <ListBox.Item id="en" textValue="English">
                  English
                  <ListBox.ItemIndicator />
                </ListBox.Item>
              </ListBox>
            </Select.Popover>
          </Select>
          <Button
            variant="ghost"
            className="w-full justify-start gap-2"
            onPress={() => void signOut()}
          >
            <LogOut className="h-4 w-4" />
            {t('nav.signOut')}
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
