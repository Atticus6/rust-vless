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
import { Plus, Trash2 } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { toast } from 'sonner'
import { getAdmin } from '@/lib/auth'
import { formatTime } from '@/lib/format'
import { createAdmin, deleteAdmin, listAdmins } from '@/lib/rpc'
import type { AdminView, Role } from '@/lib/types'

export function TeamPage() {
  const admin = getAdmin()
  const [admins, setAdmins] = useState<AdminView[]>([])
  const [open, setOpen] = useState(false)
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<Role>('user')
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    if (admin?.role !== 'admin') return
    try {
      setAdmins(await listAdmins())
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载账号失败')
    }
  }, [admin?.role])

  useEffect(() => {
    void load()
  }, [load])

  if (admin?.role !== 'admin') {
    return (
      <div className="rounded-lg border border-border bg-surface p-8 text-center text-sm text-muted">
        只有 admin 账号可以管理团队。
      </div>
    )
  }

  async function create() {
    setBusy(true)
    try {
      await createAdmin({ email, password, role })
      setEmail('')
      setPassword('')
      setRole('user')
      setOpen(false)
      await load()
      toast.success('账号已创建')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '创建失败')
    } finally {
      setBusy(false)
    }
  }

  async function remove(item: AdminView) {
    if (!window.confirm(`删除账号 ${item.email}？其名下代理用户也会被删除。`)) return
    try {
      await deleteAdmin(item.id)
      await load()
      toast.success('账号已删除')
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '删除失败')
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">团队</h1>
          <p className="text-sm text-muted">admin 管理全部资源；user 只能管理自己名下的代理用户</p>
        </div>
        <Button className="gap-2" onPress={() => setOpen(true)}>
          <Plus className="h-4 w-4" /> 添加账号
        </Button>
      </div>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">管理账号</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label="管理账号列表">
              <Table.Header>
                <Table.Column isRowHeader>邮箱</Table.Column>
                <Table.Column>角色</Table.Column>
                <Table.Column>创建时间</Table.Column>
                <Table.Column className="text-right">操作</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">还没有管理账号</div>
                )}
              >
                {admins.map((item) => (
                  <Table.Row key={item.id} id={item.id}>
                    <Table.Cell className="font-medium">{item.email}</Table.Cell>
                    <Table.Cell>
                      <Chip size="sm" variant="soft" color={item.role === 'admin' ? 'accent' : 'default'}>
                        {item.role}
                      </Chip>
                    </Table.Cell>
                    <Table.Cell className="text-xs text-muted">{formatTime(item.createdAt)}</Table.Cell>
                    <Table.Cell className="text-right">
                      {item.id !== admin?.id && (
                        <Button
                          variant="ghost"
                          size="sm"
                          isIconOnly
                          aria-label={`删除 ${item.email}`}
                          onPress={() => void remove(item)}
                        >
                          <Trash2 className="h-4 w-4 text-danger" />
                        </Button>
                      )}
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table>
        </Card.Content>
      </Card>

      <Modal.Backdrop isOpen={open} onOpenChange={setOpen}>
        <Modal.Container>
          <Modal.Dialog>
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>添加管理账号</Modal.Heading>
              <p className="text-sm text-muted">密码至少 8 位。</p>
            </Modal.Header>
            <Modal.Body>
              <div className="space-y-3">
                <TextField variant="secondary" isRequired fullWidth value={email} onChange={setEmail}>
                  <Label>邮箱</Label>
                  <Input type="email" />
                </TextField>
                <TextField variant="secondary" isRequired fullWidth value={password} onChange={setPassword}>
                  <Label>密码</Label>
                  <Input type="password" />
                </TextField>
                {/* React Aria Select 使用 selectedKey/onSelectionChange 作为受控 API。 */}
                <Select
                  aria-label="角色"
                  selectedKey={role}
                  onSelectionChange={(key) => setRole(key as Role)}
                >
                  <Label>角色</Label>
                  <Select.Trigger>
                    <Select.Value />
                    <Select.Indicator />
                  </Select.Trigger>
                  <Select.Popover>
                    <ListBox>
                      <ListBox.Item id="user" textValue="user">
                        user
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                      <ListBox.Item id="admin" textValue="admin">
                        admin
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    </ListBox>
                  </Select.Popover>
                </Select>
              </div>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                取消
              </Button>
              <Button
                onPress={() => void create()}
                isDisabled={busy || !email || password.length < 8}
              >
                创建
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  )
}
