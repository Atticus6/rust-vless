import { Link } from '@tanstack/react-router'
import { Activity, ArrowRight, Network, Server, Users } from 'lucide-react'
import { useEffect, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { api } from '@/lib/api'
import { formatBytes } from '@/lib/format'
import type { NodeView, Stats } from '@/lib/types'

export function DashboardPage() {
  const [stats, setStats] = useState<Stats | null>(null)
  const [nodes, setNodes] = useState<NodeView[]>([])
  const [error, setError] = useState('')

  useEffect(() => {
    void (async () => {
      try {
        const [s, n] = await Promise.all([
          api<Stats>('/api/stats'),
          api<NodeView[]>('/api/nodes').catch(() => []),
        ])
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
        <p className="text-sm text-muted-foreground">主节点、代理用户与流量状态</p>
      </div>

      {error && <div className="rounded-md border border-destructive/40 bg-destructive/10 p-3 text-sm text-destructive">{error}</div>}

      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {cards.map((card) => (
          <Card key={card.title}>
            <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
              <CardTitle className="text-sm font-medium text-muted-foreground">{card.title}</CardTitle>
              <card.icon className="h-4 w-4 text-primary" />
            </CardHeader>
            <CardContent>
              <div className="text-2xl font-semibold">{card.value}</div>
              <p className="mt-1 text-xs text-muted-foreground">{card.hint}</p>
            </CardContent>
          </Card>
        ))}
      </div>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle className="text-base">最近节点</CardTitle>
          <Link to="/nodes" className="flex items-center gap-1 text-sm text-primary hover:underline">
            管理节点 <ArrowRight className="h-3.5 w-3.5" />
          </Link>
        </CardHeader>
        <CardContent className="space-y-3">
          {nodes.length === 0 && <p className="text-sm text-muted-foreground">还没有节点，去「节点」页面创建。</p>}
          {nodes.map((node, index) => (
            <div key={node.id}>
              <div className="flex items-center justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2">
                  <span className="truncate text-sm font-medium">{node.name}</span>
                  {node.isMaster && <Badge variant="secondary">Master</Badge>}
                  <Badge variant={node.status === 'online' ? 'success' : 'outline'}>
                    {node.status === 'online' ? '在线' : '离线'}
                  </Badge>
                </div>
                <span className="shrink-0 text-xs text-muted-foreground">{node.activeConnections} 连接</span>
              </div>
              {index < nodes.length - 1 && <Separator className="mt-3" />}
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  )
}
