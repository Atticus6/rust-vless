import {
  Background,
  Controls,
  MarkerType,
  ReactFlow,
  type Edge,
  type Node,
} from '@xyflow/react'
import { Filter, RefreshCw } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Separator } from '@/components/ui/separator'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'
import { formatBytes, formatRelative, formatTime } from '@/lib/format'
import type {
  AccessLogPage,
  AccessLogView,
  ProxyUserView,
  TrafficTopology,
} from '@/lib/types'

const WINDOWS = [
  { label: '1 小时', value: 3600 },
  { label: '24 小时', value: 86400 },
  { label: '7 天', value: 604800 },
]

const nodeStyle = {
  background: 'hsl(var(--card))',
  color: 'hsl(var(--foreground))',
  border: '1px solid hsl(var(--border))',
  borderRadius: 10,
  width: 180,
  fontSize: 12,
  padding: 4,
} satisfies React.CSSProperties

export function TrafficPage() {
  const [windowSec, setWindowSec] = useState(3600)
  const [topology, setTopology] = useState<TrafficTopology | null>(null)
  const [logs, setLogs] = useState<AccessLogPage | null>(null)
  const [users, setUsers] = useState<ProxyUserView[]>([])
  const [selected, setSelected] = useState<AccessLogView | null>(null)
  const [loading, setLoading] = useState(false)
  const [refresh, setRefresh] = useState(0)

  const [userId, setUserId] = useState('')
  const [nodeId, setNodeId] = useState('')
  const [status, setStatus] = useState('')
  const [targetInput, setTargetInput] = useState('')
  const [target, setTarget] = useState('')
  const [offset, setOffset] = useState(0)
  const limit = 25

  const loadTopology = useCallback(async () => {
    try {
      setTopology(await api<TrafficTopology>(`/api/traffic/topology?window=${windowSec}`))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载拓扑失败')
    }
  }, [windowSec])

  const loadLogs = useCallback(async () => {
    setLoading(true)
    try {
      const query = new URLSearchParams()
      if (userId) query.set('userId', userId)
      if (nodeId) query.set('nodeId', nodeId)
      if (status) query.set('status', status)
      if (target) query.set('target', target)
      query.set('limit', String(limit))
      query.set('offset', String(offset))
      setLogs(await api<AccessLogPage>(`/api/access-logs?${query.toString()}`))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载访问记录失败')
    } finally {
      setLoading(false)
    }
  }, [userId, nodeId, status, target, offset])

  useEffect(() => {
    void loadTopology()
  }, [loadTopology])

  useEffect(() => {
    void loadLogs()
  }, [loadLogs, refresh])

  useEffect(() => {
    void api<ProxyUserView[]>('/api/users')
      .then(setUsers)
      .catch(() => setUsers([]))
  }, [])

  const nodes = useMemo<Node[]>(
    () =>
      (topology?.nodes ?? []).map((node, index) => ({
        id: node.id,
        position: { x: (index % 3) * 290, y: Math.floor(index / 3) * 170 },
        data: {
          label: `${node.name}${node.isMaster && node.name.toLowerCase() !== 'master' ? ' · master' : ''}${node.online ? '' : ' · offline'}`, 
        },
        style: {
          ...nodeStyle,
          opacity: node.online ? 1 : 0.55,
        },
      })),
    [topology?.nodes],
  )

  const edges = useMemo<Edge[]>(
    () =>
      (topology?.links ?? []).map((link) => {
        const total = link.upBytes + link.downBytes
        return {
          id: link.id,
          source: link.fromNodeId,
          target: link.toNodeId,
          label: `${formatBytes(total)} · ${link.sessions} 会话`,
          markerEnd: { type: MarkerType.ArrowClosed },
          animated: link.sessions > 0,
          style: {
            stroke: 'hsl(160 84% 39%)',
            strokeWidth: Math.min(6, 1 + Math.log10(total + 1)),
          },
          labelStyle: { fill: 'hsl(var(--muted-foreground))', fontSize: 11 },
        } satisfies Edge
      }),
    [topology?.links],
  )

  function nodeName(id: string): string {
    return topology?.nodes.find((node) => node.id === id)?.name ?? id.slice(0, 8)
  }

  function applyFilters() {
    setOffset(0)
    setTarget(targetInput.trim())
    setRefresh((value) => value + 1)
  }

  const totalBytes =
    topology?.links.reduce((sum, link) => sum + link.upBytes + link.downBytes, 0) ?? 0
  const totalSessions = topology?.links.reduce((sum, link) => sum + link.sessions, 0) ?? 0

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">流量</h1>
          <p className="text-sm text-muted-foreground">节点间流量拓扑与代理用户访问记录</p>
        </div>
        <div className="flex items-center gap-2">
          <Select
            className="w-32"
            value={String(windowSec)}
            onChange={(event) => setWindowSec(Number(event.target.value))}
          >
            {WINDOWS.map((item) => (
              <option key={item.value} value={item.value}>{item.label}</option>
            ))}
          </Select>
          <Button variant="outline" className="gap-1" onClick={() => { void loadTopology(); setRefresh((value) => value + 1) }}>
            <RefreshCw className="h-4 w-4" /> 刷新
          </Button>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">窗口流量</CardTitle>
          </CardHeader>
          <CardContent><div className="text-2xl font-semibold">{formatBytes(totalBytes)}</div></CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">中转会话</CardTitle>
          </CardHeader>
          <CardContent><div className="text-2xl font-semibold">{totalSessions}</div></CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">拓扑节点</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-semibold">{topology?.nodes.length ?? 0}</div>
            <p className="text-xs text-muted-foreground">{topology?.links.length ?? 0} 条连线</p>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">流量拓扑图</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="h-[460px] w-full overflow-hidden rounded-lg border bg-background">
            <ReactFlow
              key={windowSec}
              nodes={nodes}
              edges={edges}
              fitView
              nodesDraggable={false}
              nodesConnectable={false}
              elementsSelectable={false}
              proOptions={{ hideAttribution: true }}
              colorMode="dark"
            >
              <Background gap={20} size={1} />
              <Controls showInteractive={false} />
            </ReactFlow>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">访问记录</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-3 md:grid-cols-5">
            <div className="space-y-1">
              <Label className="text-xs">用户</Label>
              <Select value={userId} onChange={(event) => { setOffset(0); setUserId(event.target.value) }}>
                <option value="">全部</option>
                {users.map((user) => (
                  <option key={user.id} value={user.id}>{user.name}</option>
                ))}
              </Select>
            </div>
            <div className="space-y-1">
              <Label className="text-xs">入口节点</Label>
              <Select value={nodeId} onChange={(event) => { setOffset(0); setNodeId(event.target.value) }}>
                <option value="">全部</option>
                {(topology?.nodes ?? []).map((node) => (
                  <option key={node.id} value={node.id}>{node.name}</option>
                ))}
              </Select>
            </div>
            <div className="space-y-1">
              <Label className="text-xs">状态</Label>
              <Select value={status} onChange={(event) => { setOffset(0); setStatus(event.target.value) }}>
                <option value="">全部</option>
                <option value="ok">ok</option>
                <option value="error">error</option>
                <option value="denied">denied</option>
                <option value="timeout">timeout</option>
              </Select>
            </div>
            <div className="space-y-1 md:col-span-2">
              <Label className="text-xs">目标关键字</Label>
              <div className="flex gap-2">
                <Input value={targetInput} onChange={(event) => setTargetInput(event.target.value)} placeholder="example.com:443" />
                <Button variant="outline" className="shrink-0 gap-1" onClick={applyFilters}>
                  <Filter className="h-4 w-4" /> 筛选
                </Button>
              </div>
            </div>
          </div>

          <Separator />

          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>时间</TableHead>
                <TableHead>用户</TableHead>
                <TableHead>入口节点</TableHead>
                <TableHead>目标</TableHead>
                <TableHead>协议</TableHead>
                <TableHead>耗时</TableHead>
                <TableHead>上行</TableHead>
                <TableHead>下行</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>路径</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(logs?.items ?? []).map((log) => (
                <TableRow key={log.id} className="cursor-pointer" onClick={() => setSelected(log)}>
                  <TableCell className="whitespace-nowrap text-xs text-muted-foreground">
                    {formatRelative(log.startedAt)}
                  </TableCell>
                  <TableCell className="font-mono text-xs">{log.userId.slice(0, 8)}…</TableCell>
                  <TableCell className="text-xs">{nodeName(log.entryNodeId)}</TableCell>
                  <TableCell className="max-w-48 truncate font-mono text-xs">{log.target}</TableCell>
                  <TableCell className="text-xs uppercase">{log.command}</TableCell>
                  <TableCell className="text-xs">{log.durationMs} ms</TableCell>
                  <TableCell className="text-xs">{formatBytes(log.upBytes)}</TableCell>
                  <TableCell className="text-xs">{formatBytes(log.downBytes)}</TableCell>
                  <TableCell>
                    <Badge
                      variant={
                        log.status === 'ok' ? 'success' : log.status === 'denied' ? 'warning' : 'destructive'
                      }
                    >
                      {log.status}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex flex-wrap gap-1">
                      {log.path.map((id) => (
                        <Badge key={id} variant="outline" className="text-[10px]">{nodeName(id)}</Badge>
                      ))}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {!loading && (logs?.items.length ?? 0) === 0 && (
                <TableRow>
                  <TableCell colSpan={10} className="py-8 text-center text-sm text-muted-foreground">
                    没有访问记录
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>

          <div className="flex items-center justify-between text-xs text-muted-foreground">
            <span>共 {logs?.total ?? 0} 条</span>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={offset === 0}
                onClick={() => setOffset(Math.max(0, offset - limit))}
              >
                上一页
              </Button>
              <span>{Math.floor(offset / limit) + 1} / {Math.max(1, Math.ceil((logs?.total ?? 0) / limit))}</span>
              <Button
                variant="outline"
                size="sm"
                disabled={offset + limit >= (logs?.total ?? 0)}
                onClick={() => setOffset(offset + limit)}
              >
                下一页
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>

      <Dialog open={Boolean(selected)} onOpenChange={(open) => !open && setSelected(null)}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>访问详情</DialogTitle>
            <DialogDescription>{selected ? formatTime(selected.startedAt) : ''}</DialogDescription>
          </DialogHeader>
          {selected && (
            <div className="space-y-3 text-sm">
              <div className="grid grid-cols-2 gap-2">
                <div><span className="text-muted-foreground">用户：</span>{selected.userId}</div>
                <div><span className="text-muted-foreground">入口：</span>{nodeName(selected.entryNodeId)}</div>
                <div><span className="text-muted-foreground">目标：</span>{selected.target}</div>
                <div><span className="text-muted-foreground">协议：</span>{selected.command}</div>
                <div><span className="text-muted-foreground">耗时：</span>{selected.durationMs} ms</div>
                <div><span className="text-muted-foreground">状态：</span>{selected.status}</div>
                <div><span className="text-muted-foreground">上行：</span>{formatBytes(selected.upBytes)}</div>
                <div><span className="text-muted-foreground">下行：</span>{formatBytes(selected.downBytes)}</div>
                <div><span className="text-muted-foreground">客户端：</span>{selected.clientAddr || '—'}</div>
                <div><span className="text-muted-foreground">Session：</span>{selected.sessionId}</div>
              </div>
              <div>
                <div className="mb-1 text-muted-foreground">路径</div>
                <div className="flex flex-wrap gap-1">
                  {selected.path.map((id) => (
                    <Badge key={id} variant="outline">{nodeName(id)}</Badge>
                  ))}
                </div>
              </div>
              {selected.error && (
                <div className="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-xs text-destructive">
                  {selected.error}
                </div>
              )}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </div>
  )
}
