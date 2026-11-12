import { Copy, Pencil, Plus, Power, Trash2 } from 'lucide-react'
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
import { formatBytes } from '@/lib/format'
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
      setUsers(await api<ProxyUserView[]>('/api/users'))
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
      await api('/api/users', { method: 'POST', body: JSON.stringify({ name }) })
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
      await api(`/api/users/${editing.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ name: editName }),
      })
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
      await api(`/api/users/${user.id}`, {
        method: 'PATCH',
        body: JSON.stringify({ enabled: !user.enabled }),
      })
      await load()
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '更新失败')
    }
  }

  async function remove(user: ProxyUserView) {
    if (!window.confirm(`删除代理用户 ${user.name}？`)) return
    try {
      await api(`/api/users/${user.id}`, { method: 'DELETE' })
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
          <p className="text-sm text-muted-foreground">VLESS UUID 用户；启用/禁用会实时同步到所有节点</p>
        </div>
        <Button className="gap-2" onClick={() => setOpen(true)}>
          <Plus className="h-4 w-4" /> 添加用户
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">用户列表</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>名称</TableHead>
                <TableHead>UUID</TableHead>
                <TableHead>状态</TableHead>
                <TableHead>所有者</TableHead>
                <TableHead>上行</TableHead>
                <TableHead>下行</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {users.map((user) => (
                <TableRow key={user.id}>
                  <TableCell className="font-medium">{user.name}</TableCell>
                  <TableCell>
                    <button className="flex max-w-64 items-start gap-1 text-left font-mono text-[11px] text-muted-foreground hover:text-foreground" onClick={() => void copy(user.id)}>
                      <span className="break-all">{user.id}</span>
                      <Copy className="mt-0.5 h-3 w-3 shrink-0" />
                    </button>
                  </TableCell>
                  <TableCell>
                    <Badge variant={user.enabled ? 'success' : 'outline'}>{user.enabled ? '启用' : '禁用'}</Badge>
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">{user.ownerEmail}</TableCell>
                  <TableCell className="text-xs">{formatBytes(user.upBytes)}</TableCell>
                  <TableCell className="text-xs">{formatBytes(user.downBytes)}</TableCell>
                  <TableCell className="text-right">
                    <div className="flex justify-end gap-1">
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => {
                          setEditing(user)
                          setEditName(user.name)
                        }}
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button variant="ghost" size="icon" onClick={() => void toggle(user)}>
                        <Power className={user.enabled ? 'h-4 w-4 text-emerald-400' : 'h-4 w-4 text-muted-foreground'} />
                      </Button>
                      <Button variant="ghost" size="icon" onClick={() => void remove(user)}>
                        <Trash2 className="h-4 w-4 text-destructive" />
                      </Button>
                    </div>
                  </TableCell>
                </TableRow>
              ))}
              {users.length === 0 && (
                <TableRow>
                  <TableCell colSpan={7} className="py-8 text-center text-sm text-muted-foreground">
                    还没有代理用户
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Dialog open={Boolean(editing)} onOpenChange={(value) => !value && setEditing(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>编辑代理用户</DialogTitle>
            <DialogDescription>只修改名称，UUID 保持不变，客户端无需重新配置。</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="edit-user-name">名称</Label>
            <Input id="edit-user-name" value={editName} onChange={(event) => setEditName(event.target.value)} />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEditing(null)}>取消</Button>
            <Button onClick={() => void saveEdit()} disabled={busy || !editName.trim()}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>添加代理用户</DialogTitle>
            <DialogDescription>系统会生成一个 VLESS UUID，客户端用它完成鉴权。</DialogDescription>
          </DialogHeader>
          <div className="space-y-2">
            <Label htmlFor="user-name">名称</Label>
            <Input id="user-name" value={name} onChange={(event) => setName(event.target.value)} placeholder="phone" />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>取消</Button>
            <Button onClick={() => void create()} disabled={busy || !name.trim()}>创建</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
