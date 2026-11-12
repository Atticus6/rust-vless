import {
  Background,
  Controls,
  MarkerType,
  Position,
  ReactFlow,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
} from '@xyflow/react'
import { Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Select } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'
import type { LinkView, TopologyResponse } from '@/lib/types'

const nodeStyle = {
  background: 'hsl(var(--card))',
  color: 'hsl(var(--foreground))',
  border: '1px solid hsl(var(--border))',
  borderRadius: 10,
  width: 190,
  fontSize: 12,
  padding: 4,
} satisfies React.CSSProperties

export function ConnectionsPage() {
  const [topology, setTopology] = useState<TopologyResponse>({ nodes: [], links: [] })
  const [pending, setPending] = useState<{ from: string; to: string } | null>(null)
  const [transport, setTransport] = useState<'rpc' | 'http'>('rpc')
  const [priority, setPriority] = useState('0')
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setTopology(await api<TopologyResponse>('/api/topology'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载拓扑失败')
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([])

  useEffect(() => {
    setNodes(
      topology.nodes.map((node, index) => ({
        id: node.id,
        position: { x: (index % 3) * 300, y: Math.floor(index / 3) * 180 },
        data: { label: node.isMaster && node.name.toLowerCase() !== 'master' ? `${node.name} · master` : node.name },
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        style: nodeStyle,
      })),
    )
  }, [topology.nodes, setNodes])

  const edges = useMemo<Edge[]>(
    () =>
      topology.links.map((link) => ({
        id: link.id,
        source: link.fromNodeId,
        target: link.toNodeId,
        label: `${link.transport} · #${link.priority}`,
        markerEnd: { type: MarkerType.ArrowClosed },
        animated: true,
        style: { stroke: 'hsl(160 84% 39%)' },
        labelStyle: { fill: 'hsl(var(--muted-foreground))', fontSize: 11 },
      })),
    [topology.links],
  )

  function onConnect(connection: Connection) {
    if (!connection.source || !connection.target || connection.source === connection.target) return
    setPending({ from: connection.source, to: connection.target })
    setTransport('rpc')
    setPriority('0')
  }

  async function createLink() {
    if (!pending) return
    setBusy(true)
    try {
      await api('/api/links', {
        method: 'POST',
        body: JSON.stringify({
          fromNodeId: pending.from,
          toNodeId: pending.to,
          transport,
          priority: Number(priority) || 0,
        }),
      })
      setPending(null)
      await load()
      toast.success('连线已创建')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '创建连线失败')
    } finally {
      setBusy(false)
    }
  }

  async function removeLink(link: LinkView) {
    if (!window.confirm(`删除连线 ${link.fromName} → ${link.toName}？`)) return
    try {
      await api(`/api/links/${link.id}`, { method: 'DELETE' })
      await load()
      toast.success('连线已删除')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '删除失败')
    }
  }

  function nodeName(id: string) {
    return topology.nodes.find((node) => node.id === id)?.name ?? id
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">连接</h1>
        <p className="text-sm text-muted-foreground">
          拖拽节点建立有向连线；有出边的节点会把流量转发给下一跳，无出边的节点作为出口。
        </p>
      </div>

      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle className="text-base">拓扑图</CardTitle>
          <div className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="inline-block h-2 w-2 rounded-full bg-emerald-500" />
            从节点右侧连接到目标节点左侧
          </div>
        </CardHeader>
        <CardContent>
          <div className="h-[440px] w-full overflow-hidden rounded-lg border bg-background">
            <ReactFlow
              nodes={nodes}
              edges={edges}
              onNodesChange={onNodesChange}
              onConnect={onConnect}
              fitView
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
          <CardTitle className="text-base">连线列表</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>源节点</TableHead>
                <TableHead>目标节点</TableHead>
                <TableHead>传输</TableHead>
                <TableHead>优先级</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {topology.links.map((link) => (
                <TableRow key={link.id}>
                  <TableCell className="font-medium">{nodeName(link.fromNodeId)}</TableCell>
                  <TableCell className="font-medium">{nodeName(link.toNodeId)}</TableCell>
                  <TableCell>
                    <Badge variant={link.transport === 'rpc' ? 'default' : 'secondary'}>
                      {link.transport.toUpperCase()}
                    </Badge>
                  </TableCell>
                  <TableCell>{link.priority}</TableCell>
                  <TableCell className="text-right">
                    <Button variant="ghost" size="icon" onClick={() => void removeLink(link)}>
                      <Trash2 className="h-4 w-4 text-destructive" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {topology.links.length === 0 && (
                <TableRow>
                  <TableCell colSpan={5} className="py-8 text-center text-sm text-muted-foreground">
                    还没有连线，拖拽两个节点建立连接
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Dialog open={Boolean(pending)} onOpenChange={(open) => !open && setPending(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>创建连线</DialogTitle>
            <DialogDescription>
              {pending ? `${nodeName(pending.from)} → ${nodeName(pending.to)}` : ''}
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-2">
              <Label>传输方式</Label>
              <Select value={transport} onChange={(event) => setTransport(event.target.value as 'rpc' | 'http')}>
                <option value="rpc">RPC (gRPC)</option>
                <option value="http">HTTP (WebSocket)</option>
              </Select>
            </div>
            <div className="space-y-2">
              <Label>优先级</Label>
              <Input type="number" value={priority} onChange={(event) => setPriority(event.target.value)} />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setPending(null)}>取消</Button>
            <Button onClick={() => void createLink()} disabled={busy} className="gap-1">
              <Plus className="h-4 w-4" /> 创建
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
