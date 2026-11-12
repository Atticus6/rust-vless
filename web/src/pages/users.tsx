import { Button, Card, Chip, Input, Label, Modal, Table, TextField } from '@heroui/react'
import { Copy, Pencil, Plus, Power, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'
import { useI18n } from '@/i18n'
import { formatBytes } from '@/lib/format'
import { client } from '@/lib/trpc'
import type { ProxyUserView } from '@/lib/types'

export function UsersPage() {
  const { t } = useI18n()
  const users = client.listUsers.useQuery()
  const createUser = client.createUser.useMutation()
  const updateUser = client.updateUser.useMutation()
  const deleteUser = client.deleteUser.useMutation()

  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [editing, setEditing] = useState<ProxyUserView | null>(null)
  const [editName, setEditName] = useState('')

  async function create() {
    if (!name.trim()) return
    try {
      await createUser.mutateAsync({ name })
      setName('')
      setOpen(false)
      toast.success(t('users.created'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.createFailed'))
    }
  }

  async function saveEdit() {
    if (!editing || !editName.trim()) return
    try {
      await updateUser.mutateAsync({ id: editing.id, name: editName })
      setEditing(null)
      toast.success(t('users.renamed'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.updateFailed'))
    }
  }

  async function toggle(user: ProxyUserView) {
    try {
      await updateUser.mutateAsync({ id: user.id, enabled: !user.enabled })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.updateFailed'))
    }
  }

  async function remove(user: ProxyUserView) {
    if (!window.confirm(t('users.deleteConfirm', { name: user.name }))) return
    try {
      await deleteUser.mutateAsync({ id: user.id })
      toast.success(t('users.deleted'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('common.deleteFailed'))
    }
  }

  async function copy(id: string) {
    await navigator.clipboard.writeText(id)
    toast.success(t('users.uuidCopied'))
  }

  return (
    <div className="space-y-6">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{t('users.title')}</h1>
          <p className="text-sm text-muted">{t('users.subtitle')}</p>
        </div>
        <Button className="gap-2" onPress={() => setOpen(true)}>
          <Plus className="h-4 w-4" /> {t('users.add')}
        </Button>
      </div>

      {users.error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {users.error.message}
        </div>
      )}

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('users.list')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label={t('users.list')}>
              <Table.Header>
                <Table.Column isRowHeader>{t('common.name')}</Table.Column>
                <Table.Column>UUID</Table.Column>
                <Table.Column>{t('common.status')}</Table.Column>
                <Table.Column>{t('users.owner')}</Table.Column>
                <Table.Column>{t('common.upload')}</Table.Column>
                <Table.Column>{t('common.download')}</Table.Column>
                <Table.Column className="text-right">{t('common.actions')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {users.isLoading ? t('common.loading') : t('users.empty')}
                  </div>
                )}
              >
                {(users.data ?? []).map((user) => (
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
                        {user.enabled ? t('common.enabled') : t('common.disabled')}
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
                          aria-label={t('users.editAria', { name: user.name })}
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
                          aria-label={
                            user.enabled
                              ? t('users.disableAria', { name: user.name })
                              : t('users.enableAria', { name: user.name })
                          }
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
                          aria-label={t('users.deleteAria', { name: user.name })}
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
              <Modal.Heading>{t('users.editTitle')}</Modal.Heading>
              <p className="text-sm text-muted">{t('users.editHint')}</p>
            </Modal.Header>
            <Modal.Body>
              <TextField variant="secondary" fullWidth value={editName} onChange={setEditName}>
                <Label>{t('common.name')}</Label>
                <Input />
              </TextField>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                {t('common.cancel')}
              </Button>
              <Button
                onPress={() => void saveEdit()}
                isDisabled={updateUser.isPending || !editName.trim()}
              >
                {t('common.save')}
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
              <Modal.Heading>{t('users.addTitle')}</Modal.Heading>
              <p className="text-sm text-muted">{t('users.addHint')}</p>
            </Modal.Header>
            <Modal.Body>
              <TextField variant="secondary" fullWidth value={name} onChange={setName}>
                <Label>{t('common.name')}</Label>
                <Input placeholder="phone" />
              </TextField>
            </Modal.Body>
            <Modal.Footer>
              <Button slot="close" variant="secondary">
                {t('common.cancel')}
              </Button>
              <Button
                onPress={() => void create()}
                isDisabled={createUser.isPending || !name.trim()}
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
