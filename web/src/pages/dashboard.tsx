import { Card, Chip, Separator } from '@heroui/react'
import { Link } from '@tanstack/react-router'
import { Activity, ArrowRight, Network, Server, Users } from 'lucide-react'
import { useI18n } from '@/i18n'
import { formatBytes } from '@/lib/format'
import { client } from '@/lib/trpc'

export function DashboardPage() {
  const { t } = useI18n()
  // tRPC 风格：每个查询独立管理 loading / error，互不阻塞。
  const stats = client.getStats.useQuery()
  const nodes = client.listNodes.useQuery()

  const recent = (nodes.data ?? []).slice(0, 4)
  const error = stats.error ?? null

  const cards = [
    {
      title: t('dashboard.onlineNodes'),
      value: stats.data ? `${stats.data.nodesOnline} / ${stats.data.nodesTotal}` : '—',
      icon: Server,
      hint: `${stats.data?.linksTotal ?? 0} ${t('common.links')}`,
    },
    {
      title: t('dashboard.proxyUsers'),
      value: stats.data ? `${stats.data.usersEnabled} / ${stats.data.usersTotal}` : '—',
      icon: Users,
      hint: t('dashboard.enabledOfTotal'),
    },
    {
      title: t('dashboard.totalTraffic'),
      value: stats.data ? formatBytes(stats.data.upBytes + stats.data.downBytes) : '—',
      icon: Activity,
      hint: stats.data
        ? `↑ ${formatBytes(stats.data.upBytes)} · ↓ ${formatBytes(stats.data.downBytes)}`
        : '',
    },
    {
      title: t('dashboard.activeConnections'),
      value: stats.data ? String(stats.data.activeConnections) : '—',
      icon: Network,
      hint: t('dashboard.vlessSessions'),
    },
  ]

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">{t('nav.overview')}</h1>
        <p className="text-sm text-muted">{t('dashboard.subtitle')}</p>
      </div>

      {error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {error instanceof Error ? error.message : t('common.loadFailed')}
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {cards.map((card) => (
          <Card key={card.title}>
            <Card.Header className="flex flex-row items-center justify-between space-y-0 pb-2">
              <Card.Title className="text-sm font-medium text-muted">{card.title}</Card.Title>
              <card.icon className="h-4 w-4 text-accent" />
            </Card.Header>
            <Card.Content>
              <div className="text-2xl font-semibold">{card.value}</div>
              <p className="mt-1 text-xs text-muted">{card.hint}</p>
            </Card.Content>
          </Card>
        ))}
      </div>

      <Card>
        <Card.Header className="flex flex-row items-center justify-between">
          <Card.Title className="text-base">{t('dashboard.recentNodes')}</Card.Title>
          <Link to="/nodes" className="flex items-center gap-1 text-sm text-accent hover:underline">
            {t('dashboard.manageNodes')} <ArrowRight className="h-3.5 w-3.5" />
          </Link>
        </Card.Header>
        <Card.Content className="space-y-3">
          {recent.length === 0 && <p className="text-sm text-muted">{t('dashboard.noNodes')}</p>}
          {recent.map((node, index) => (
            <div key={node.id}>
              <div className="flex items-center justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2">
                  <span className="truncate text-sm font-medium">{node.name}</span>
                  {node.isMaster && (
                    <Chip size="sm" variant="soft" color="accent">
                      Master
                    </Chip>
                  )}
                  <Chip
                    size="sm"
                    variant="soft"
                    color={node.status === 'online' ? 'success' : 'default'}
                  >
                    {node.status === 'online' ? t('common.online') : t('common.offline')}
                  </Chip>
                </div>
                <span className="shrink-0 text-xs text-muted">
                  {node.activeConnections} {t('common.connections')}
                </span>
              </div>
              {index < recent.length - 1 && <Separator className="mt-3" />}
            </div>
          ))}
        </Card.Content>
      </Card>
    </div>
  )
}
