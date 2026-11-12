import { Button, Card, Chip, Input, Label, Modal, Table, TextField } from '@heroui/react'
import { Copy, Pencil, Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import { formatRelative } from '@/lib/format'
import { createNode, deleteNode, listNodes, updateNode } from '@/lib/rpc'
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
      setNodes(await listNodes())
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
      const result = await createNode(name)
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
      await updateNode(editing.id, { name: editName, httpUrl: editHttp, rpcUrl: editRpc })
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
      await deleteNode(node.id)
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
          <p className="text-sm text-muted">主节点与子节点；子节点主动连接主节点</p>
        </div>
        <Button className="gap-2" onPress={() => setCreateOpen(true)}>
          <Plus className="h-4 w-4" /> 创建节点
        </Button>
      </div>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">节点列表</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label="节点列表">
              <Table.Header>
                <Table.Column isRowHeader>名称</Table.Column>
                <Table.Column>状态</Table.Column>
                <Table.Column>HTTP</Table.Column>
                <Table.Column>RPC</Table.Column>
                <Table.Column>版本</Table.Column>
                <Table.Column>连接</Table.Column>
                <Table.Column>最近心跳</Table.Column>
                <Table.Column className="text-right">操作</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">还没有节点</div>
                )}
              >
                {nodes.map((node) => (
                  <Table.Row key={node.id} id={node.id}>
                    <Table.Cell>
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{node.name}</span>
                        {node.isMaster && (
                          <Chip size="sm" variant="soft" color="accent">
                            Master
                          </Chip>
                        )}
                      </div>
                    </Table.Cell>
                    <Table.Cell>
                      <Chip size="sm" variant="soft" color={node.status === 'online' ? 'success' : 'default'}>
                        {node.status === 'online' ? '在线' : '离线'}
                      </Chip>
                    </Table.Cell>
                    <Table.Cell className="max-w-40 truncate text-xs text-muted">
                      {node.httpUrl || '—'}
                    </Table.Cell>
                    <Table.Cell className="max-w-40 truncate text-xs text-muted">
                      {node.rpcUrl || '—'}
                    </Table.Cell>
                    <Table.Cell className="text-xs text-muted">{node.version || '—'}</Table.Cell>
                    <Table.Cell>{node.activeConnections}</Table.Cell>
                    <Table.Cell className="text-xs text-muted">{formatRelative(node.lastSeen)}</Table.Cell>
                    <Table.Cell className="text-right">
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          isIconOnly
                          aria-label={`编辑 ${node.name}`}
                          onPress={() => {
                            setEditing(node)
                            setEditName(node.name)
                            setEditHttp(node.httpUrl)
                            setEditRpc(node.rpcUrl)
                          }}
                        >
                          <Pencil className="h-4 w-4" />
                        </Button>
                        {!node.isMaster && (
                          <Button
                            variant="ghost"
                            size="sm"
                            isIconOnly
                            aria-label={`删除 ${node.name}`}
                            onPress={() => void remove(node)}
                          >
                            <Trash2 className="h-4 w-4 text-danger" />
                          </Button>
                        )}
                      </div>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table>
        </Card.Content>
      </Card>

      <Modal.Backdrop isOpen={createOpen} onOpenChange={setCreateOpen}>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>创建子节点</Modal.Heading>
              <p className="text-sm text-muted">创建后会展示一次性 token，请立即保存。</p>
            </Modal.Header>
            <Modal.Body>
              <TextField variant="secondary" fullWidth value={name} onChange={setName}>
                <Label>名称</Label>
                <Input placeholder="tokyo-1" />
              </TextField>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                取消
              </Button>
              <Button onPress={() => void create()} isDisabled={busy || !name.trim()}>
                创建
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>

      <Modal.Backdrop isOpen={Boolean(created)} onOpenChange={(open) => !open && setCreated(null)}>
        <Modal.Container>
          <Modal.Dialog className="sm:max-w-2xl">
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>节点已创建</Modal.Heading>
              <p className="text-sm text-muted">token 只显示一次，复制下面的启动命令并保存。</p>
            </Modal.Header>
            <Modal.Body>
              <div className="rounded-md border border-border bg-surface-secondary p-3">
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-xs font-medium text-muted">启动命令</span>
                  <Button
                    variant="outline"
                    size="sm"
                    className="gap-1"
                    onPress={() => void copy(startCommand)}
                  >
                    <Copy className="h-3.5 w-3.5" /> 复制
                  </Button>
                </div>
                <pre className="whitespace-pre-wrap break-all text-xs">{startCommand}</pre>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close">完成</Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>

      <Modal.Backdrop isOpen={Boolean(editing)} onOpenChange={(open) => !open && setEditing(null)}>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>编辑节点</Modal.Heading>
              <p className="text-sm text-muted">公网地址用于其他节点建立中转连线。</p>
            </Modal.Header>
            <Modal.Body>
              <div className="space-y-3">
                <TextField variant="secondary" fullWidth value={editName} onChange={setEditName}>
                  <Label>名称</Label>
                  <Input />
                </TextField>
                <TextField variant="secondary" fullWidth value={editHttp} onChange={setEditHttp}>
                  <Label>公网 HTTP 地址</Label>
                  <Input placeholder="http://node:8080" />
                </TextField>
                <TextField variant="secondary" fullWidth value={editRpc} onChange={setEditRpc}>
                  <Label>公网 RPC 地址</Label>
                  <Input placeholder="http://node:50051" />
                </TextField>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                取消
              </Button>
              <Button onPress={() => void saveEdit()} isDisabled={busy}>
                保存
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  )
}
