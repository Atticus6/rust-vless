import { Button, Card, Input, Label, TextField } from '@heroui/react'
import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { toast } from 'sonner'
import { setAuth } from '@/lib/auth'
import { login } from '@/lib/rpc'

export function LoginPage() {
  const navigate = useNavigate()
  const [email, setEmail] = useState('admin@example.com')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(event: React.FormEvent) {
    event.preventDefault()
    setBusy(true)
    try {
      const result = await login(email, password)
      setAuth(result.token, result.admin)
      void navigate({ to: '/' })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '登录失败')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="grid min-h-screen place-items-center bg-[radial-gradient(ellipse_at_top,_oklch(0.62_0.195_253.83_/_0.12),_transparent_55%)] p-4">
      <Card className="w-full max-w-sm">
        <Card.Header>
          <Card.Title className="text-xl">登录 rust-vless</Card.Title>
          <Card.Description>主节点控制台 · 多用户代理管理</Card.Description>
        </Card.Header>
        <Card.Content>
          <form className="space-y-4" onSubmit={submit}>
            <TextField variant="secondary" isRequired fullWidth value={email} onChange={setEmail}>
              <Label>邮箱</Label>
              <Input type="email" autoComplete="username" />
            </TextField>
            <TextField variant="secondary" isRequired fullWidth value={password} onChange={setPassword}>
              <Label>密码</Label>
              <Input type="password" autoComplete="current-password" />
            </TextField>
            <Button type="submit" className="w-full" isDisabled={busy}>
              {busy ? '登录中…' : '登录'}
            </Button>
          </form>
          <p className="mt-4 text-xs text-muted">
            初始账号由主节点首次启动时创建，可通过 RUST_VLESS_ADMIN_EMAIL / RUST_VLESS_ADMIN_PASSWORD 配置。
          </p>
        </Card.Content>
      </Card>
    </div>
  )
}
