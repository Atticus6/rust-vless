import { Button, Card, Chip, Input, Label, ListBox, Modal, Select, Table, TextField } from '@heroui/react'
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
import { createLink, deleteLink, topology } from '@/lib/rpc'
import type { LinkView, TopologyResponse } from '@/lib/types'

const nodeStyle = {
  background: 'var(--surface)',
  color: 'var(--foreground)',
  border: '1px solid var(--border)',
  borderRadius: 10,
  width: 190,
  fontSize: 12,
  padding: 4,
} satisfies React.CSSProperties

export function ConnectionsPage() {
  const [topologyData, setTopologyData] = useState<TopologyResponse>({ nodes: [], links: [] })
  const [pending, setPending] = useState<{ from: string; to: string } | null>(null)
  const [transport, setTransport] = useState<'rpc' | 'http'>('rpc')
  const [priority, setPriority] = useState('0')
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    try {
      setTopologyData(await topology())
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
      topologyData.nodes.map((node, index) => ({
        id: node.id,
        position: { x: (index % 3) * 300, y: Math.floor(index / 3) * 180 },
        data: {
          label:
            node.isMaster && node.name.toLowerCase() !== 'master'
              ? `${node.name} · master`
              : node.name,
        },
        sourcePosition: Position.Right,
        targetPosition: Position.Left,
        style: nodeStyle,
      })),
    )
  }, [topologyData.nodes, setNodes])

  const edges = useMemo<Edge[]>(
    () =>
      topologyData.links.map((link) => ({
        id: link.id,
        source: link.fromNodeId,
        target: link.toNodeId,
        label: `${link.transport} · #${link.priority}`,
        markerEnd: { type: MarkerType.ArrowClosed },
        animated: true,
        style: { stroke: 'oklch(0.6204 0.195 253.83)' },
        labelStyle: { fill: 'var(--muted)', fontSize: 11 },
      })),
    [topologyData.links],
  )

  function onConnect(connection: Connection) {
    if (!connection.source || !connection.target || connection.source === connection.target) return
    setPending({ from: connection.source, to: connection.target })
    setTransport('rpc')
    setPriority('0')
  }

  async function addLink() {
    if (!pending) return
    setBusy(true)
    try {
      await createLink({
        fromNodeId: pending.from,
        toNodeId: pending.to,
        transport,
        priority: Number(priority) || 0,
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
      await deleteLink(link.id)
      await load()
      toast.success('连线已删除')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '删除失败')
    }
  }

  function nodeName(id: string) {
    return topologyData.nodes.find((node) => node.id === id)?.name ?? id
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">连接</h1>
        <p className="text-sm text-muted">
          拖拽节点建立有向连线；有出边的节点会把流量转发给下一跳，无出边的节点作为出口。
        </p>
      </div>

      <Card>
        <Card.Header className="flex flex-row items-center justify-between">
          <Card.Title className="text-base">拓扑图</Card.Title>
          <div className="flex items-center gap-2 text-xs text-muted">
            <span className="inline-block h-2 w-2 rounded-full bg-success" />
            从节点右侧连接到目标节点左侧
          </div>
        </Card.Header>
        <Card.Content>
          <div className="h-[440px] w-full overflow-hidden rounded-lg border border-border bg-background">
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
        </Card.Content>
      </Card>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">连线列表</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label="连线列表">
              <Table.Header>
                <Table.Column isRowHeader>源节点</Table.Column>
                <Table.Column>目标节点</Table.Column>
                <Table.Column>传输</Table.Column>
                <Table.Column>优先级</Table.Column>
                <Table.Column className="text-right">操作</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    还没有连线，拖拽两个节点建立连接
                  </div>
                )}
              >
                {topologyData.links.map((link) => (
                  <Table.Row key={link.id} id={link.id}>
                    <Table.Cell className="font-medium">{nodeName(link.fromNodeId)}</Table.Cell>
                    <Table.Cell className="font-medium">{nodeName(link.toNodeId)}</Table.Cell>
                    <Table.Cell>
                      <Chip
                        size="sm"
                        variant="soft"
                        color={link.transport === 'rpc' ? 'accent' : 'default'}
                      >
                        {link.transport.toUpperCase()}
                      </Chip>
                    </Table.Cell>
                    <Table.Cell>{link.priority}</Table.Cell>
                    <Table.Cell className="text-right">
                      <Button
                        variant="ghost"
                        size="sm"
                        isIconOnly
                        aria-label={`删除连线 ${link.fromName} 到 ${link.toName}`}
                        onPress={() => void removeLink(link)}
                      >
                        <Trash2 className="h-4 w-4 text-danger" />
                      </Button>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table>
        </Card.Content>
      </Card>

      <Modal.Backdrop isOpen={Boolean(pending)} onOpenChange={(open) => !open && setPending(null)}>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>创建连线</Modal.Heading>
              <p className="text-sm text-muted">
                {pending ? `${nodeName(pending.from)} → ${nodeName(pending.to)}` : ''}
              </p>
            </Modal.Header>
            <Modal.Body>
              <div className="grid gap-3 sm:grid-cols-2">
                {/* React Aria Select 使用 selectedKey/onSelectionChange 作为受控 API。 */}
                <Select
                  aria-label="传输方式"
                  selectedKey={transport}
                  onSelectionChange={(key) => setTransport(key as 'rpc' | 'http')}
                >
                  <Label>传输方式</Label>
                  <Select.Trigger>
                    <Select.Value />
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover>
                    <ListBox>
                      <ListBox.Item id="rpc" textValue="RPC (gRPC)">
                        RPC (gRPC)
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                      <ListBox.Item id="http" textValue="HTTP (WebSocket)">
                        HTTP (WebSocket)
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    </ListBox>
                  </Select.Popover>
                </Select>
                <TextField variant="secondary" fullWidth value={priority} onChange={setPriority}>
                  <Label>优先级</Label>
                  <Input type="number" />
                </TextField>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                取消
              </Button>
              <Button onPress={() => void addLink()} isDisabled={busy} className="gap-1">
                <Plus className="h-4 w-4" /> 创建
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  )
}
