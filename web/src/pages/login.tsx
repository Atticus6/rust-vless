import { Button, Card, Input, Label, TextField } from '@heroui/react'
import { useNavigate } from '@tanstack/react-router'
import { useState } from 'react'
import { toast } from 'sonner'
import { useI18n } from '@/i18n'
import { setAuth } from '@/lib/auth'
import { client } from '@/lib/trpc'

export function LoginPage() {
  const navigate = useNavigate()
  const { t } = useI18n()
  const [email, setEmail] = useState('admin@example.com')
  const [password, setPassword] = useState('')
  const [busy, setBusy] = useState(false)

  async function submit(event: React.FormEvent) {
    event.preventDefault()
    setBusy(true)
    try {
      const result = await client.login({ email, password })
      if (!result.admin) throw new Error(t('login.failed'))
      setAuth(result.token, result.admin)
      void navigate({ to: '/' })
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('login.failed'))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="grid min-h-screen place-items-center bg-[radial-gradient(ellipse_at_top,_oklch(0.62_0.195_253.83_/_0.12),_transparent_55%)] p-4">
      <Card className="w-full max-w-sm">
        <Card.Header>
          <Card.Title className="text-xl">{t('login.title')}</Card.Title>
          <Card.Description>{t('login.subtitle')}</Card.Description>
        </Card.Header>
        <Card.Content>
          <form className="space-y-4" onSubmit={submit}>
            <TextField variant="secondary" isRequired fullWidth value={email} onChange={setEmail}>
              <Label>{t('login.email')}</Label>
              <Input type="email" autoComplete="username" />
            </TextField>
            <TextField
              variant="secondary"
              isRequired
              fullWidth
              value={password}
              onChange={setPassword}
            >
              <Label>{t('login.password')}</Label>
              <Input type="password" autoComplete="current-password" />
            </TextField>
            <Button type="submit" className="w-full" isDisabled={busy}>
              {busy ? t('login.submitting') : t('login.submit')}
            </Button>
          </form>
          <p className="mt-4 text-xs text-muted">{t('login.hint')}</p>
        </Card.Content>
      </Card>
    </div>
  )
}
