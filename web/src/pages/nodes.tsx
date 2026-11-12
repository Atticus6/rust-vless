import { Button, Card, Chip, Input, Label, Modal, Table, TextField } from '@heroui/react'
import { Copy, Pencil, Plus, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import { useI18n } from '@/i18n'
import { formatRelative } from '@/lib/format'
import { client } from '@/lib/trpc'
import type { NodeView } from '@/lib/types'

export function NodesPage() {
  const { t, locale } = useI18n()
  // tRPC 风格：查询与 mutation 都由函数签名推导，无需手写类型。
  const nodes = client.listNodes.useQuery()
  const createNode = client.createNode.useMutation()
  const updateNode = client.updateNode.useMutation()
  const deleteNode = client.deleteNode.useMutation()

  const [createOpen, setCreateOpen] = useState(false)
  const [name, setName] = useState('')
  const [created, setCreated] = useState<{ node: NodeView; token: string } | null>(null)
  const [editing, setEditing] = useState<NodeView | null>(null)
  const [editName, setEditName] = useState('')
  const [editHttp, setEditHttp] = useState('')
  const [editRpc, setEditRpc] = useState('')

  async function create() {
    if (!name.trim()) return
    try {
      const result = await createNode.mutateAsync({ name })
      setCreated(result)
      setCreateOpen(false)
      setName('')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.createFailed'))
    }
  }

  async function saveEdit() {
    if (!editing) return
    try {
      await updateNode.mutateAsync({ id: editing.id, name: editName, httpUrl: editHttp, rpcUrl: editRpc })
      setEditing(null)
      toast.success(t('nodes.updated'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.updateFailed'))
    }
  }

  async function remove(node: NodeView) {
    if (!window.confirm(t('nodes.deleteConfirm', { name: node.name }))) return
    try {
      await deleteNode.mutateAsync({ id: node.id })
      toast.success(t('nodes.deleted'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.deleteFailed'))
    }
  }

  async function copy(text: string) {
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(text)
        toast.success(t('common.copied'))
        return
      }
    } catch {
      // 回退到手动复制。
    }
    window.prompt(t('common.manualCopy'), text)
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
        `RUST_VLESS_UPGRADE_COMMAND=/usr/local/share/rust-vless/node/rust-vless-upgrade-node \\`,
        `rust-vless-node`,
      ].join('\n')
    : ''

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{t('nodes.title')}</h1>
          <p className="text-sm text-muted">{t('nodes.subtitle')}</p>
        </div>
        <Button className="gap-2" onPress={() => setCreateOpen(true)}>
          <Plus className="h-4 w-4" /> {t('nodes.create')}
        </Button>
      </div>

      {nodes.error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {nodes.error.message}
        </div>
      )}

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('nodes.list')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label={t('nodes.list')}>
              <Table.Header>
                <Table.Column isRowHeader>{t('common.name')}</Table.Column>
                <Table.Column>{t('common.status')}</Table.Column>
                <Table.Column>HTTP</Table.Column>
                <Table.Column>RPC</Table.Column>
                <Table.Column>{t('nodes.version')}</Table.Column>
                <Table.Column>{t('common.connections')}</Table.Column>
                <Table.Column>{t('nodes.lastHeartbeat')}</Table.Column>
                <Table.Column className="text-right">{t('common.actions')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {nodes.isLoading ? t('common.loading') : t('nodes.empty')}
                  </div>
                )}
              >
                {(nodes.data ?? []).map((node) => (
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
                      <Chip
                        size="sm"
                        variant="soft"
                        color={node.status === 'online' ? 'success' : 'default'}
                      >
                        {node.status === 'online' ? t('common.online') : t('common.offline')}
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
                    <Table.Cell className="text-xs text-muted">
                      {formatRelative(node.lastSeen, locale)}
                    </Table.Cell>
                    <Table.Cell className="text-right">
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          isIconOnly
                          aria-label={t('nodes.editAria', { name: node.name })}
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
                            aria-label={t('nodes.deleteAria', { name: node.name })}
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
              <Modal.Heading>{t('nodes.createTitle')}</Modal.Heading>
              <p className="text-sm text-muted">{t('nodes.createHint')}</p>
            </Modal.Header>
            <Modal.Body>
              <TextField variant="secondary" fullWidth value={name} onChange={setName}>
                <Label>{t('common.name')}</Label>
                <Input placeholder="tokyo-1" />
              </TextField>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                {t('common.cancel')}
              </Button>
              <Button
                onPress={() => void create()}
                isDisabled={createNode.isPending || !name.trim()}
              >
                {t('common.create')}
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
              <Modal.Heading>{t('nodes.createdTitle')}</Modal.Heading>
              <p className="text-sm text-muted">{t('nodes.createdHint')}</p>
            </Modal.Header>
            <Modal.Body>
              <div className="rounded-md border border-border bg-surface-secondary p-3">
                <div className="mb-2 flex items-center justify-between">
                  <span className="text-xs font-medium text-muted">{t('nodes.startCommand')}</span>
                  <Button
                    variant="outline"
                    size="sm"
                    className="gap-1"
                    onPress={() => void copy(startCommand)}
                  >
                    <Copy className="h-3.5 w-3.5" /> {t('common.copy')}
                  </Button>
                </div>
                <pre className="whitespace-pre-wrap break-all text-xs">{startCommand}</pre>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close">{t('common.done')}</Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>

      <Modal.Backdrop isOpen={Boolean(editing)} onOpenChange={(open) => !open && setEditing(null)}>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{t('nodes.editTitle')}</Modal.Heading>
              <p className="text-sm text-muted">{t('nodes.editHint')}</p>
            </Modal.Header>
            <Modal.Body>
              <div className="space-y-3">
                <TextField variant="secondary" fullWidth value={editName} onChange={setEditName}>
                  <Label>{t('common.name')}</Label>
                  <Input />
                </TextField>
                <TextField variant="secondary" fullWidth value={editHttp} onChange={setEditHttp}>
                  <Label>{t('nodes.publicHttp')}</Label>
                  <Input placeholder="http://node:8080" />
                </TextField>
                <TextField variant="secondary" fullWidth value={editRpc} onChange={setEditRpc}>
                  <Label>{t('nodes.publicRpc')}</Label>
                  <Input placeholder="http://node:50051" />
                </TextField>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                {t('common.cancel')}
              </Button>
              <Button onPress={() => void saveEdit()} isDisabled={updateNode.isPending}>
                {t('common.save')}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  )
}
