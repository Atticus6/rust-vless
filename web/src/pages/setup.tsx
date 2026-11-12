import { Button, Card, Input, Label, TextField } from '@heroui/react'
import { useNavigate } from '@tanstack/react-router'
import { Code } from '@connectrpc/connect'
import { useState } from 'react'
import { toast } from 'sonner'
import { useI18n } from '@/i18n'
import { setAuth } from '@/lib/auth'
import { RpcError } from '@/lib/connect'
import { client } from '@/lib/trpc'

/**
 * 首次安装页面。
 *
 * 主节点没有任何管理员时展示；创建第一个 admin 账号后立即登录。
 * 已有账号时路由会把访问者重定向到登录页。
 */
export function SetupPage() {
  const navigate = useNavigate()
  const { t } = useI18n()
  const [email, setEmail] = useState('admin@example.com')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [masterName, setMasterName] = useState('master')
  const [setupToken, setSetupToken] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(event: React.FormEvent) {
    event.preventDefault()
    if (password !== confirm) {
      toast.error(t('setup.mismatch'))
      return
    }
    setBusy(true)
    try {
      const result = await client.setup({ email, password, masterName, setupToken })
      if (!result.admin) throw new Error(t('login.failed'))
      setAuth(result.token, result.admin)
      void navigate({ to: '/' })
    } catch (error) {
      // 按 gRPC 状态码给出可操作的提示：锁定后用户需要知道如何恢复。
      if (error instanceof RpcError) {
        if (error.code === Code.PermissionDenied) {
          toast.error(t('setup.codeLocked'))
          return
        }
        if (error.code === Code.Unauthenticated) {
          toast.error(t('setup.codeWrong'))
          return
        }
        if (error.code === Code.AlreadyExists) {
          toast.error(t('setup.alreadyInstalled'))
          return
        }
      }
      toast.error(error instanceof Error ? error.message : t('setup.failed'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="grid min-h-screen place-items-center bg-[radial-gradient(ellipse_at_top,_oklch(0.62_0.195_253.83_/_0.12),_transparent_55%)] p-4">
      <Card className="w-full max-w-md">
        <Card.Header>
          <Card.Title className="text-xl">{t('setup.title')}</Card.Title>
          <Card.Description>{t('setup.description')}</Card.Description>
        </Card.Header>
        <Card.Content>
          <form className="space-y-4" onSubmit={submit}>
            <TextField variant="secondary" isRequired fullWidth value={email} onChange={setEmail}>
              <Label>{t('setup.adminEmail')}</Label>
              <Input type="email" autoComplete="username" />
            </TextField>
            <TextField
              variant="secondary"
              isRequired
              fullWidth
              value={password}
              onChange={setPassword}
            >
              <Label>{t('setup.password')}</Label>
              <Input type="password" autoComplete="new-password" />
            </TextField>
            <TextField
              variant="secondary"
              isRequired
              fullWidth
              value={confirm}
              onChange={setConfirm}
            >
              <Label>{t('setup.confirmPassword')}</Label>
              <Input type="password" autoComplete="new-password" />
            </TextField>
            <TextField
              variant="secondary"
              isRequired
              fullWidth
              value={setupToken}
              onChange={setSetupToken}
            >
              <Label>{t('setup.setupCode')}</Label>
              <Input placeholder={t('setup.setupCodePlaceholder')} autoComplete="off" />
            </TextField>
            <TextField variant="secondary" fullWidth value={masterName} onChange={setMasterName}>
              <Label>{t('setup.masterName')}</Label>
              <Input placeholder="master" />
            </TextField>
            <Button
              type="submit"
              className="w-full"
              isDisabled={busy || password.length < 8 || !setupToken.trim()}
            >
              {busy ? t('setup.installing') : t('setup.submit')}
            </Button>
          </form>
          <p className="mt-4 text-xs text-muted">{t('setup.hint')}</p>
        </Card.Content>
      </Card>
    </div>
  )
}
