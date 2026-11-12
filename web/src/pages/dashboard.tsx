import { Card, Chip, Separator } from '@heroui/react'
import { Link } from '@tanstack/react-router'
import { Activity, ArrowRight, Network, Server, Users } from 'lucide-react'
import { useEffect, useState } from 'react'
import { formatBytes } from '@/lib/format'
import { listNodes, stats as fetchStats } from '@/lib/rpc'
import type { NodeView, Stats } from '@/lib/types'

export function DashboardPage() {
  const [stats, setStats] = useState<Stats | null>(null)
  const [nodes, setNodes] = useState<NodeView[]>([])
  const [error, setError] = useState('')

  useEffect(() => {
    void (async () => {
      try {
        const [s, n] = await Promise.all([fetchStats(), listNodes().catch(() => [])])
        setStats(s)
        setNodes(n.slice(0, 4))
      } catch (err) {
        setError(err instanceof Error ? err.message : '加载失败')
      }
    })()
  }, [])

  const cards = [
    {
      title: '在线节点',
      value: stats ? `${stats.nodesOnline} / ${stats.nodesTotal}` : '—',
      icon: Server,
      hint: `${stats?.linksTotal ?? 0} 条连线`,
    },
    {
      title: '代理用户',
      value: stats ? `${stats.usersEnabled} / ${stats.usersTotal}` : '—',
      icon: Users,
      hint: '启用 / 总数',
    },
    {
      title: '累计流量',
      value: stats ? formatBytes(stats.upBytes + stats.downBytes) : '—',
      icon: Activity,
      hint: stats ? `↑ ${formatBytes(stats.upBytes)} · ↓ ${formatBytes(stats.downBytes)}` : '',
    },
    {
      title: '活跃连接',
      value: stats ? String(stats.activeConnections) : '—',
      icon: Network,
      hint: 'VLESS 会话',
    },
  ]

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">总览</h1>
        <p className="text-sm text-muted">主节点、代理用户与流量状态</p>
      </div>

      {error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {error}
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
          <Card.Title className="text-base">最近节点</Card.Title>
          <Link to="/nodes" className="flex items-center gap-1 text-sm text-accent hover:underline">
            管理节点 <ArrowRight className="h-3.5 w-3.5" />
          </Link>
        </Card.Header>
        <Card.Content className="space-y-3">
          {nodes.length === 0 && <p className="text-sm text-muted">还没有节点，去「节点」页面创建。</p>}
          {nodes.map((node, index) => (
            <div key={node.id}>
              <div className="flex items-center justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2">
                  <span className="truncate text-sm font-medium">{node.name}</span>
                  {node.isMaster && (
                    <Chip size="sm" variant="soft" color="accent">
                      Master
                    </Chip>
                  )}
                  <Chip size="sm" variant="soft" color={node.status === 'online' ? 'success' : 'default'}>
                    {node.status === 'online' ? '在线' : '离线'}
                  </Chip>
                </div>
                <span className="shrink-0 text-xs text-muted">{node.activeConnections} 连接</span>
              </div>
              {index < nodes.length - 1 && <Separator className="mt-3" />}
            </div>
          ))}
        </Card.Content>
      </Card>
    </div>
  )
}
