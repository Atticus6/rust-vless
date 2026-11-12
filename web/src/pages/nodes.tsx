import { Copy, Pencil, Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
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
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'
import { formatRelative } from '@/lib/format'
import type { CreateNodeResponse, NodeView } from '@/lib/types'

export function NodesPage() {
  const [nodes, setNodes] = useState<NodeView[]>([])
  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState('')
  const [created, setCreated] = useState<CreateNodeResponse | null>(null)
  const [editing, setEditing] = useState<NodeView | null>(null)
  const [editName, setEditName] = useState('')
  const [editHttp, setEditHttp] = useState('')
  const [editRpc, setEditRpc] = useState('')
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setNodes(await api<NodeView[]>('/api/nodes'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载节点失败')
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  async function create() {
    if (!name.trim()) return
    setBusy(true)
    try {
      const result = await api<CreateNodeResponse>('/api/nodes', {
        method: 'POST',
        body: JSON.stringify({ name }),
      })
      setCreated(result)
      setCreateOpen(false)
      setName('')
      await load()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '创建失败')
    } finally {
      setBusy(false)
    }
  }

  async function saveEdit() {
    if (!editing) return
    setBusy(true)
    try {
      await api(`/api/nodes/${editing.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ name: editName, httpUrl: editHttp, rpcUrl: editRpc }),
      })
      setEditing(null)
      await load()
      toast.success('节点已更新')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '更新失败')
    } finally {
      setBusy(false)
    }
  }

  async function remove(node: NodeView) {
    if (!window.confirm(`删除节点 ${node.name}？`)) return
    try {
      await api(`/api/nodes/${node.id}`, { method: 'DELETE' })
      await load()
      toast.success('节点已删除')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '删除失败')
    }
  }

  async function copy(text: string) {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text)
        toast.success('已复制')
        return
      }
    } catch {
      // fall through to manual copy
    }
    window.prompt('请手动复制', text)
  }

  const startCommand = created
    ? [
        `RUST_VLESS_MASTER_GRPC=http://<master>:50051 \\`,
        `RUST_VLESS_NODE_ID=${created.node.id} \\`,
        `RUST_VLESS_NODE_TOKEN=${created.token} \\`,
        `RUST_VLESS_NODE_LISTEN=0.0.0.0:8080 \\`,
        `RUST_VLESS_RPC_ADDR=0.0.0.0:50051 \\`,
        `RUST_VLESS_PUBLIC_HTTP_URL=http://<node>:8080 \\`,
        `RUST_VLESS_PUBLIC_RPC_URL=http://<node>:50051 \\`,
        `rust-vless-node`,
      ].join('\n')
    : ''

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">节点</h1>
          <p className="text-sm text-muted-foreground">主节点与子节点；子节点主动连接主节点</p>
        </div>
        <Button className="gap-2" onClick={() => setCreateOpen(true)}>
          <Plus className="h-4 w-4" /> 创建节点
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">节点列表</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>名称</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>HTTP</TableHead>
                <TableHead>RPC</TableHead>
                <TableHead>版本</TableHead>
                <TableHead>连接</TableHead>
                <TableHead>最近心跳</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {nodes.map((node) => (
                <TableRow key={node.id}>
                  <TableCell>
                    <div className="flex items-center gap-2">
                      <span className="font-medium">{node.name}</span>
                      {node.isMaster && <Badge variant="secondary">Master</Badge>}
                    </div>
                  </TableCell>
                  <TableCell>
                    <Badge variant={node.status === 'online' ? 'success' : 'outline'}>
                      {node.status === 'online' ? '在线' : '离线'}
                    </Badge>
                  </TableCell>
                  <TableCell className="max-w-40 truncate text-xs text-muted-foreground">{node.httpUrl || '—'}</TableCell>
                  <TableCell className="max-w-40 truncate text-xs text-muted-foreground">{node.rpcUrl || '—'}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">{node.version || '—'}</TableCell>
                  <TableCell>{node.activeConnections}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">{formatRelative(node.lastSeen)}</TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => {
                          setEditing(node)
                          setEditName(node.name)
                          setEditHttp(node.httpUrl)
                          setEditRpc(node.rpcUrl)
                        }}
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      {!node.isMaster && (
                        <Button variant="ghost" size="icon" onClick={() => void remove(node)}>
                          <Trash2 className="h-4 w-4 text-destructive" />
                        </Button>
                      )}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {nodes.length === 0 && (
                <TableRow>
                  <TableCell colSpan={8} className="py-8 text-center text-sm text-muted-foreground">
                    还没有节点
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>创建子节点</DialogTitle>
            <DialogDescription>创建后会展示一次性 token，请立即保存。</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="node-name">名称</Label>
            <Input id="node-name" value={name} onChange={(event) => setName(event.target.value)} placeholder="tokyo-1" />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setCreateOpen(false)}>取消</Button>
            <Button onClick={() => void create()} disabled={busy || !name.trim()}>创建</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(created)} onOpenChange={(open) => !open && setCreated(null)}>
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>节点已创建</DialogTitle>
            <DialogDescription>token 只显示一次，复制下面的启动命令并保存。</DialogDescription>
          </DialogHeader>
          <div className="rounded-md border bg-muted/40 p-3">
            <div className="mb-2 flex items-center justify-between">
              <span className="text-xs font-medium text-muted-foreground">启动命令</span>
              <Button variant="outline" size="sm" className="gap-1" onClick={() => void copy(startCommand)}>
                <Copy className="h-3.5 w-3.5" /> 复制
              </Button>
            </div>
            <pre className="whitespace-pre-wrap break-all text-xs">{startCommand}</pre>
          </div>
          <DialogFooter>
            <Button onClick={() => setCreated(null)}>完成</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={Boolean(editing)} onOpenChange={(open) => !open && setEditing(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>编辑节点</DialogTitle>
            <DialogDescription>公网地址用于其他节点建立中转连线。</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-2">
              <Label htmlFor="edit-name">名称</Label>
              <Input id="edit-name" value={editName} onChange={(event) => setEditName(event.target.value)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-http">公网 HTTP 地址</Label>
              <Input id="edit-http" value={editHttp} onChange={(event) => setEditHttp(event.target.value)} placeholder="http://node:8080" />
            </div>
            <div className="space-y-2">
              <Label htmlFor="edit-rpc">公网 RPC 地址</Label>
              <Input id="edit-rpc" value={editRpc} onChange={(event) => setEditRpc(event.target.value)} placeholder="http://node:50051" />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditing(null)}>取消</Button>
            <Button onClick={() => void saveEdit()} disabled={busy}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
