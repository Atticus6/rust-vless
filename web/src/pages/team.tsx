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
import { useState } from 'react'
import { toast } from 'sonner'
import { WebRole } from '@/gen/rpc_pb'
import { useI18n } from '@/i18n'
import { getAdmin } from '@/lib/auth'
import { formatTime } from '@/lib/format'
import { client } from '@/lib/trpc'
import type { AdminView, Role } from '@/lib/types'

export function TeamPage() {
  const { t, locale } = useI18n()
  const admin = getAdmin()
  const isAdmin = admin?.role === 'admin'
  const admins = client.listAdmins.useQuery(undefined, { enabled: isAdmin })
  const createAdmin = client.createAdmin.useMutation()
  const deleteAdmin = client.deleteAdmin.useMutation()

  const [open, setOpen] = useState(false)
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<Role>('user')

  if (!isAdmin) {
    return (
      <div className="rounded-lg border border-border bg-surface p-8 text-center text-sm text-muted">
        {t('team.adminOnly')}
      </div>
    )
  }

  async function create() {
    try {
      await createAdmin.mutateAsync({
        email,
        password,
        role: role === 'admin' ? WebRole.ADMIN : WebRole.USER,
      })
      setEmail('')
      setPassword('')
      setRole('user')
      setOpen(false)
      toast.success(t('team.created'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.createFailed'))
    }
  }

  async function remove(item: AdminView) {
    if (!window.confirm(t('team.deleteConfirm', { email: item.email }))) return
    try {
      await deleteAdmin.mutateAsync({ id: item.id })
      toast.success(t('team.deleted'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.deleteFailed'))
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{t('team.title')}</h1>
          <p className="text-sm text-muted">{t('team.subtitle')}</p>
        </div>
        <Button className="gap-2" onPress={() => setOpen(true)}>
          <Plus className="h-4 w-4" /> {t('team.add')}
        </Button>
      </div>

      {admins.error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {admins.error.message}
        </div>
      )}

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('team.list')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label={t('team.list')}>
              <Table.Header>
                <Table.Column isRowHeader>{t('login.email')}</Table.Column>
                <Table.Column>{t('team.role')}</Table.Column>
                <Table.Column>{t('team.createdAt')}</Table.Column>
                <Table.Column className="text-right">{t('common.actions')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {admins.isLoading ? t('common.loading') : t('team.empty')}
                  </div>
                )}
              >
                {(admins.data ?? []).map((item) => (
                  <Table.Row key={item.id} id={item.id}>
                    <Table.Cell className="font-medium">{item.email}</Table.Cell>
                    <Table.Cell>
                      <Chip
                        size="sm"
                        variant="soft"
                        color={item.role === 'admin' ? 'accent' : 'default'}
                      >
                        {item.role}
                      </Chip>
                    </Table.Cell>
                    <Table.Cell className="text-xs text-muted">
                      {formatTime(item.createdAt, locale)}
                    </Table.Cell>
                    <Table.Cell className="text-right">
                      {item.id !== admin?.id && (
                        <Button
                          variant="ghost"
                          size="sm"
                          isIconOnly
                          aria-label={t('team.deleteAria', { name: item.email })}
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
              <Modal.Heading>{t('team.addTitle')}</Modal.Heading>
              <p className="text-sm text-muted">{t('team.addHint')}</p>
            </Modal.Header>
            <Modal.Body>
              <div className="space-y-3">
                <TextField variant="secondary" isRequired fullWidth value={email} onChange={setEmail}>
                  <Label>{t('login.email')}</Label>
                  <Input type="email" />
                </TextField>
                <TextField
                  variant="secondary"
                  isRequired
                  fullWidth
                  value={password}
                  onChange={setPassword}
                >
                  <Label>{t('login.password')}</Label>
                  <Input type="password" />
                </TextField>
                {/* React Aria Select 使用 selectedKey/onSelectionChange 作为受控 API。 */}
                <Select
                  aria-label={t('team.role')}
                  selectedKey={role}
                  onSelectionChange={(key) => setRole(key as Role)}
                >
                  <Label>{t('team.role')}</Label>
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
                {t('common.cancel')}
              </Button>
              <Button
                onPress={() => void create()}
                isDisabled={createAdmin.isPending || !email || password.length < 8}
              >
                {t('common.create')}
              </Button>
            </Modal.Footer>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  )
}
