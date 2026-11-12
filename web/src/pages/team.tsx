import { Plus, Trash2 } from 'lucide-react'
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
import { Select } from '@/components/ui/select'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { api } from '@/lib/api'
import { getAdmin } from '@/lib/auth'
import { formatTime } from '@/lib/format'
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
      setAdmins(await api<AdminView[]>('/api/admins'))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载账号失败')
    }
  }, [admin?.role])

  useEffect(() => {
    void load()
  }, [load])

  if (admin?.role !== 'admin') {
    return (
      <div className="rounded-lg border bg-card p-8 text-center text-sm text-muted-foreground">
        只有 admin 账号可以管理团队。
      </div>
    )
  }

  async function create() {
    setBusy(true)
    try {
      await api('/api/admins', {
        method: 'POST',
        body: JSON.stringify({ email, password, role }),
      })
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
      await api(`/api/admins/${item.id}`, { method: 'DELETE' })
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
          <p className="text-sm text-muted-foreground">admin 管理全部资源；user 只能管理自己名下的代理用户</p>
        </div>
        <Button className="gap-2" onClick={() => setOpen(true)}>
          <Plus className="h-4 w-4" /> 添加账号
        </Button>
      </div>

      <Card>
        <CardHeader>
          <CardTitle className="text-base">管理账号</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>邮箱</TableHead>
                <TableHead>角色</TableHead>
                <TableHead>创建时间</TableHead>
                <TableHead className="text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {admins.map((item) => (
                <TableRow key={item.id}>
                  <TableCell className="font-medium">{item.email}</TableCell>
                  <TableCell>
                    <Badge variant={item.role === 'admin' ? 'default' : 'secondary'}>{item.role}</Badge>
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">{formatTime(item.createdAt)}</TableCell>
                  <TableCell className="text-right">
                    {item.id !== admin?.id && (
                      <Button variant="ghost" size="icon" onClick={() => void remove(item)}>
                        <Trash2 className="h-4 w-4 text-destructive" />
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>添加管理账号</DialogTitle>
            <DialogDescription>密码至少 8 位。</DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            <div className="space-y-2">
              <Label htmlFor="team-email">邮箱</Label>
              <Input id="team-email" type="email" value={email} onChange={(event) => setEmail(event.target.value)} />
            </div>
            <div className="space-y-2">
              <Label htmlFor="team-password">密码</Label>
              <Input id="team-password" type="password" value={password} onChange={(event) => setPassword(event.target.value)} />
            </div>
            <div className="space-y-2">
              <Label>角色</Label>
              <Select value={role} onChange={(event) => setRole(event.target.value as Role)}>
                <option value="user">user</option>
                <option value="admin">admin</option>
              </Select>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>取消</Button>
            <Button onClick={() => void create()} disabled={busy || !email || password.length < 8}>创建</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
