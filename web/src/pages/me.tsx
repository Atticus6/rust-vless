import { Button, Card, Chip, Separator, Table } from '@heroui/react'
import { Copy, RefreshCw } from 'lucide-react'
import { toast } from 'sonner'
import { useI18n } from '@/i18n'
import { formatBytes, formatRelative } from '@/lib/format'
import { client } from '@/lib/trpc'
import { subscriptionUrl } from '@/lib/connect'

/**
 * 用户个人面板。
 *
 * 展示当前账号的订阅入口（Clash / V2Ray）与代理用户，并列出该账号可见的
 * 流量日志；普通 user 账号只能看到自己名下的数据。
 */
export function MePage() {
  const { t, locale } = useI18n()
  const subscription = client.getSubscription.useQuery()
  const logs = client.listAccessLogs.useQuery({ limit: 20n })

  const v2rayUrl = subscription.data ? subscriptionUrl(subscription.data.v2rayUrl) : ''
  const clashUrl = subscription.data ? subscriptionUrl(subscription.data.clashUrl) : ''

  async function copy(value: string) {
    try {
      await navigator.clipboard.writeText(value)
      toast.success(t('me.copied'))
    } catch {
      window.prompt(t('common.manualCopy'), value)
    }
  }

  const error = subscription.error ?? logs.error

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{t('me.title')}</h1>
          <p className="text-sm text-muted">{t('me.subtitle')}</p>
        </div>
        <Button
          variant="outline"
          className="gap-1"
          isDisabled={subscription.isFetching}
          onPress={() => {
            void subscription.refetch()
            void logs.refetch()
          }}
        >
          <RefreshCw className="h-4 w-4" /> {t('common.refresh')}
        </Button>
      </div>

      {error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {error.message}
        </div>
      )}

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <Card.Header>
            <Card.Title className="text-base">{t('me.v2ray')}</Card.Title>
          </Card.Header>
          <Card.Content className="space-y-3">
            <p className="break-all font-mono text-xs text-muted">{v2rayUrl || '—'}</p>
            <Button
              size="sm"
              variant="secondary"
              className="gap-1"
              isDisabled={!v2rayUrl}
              onPress={() => void copy(v2rayUrl)}
            >
              <Copy className="h-3.5 w-3.5" /> {t('me.copyV2ray')}
            </Button>
          </Card.Content>
        </Card>
        <Card>
          <Card.Header>
            <Card.Title className="text-base">{t('me.clash')}</Card.Title>
          </Card.Header>
          <Card.Content className="space-y-3">
            <p className="break-all font-mono text-xs text-muted">{clashUrl || '—'}</p>
            <Button
              size="sm"
              variant="secondary"
              className="gap-1"
              isDisabled={!clashUrl}
              onPress={() => void copy(clashUrl)}
            >
              <Copy className="h-3.5 w-3.5" /> {t('me.copyClash')}
            </Button>
          </Card.Content>
        </Card>
      </div>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('me.users')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label={t('me.users')}>
              <Table.Header>
                <Table.Column isRowHeader>{t('common.name')}</Table.Column>
                <Table.Column>UUID</Table.Column>
                <Table.Column>{t('common.status')}</Table.Column>
                <Table.Column>{t('common.upload')}</Table.Column>
                <Table.Column>{t('common.download')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {subscription.isLoading ? t('common.loading') : t('users.empty')}
                  </div>
                )}
              >
                {(subscription.data?.users ?? []).map((user) => (
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
                    <Table.Cell className="text-xs">{formatBytes(user.upBytes)}</Table.Cell>
                    <Table.Cell className="text-xs">{formatBytes(user.downBytes)}</Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table>
        </Card.Content>
      </Card>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('me.logs')}</Card.Title>
        </Card.Header>
        <Card.Content className="space-y-3">
          <Table>
            <Table.Content aria-label={t('me.logs')}>
              <Table.Header>
                <Table.Column isRowHeader>{t('common.time')}</Table.Column>
                <Table.Column>{t('traffic.entryNode')}</Table.Column>
                <Table.Column>{t('common.target')}</Table.Column>
                <Table.Column>{t('common.protocol')}</Table.Column>
                <Table.Column>{t('common.duration')}</Table.Column>
                <Table.Column>{t('common.upload')}</Table.Column>
                <Table.Column>{t('common.download')}</Table.Column>
                <Table.Column>{t('common.status')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {logs.isLoading ? t('common.loading') : t('traffic.empty')}
                  </div>
                )}
              >
                {(logs.data?.items ?? []).map((log) => (
                  <Table.Row key={log.id} id={String(log.id)}>
                    <Table.Cell className="whitespace-nowrap text-xs text-muted">
                      {formatRelative(log.startedAt, locale)}
                    </Table.Cell>
                    <Table.Cell className="text-xs">{log.entryNodeId.slice(0, 8)}…</Table.Cell>
                    <Table.Cell className="max-w-48 truncate font-mono text-xs">
                      {log.target}
                    </Table.Cell>
                    <Table.Cell className="text-xs uppercase">{log.command}</Table.Cell>
                    <Table.Cell className="text-xs">{log.durationMs} ms</Table.Cell>
                    <Table.Cell className="text-xs">{formatBytes(log.upBytes)}</Table.Cell>
                    <Table.Cell className="text-xs">{formatBytes(log.downBytes)}</Table.Cell>
                    <Table.Cell>
                      <Chip
                        size="sm"
                        variant="soft"
                        color={
                          log.status === 'ok'
                            ? 'success'
                            : log.status === 'denied'
                              ? 'warning'
                              : 'danger'
                        }
                      >
                        {log.status}
                      </Chip>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table>
          <p className="text-xs text-muted">{t('me.total', { total: logs.data?.total ?? 0 })}</p>
        </Card.Content>
      </Card>
      <Separator />
    </div>
  )
}
