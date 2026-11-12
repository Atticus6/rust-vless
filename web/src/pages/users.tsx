import { Button, Card, Chip, Input, Label, Modal, Table, TextField } from '@heroui/react'
import { Copy, Pencil, Plus, Power, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import { formatBytes } from '@/lib/format'
import { createUser, deleteUser, listUsers, updateUser } from '@/lib/rpc'
import type { ProxyUserView } from '@/lib/types'

export function UsersPage() {
  const [users, setUsers] = useState<ProxyUserView[]>([])
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [editing, setEditing] = useState<ProxyUserView | null>(null)
  const [editName, setEditName] = useState('')

  const load = useCallback(async () => {
    try {
      setUsers(await listUsers())
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载用户失败')
    }
  }, [])

  useEffect(() => {
    void load()
  }, [load])

  async function create() {
    if (!name.trim()) return
    setBusy(true)
    try {
      await createUser(name)
      setName('')
      setOpen(false)
      await load()
      toast.success('代理用户已创建，配置会实时下发节点')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '创建失败')
    } finally {
      setBusy(false)
    }
  }

  async function saveEdit() {
    if (!editing || !editName.trim()) return
    setBusy(true)
    try {
      await updateUser(editing.id, { name: editName })
      setEditing(null)
      await load()
      toast.success('用户名称已更新')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '更新失败')
    } finally {
      setBusy(false)
    }
  }

  async function toggle(user: ProxyUserView) {
    try {
      await updateUser(user.id, { enabled: !user.enabled })
      await load()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '更新失败')
    }
  }

  async function remove(user: ProxyUserView) {
    if (!window.confirm(`删除代理用户 ${user.name}？`)) return
    try {
      await deleteUser(user.id)
      await load()
      toast.success('代理用户已删除')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '删除失败')
    }
  }

  async function copy(id: string) {
    await navigator.clipboard.writeText(id)
    toast.success('UUID 已复制')
  }

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">代理用户</h1>
          <p className="text-sm text-muted">VLESS UUID 用户；启用/禁用会实时同步到所有节点</p>
        </div>
        <Button className="gap-2" onPress={() => setOpen(true)}>
          <Plus className="h-4 w-4" /> 添加用户
        </Button>
      </div>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">用户列表</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label="代理用户列表">
              <Table.Header>
                <Table.Column isRowHeader>名称</Table.Column>
                <Table.Column>UUID</Table.Column>
                <Table.Column>状态</Table.Column>
                <Table.Column>所有者</Table.Column>
                <Table.Column>上行</Table.Column>
                <Table.Column>下行</Table.Column>
                <Table.Column className="text-right">操作</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">还没有代理用户</div>
                )}
              >
                {users.map((user) => (
                  <Table.Row key={user.id} id={user.id}>
                    <Table.Cell className="font-medium">{user.name}</Table.Cell>
                    <Table.Cell>
                      <button
                        className="flex max-w-64 items-start gap-1 text-left font-mono text-[11px] text-muted hover:text-foreground"
                        onClick={() => void copy(user.id)}
                      >
                        <span className="break-all">{user.id}</span>
                        <Copy className="mt-0.5 h-3 w-3 shrink-0" />
                      </button>
                    </Table.Cell>
                    <Table.Cell>
                      <Chip size="sm" variant="soft" color={user.enabled ? 'success' : 'default'}>
                        {user.enabled ? '启用' : '禁用'}
                      </Chip>
                    </Table.Cell>
                    <Table.Cell className="text-xs text-muted">{user.ownerEmail}</Table.Cell>
                    <Table.Cell className="text-xs">{formatBytes(user.upBytes)}</Table.Cell>
                    <Table.Cell className="text-xs">{formatBytes(user.downBytes)}</Table.Cell>
                    <Table.Cell className="text-right">
                      <div className="flex justify-end gap-1">
                        <Button
                          variant="ghost"
                          size="sm"
                          isIconOnly
                          aria-label={`编辑 ${user.name}`}
                          onPress={() => {
                            setEditing(user)
                            setEditName(user.name)
                          }}
                        >
                          <Pencil className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          isIconOnly
                          aria-label={user.enabled ? `禁用 ${user.name}` : `启用 ${user.name}`}
                          onPress={() => void toggle(user)}
                        >
                          <Power
                            className={user.enabled ? 'h-4 w-4 text-success' : 'h-4 w-4 text-muted'}
                          />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          isIconOnly
                          aria-label={`删除 ${user.name}`}
                          onPress={() => void remove(user)}
                        >
                          <Trash2 className="h-4 w-4 text-danger" />
                        </Button>
                      </div>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table>
        </Card.Content>
      </Card>

      <Modal.Backdrop isOpen={Boolean(editing)} onOpenChange={(value) => !value && setEditing(null)}>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>编辑代理用户</Modal.Heading>
              <p className="text-sm text-muted">只修改名称，UUID 保持不变，客户端无需重新配置。</p>
            </Modal.Header>
            <Modal.Body>
              <TextField variant="secondary" fullWidth value={editName} onChange={setEditName}>
                <Label>名称</Label>
                <Input />
              </TextField>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                取消
              </Button>
              <Button onPress={() => void saveEdit()} isDisabled={busy || !editName.trim()}>
                保存
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>

      <Modal.Backdrop isOpen={open} onOpenChange={setOpen}>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>添加代理用户</Modal.Heading>
              <p className="text-sm text-muted">系统会生成一个 VLESS UUID，客户端用它完成鉴权。</p>
            </Modal.Header>
            <Modal.Body>
              <TextField variant="secondary" fullWidth value={name} onChange={setName}>
                <Label>名称</Label>
                <Input placeholder="phone" />
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
    </div>
  )
}
