import {
  Button,
  Card,
  Chip,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Table,
  TextField,
} from '@heroui/react'
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
import { useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { LinkTransport } from '@/gen/rpc_pb'
import { useI18n } from '@/i18n'
import { client } from '@/lib/trpc'
import type { LinkView } from '@/lib/types'

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
  const { t } = useI18n()
  const topology = client.getTopology.useQuery()
  const createLink = client.createLink.useMutation()
  const deleteLink = client.deleteLink.useMutation()

  const [pending, setPending] = useState<{ from: string; to: string } | null>(null)
  const [transport, setTransport] = useState<'rpc' | 'http'>('rpc')
  const [priority, setPriority] = useState('0')

  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([])

  useEffect(() => {
    setNodes(
      (topology.data?.nodes ?? []).map((node, index) => ({
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
  }, [topology.data?.nodes, setNodes])

  const edges = useMemo<Edge[]>(
    () =>
      (topology.data?.links ?? []).map((link) => ({
        id: link.id,
        source: link.fromNodeId,
        target: link.toNodeId,
        label: `${link.transport} · #${link.priority}`,
        markerEnd: { type: MarkerType.ArrowClosed },
        animated: true,
        style: { stroke: 'oklch(0.6204 0.195 253.83)' },
        labelStyle: { fill: 'var(--muted)', fontSize: 11 },
      })),
    [topology.data?.links],
  )

  function onConnect(connection: Connection) {
    if (!connection.source || !connection.target || connection.source === connection.target) return
    setPending({ from: connection.source, to: connection.target })
    setTransport('rpc')
    setPriority('0')
  }

  async function addLink() {
    if (!pending) return
    try {
      await createLink.mutateAsync({
        fromNodeId: pending.from,
        toNodeId: pending.to,
        transport: transport === 'http' ? LinkTransport.HTTP : LinkTransport.RPC,
        priority: BigInt(Number(priority) || 0),
      })
      setPending(null)
      toast.success(t('connections.created'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('connections.createFailed'))
    }
  }

  async function removeLink(link: LinkView) {
    if (
      !window.confirm(t('connections.deleteConfirm', { from: link.fromName, to: link.toName }))
    ) {
      return
    }
    try {
      await deleteLink.mutateAsync({ id: link.id })
      toast.success(t('connections.deleted'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.deleteFailed'))
    }
  }

  function nodeName(id: string) {
    return topology.data?.nodes.find((node) => node.id === id)?.name ?? id
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight">{t('connections.title')}</h1>
        <p className="text-sm text-muted">{t('connections.subtitle')}</p>
      </div>

      {topology.error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {topology.error.message}
        </div>
      )}

      <Card>
        <Card.Header className="flex flex-row items-center justify-between">
          <Card.Title className="text-base">{t('connections.topology')}</Card.Title>
          <div className="flex items-center gap-2 text-xs text-muted">
            <span className="inline-block h-2 w-2 rounded-full bg-success" />
            {t('connections.dragHint')}
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
          <Card.Title className="text-base">{t('connections.list')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label={t('connections.list')}>
              <Table.Header>
                <Table.Column isRowHeader>{t('connections.source')}</Table.Column>
                <Table.Column>{t('connections.target')}</Table.Column>
                <Table.Column>{t('connections.transport')}</Table.Column>
                <Table.Column>{t('connections.priority')}</Table.Column>
                <Table.Column className="text-right">{t('common.actions')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {topology.isLoading ? t('common.loading') : t('connections.empty')}
                  </div>
                )}
              >
                {(topology.data?.links ?? []).map((link) => (
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
                        aria-label={t('connections.deleteAria', {
                          from: link.fromName,
                          to: link.toName,
                        })}
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
              <Modal.Heading>{t('connections.createTitle')}</Modal.Heading>
              <p className="text-sm text-muted">
                {pending ? `${nodeName(pending.from)} → ${nodeName(pending.to)}` : ''}
              </p>
            </Modal.Header>
            <Modal.Body>
              <div className="grid gap-3 sm:grid-cols-2">
                {/* React Aria Select 使用 selectedKey/onSelectionChange 作为受控 API。 */}
                <Select
                  aria-label={t('connections.transportLabel')}
                  selectedKey={transport}
                  onSelectionChange={(key) => setTransport(key as 'rpc' | 'http')}
                >
                  <Label>{t('connections.transportLabel')}</Label>
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
                  <Label>{t('connections.priority')}</Label>
                  <Input type="number" />
                </TextField>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                {t('common.cancel')}
              </Button>
              <Button onPress={() => void addLink()} isDisabled={createLink.isPending} className="gap-1">
                <Plus className="h-4 w-4" /> {t('common.create')}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  )
}
