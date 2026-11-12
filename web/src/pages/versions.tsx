import { Button, Card, Chip, Table } from '@heroui/react'
import { RefreshCw, UploadCloud } from 'lucide-react'
import { toast } from 'sonner'
import { useI18n } from '@/i18n'
import { getAdmin } from '@/lib/auth'
import { client } from '@/lib/trpc'

/**
 * 版本管理页（仅 admin）。
 *
 * 展示主节点与各节点版本，并支持一键通知所有在线节点升级。
 * 「投递成功」只代表指令进入节点通道，最终版本以节点重连后上报为准。
 */
export function VersionsPage() {
  const { t } = useI18n()
  const admin = getAdmin()
  const isAdmin = admin?.role === 'admin'
  const versions = client.getVersionInfo.useQuery(undefined, { enabled: isAdmin })
  const updateAll = client.updateAllNodes.useMutation()

  if (!isAdmin) {
    return (
      <div className="rounded-lg border border-border bg-surface p-8 text-center text-sm text-muted">
        {t('versions.adminOnly')}
      </div>
    )
  }

  const info = versions.data

  async function updateAllNodes() {
    try {
      const result = await updateAll.mutateAsync({ version: info?.latestVersion ?? '' })
      toast.success(
        t('versions.updateSent', { online: result.online, requested: result.requested }),
      )
    } catch (error) {
      toast.error(error instanceof Error ? error.message : t('versions.updateFailed'))
    }
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{t('versions.title')}</h1>
          <p className="text-sm text-muted">
            {t('versions.summary', {
              master: info?.masterVersion ?? '—',
              latest: info?.latestVersion ?? '—',
            })}
          </p>
        </div>
        <div className="flex gap-2">
          <Button variant="outline" className="gap-1" onPress={() => void versions.refetch()}>
            <RefreshCw className="h-4 w-4" /> {t('common.refresh')}
          </Button>
          <Button
            className="gap-1"
            isDisabled={updateAll.isPending || !info}
            onPress={() => void updateAllNodes()}
          >
            <UploadCloud className="h-4 w-4" /> {t('versions.updateAll')}
          </Button>
        </div>
      </div>

      {versions.error && (
        <div className="flex items-center justify-between gap-3 rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          <span>{versions.error.message}</span>
          <Button size="sm" variant="outline" onPress={() => void versions.refetch()}>
            {t('common.retry')}
          </Button>
        </div>
      )}

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('versions.list')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <Table>
            <Table.Content aria-label={t('versions.list')}>
              <Table.Header>
                <Table.Column isRowHeader>{t('versions.node')}</Table.Column>
                <Table.Column>{t('common.status')}</Table.Column>
                <Table.Column>{t('versions.current')}</Table.Column>
                <Table.Column>{t('versions.target')}</Table.Column>
                <Table.Column>{t('versions.recommendation')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {versions.isLoading ? t('common.loading') : t('versions.empty')}
                  </div>
                )}
              >
                {(info?.nodes ?? []).map((node) => {
                  const outdated = node.version !== info?.latestVersion
                  return (
                    <Table.Row key={node.nodeId} id={node.nodeId}>
                      <Table.Cell className="font-medium">{node.name}</Table.Cell>
                      <Table.Cell>
                        <Chip size="sm" variant="soft" color={node.online ? 'success' : 'default'}>
                          {node.online ? t('common.online') : t('common.offline')}
                        </Chip>
                      </Table.Cell>
                      <Table.Cell className="font-mono text-xs">{node.version || '—'}</Table.Cell>
                      <Table.Cell className="font-mono text-xs">{info?.latestVersion}</Table.Cell>
                      <Table.Cell>
                        <Chip size="sm" variant="soft" color={outdated ? 'warning' : 'success'}>
                          {outdated ? t('versions.updateNeeded') : t('versions.upToDate')}
                        </Chip>
                      </Table.Cell>
                    </Table.Row>
                  )
                })}
              </Table.Body>
            </Table.Content>
          </Table>
        </Card.Content>
      </Card>
    </div>
  )
}
