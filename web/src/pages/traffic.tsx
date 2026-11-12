import {
  Button,
  Card,
  Chip,
  Input,
  Label,
  ListBox,
  Modal,
  Select,
  Separator,
  Table,
} from '@heroui/react'
import {
  Background,
  Controls,
  MarkerType,
  ReactFlow,
  type Edge,
  type Node,
} from '@xyflow/react'
import { Filter, RefreshCw } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useI18n } from '@/i18n'
import { formatBytes, formatRelative, formatTime } from '@/lib/format'
import { client } from '@/lib/trpc'


const WINDOWS = [
  { label: 'traffic.window1h', value: 3600 },
  { label: 'traffic.window24h', value: 86400 },
  { label: 'traffic.window7d', value: 604800 },
] as const

/** React Aria 的下拉框不能使用空 key，这里用显式哨兵表示“全部”。 */
const ALL = '__all__'
const PAGE_SIZE = 25

const nodeStyle = {
  background: 'var(--surface)',
  color: 'var(--foreground)',
  border: '1px solid var(--border)',
  borderRadius: 10,
  width: 180,
  fontSize: 12,
  padding: 4,
} satisfies React.CSSProperties

const STATUSES = ['ok', 'error', 'denied', 'timeout'] as const

export function TrafficPage() {
  const { t, locale } = useI18n()
  const [windowSec, setWindowSec] = useState<number>(3600)
  const [offset, setOffset] = useState(0)
  const [userId, setUserId] = useState(ALL)
  const [nodeId, setNodeId] = useState(ALL)
  const [status, setStatus] = useState(ALL)
  const [targetInput, setTargetInput] = useState('')
  const [target, setTarget] = useState('')
  const [selected, setSelected] = useState<string | null>(null)

  const filter = {
    userId: userId === ALL ? undefined : userId,
    nodeId: nodeId === ALL ? undefined : nodeId,
    status: status === ALL ? undefined : status,
    target: target || undefined,
    limit: BigInt(PAGE_SIZE),
    offset: BigInt(offset),
  }

  const topology = client.getTrafficTopology.useQuery({ window: BigInt(windowSec) })
  const logs = client.listAccessLogs.useQuery(filter)
  const users = client.listUsers.useQuery()

  const nodes = useMemo<Node[]>(
    () =>
      (topology.data?.nodes ?? []).map((node, index) => ({
        id: node.id,
        position: { x: (index % 3) * 290, y: Math.floor(index / 3) * 170 },
        data: {
          label: `${node.name}${node.isMaster && node.name.toLowerCase() !== 'master' ? ' · master' : ''}${node.online ? '' : ' · offline'}`,
        },
        style: { ...nodeStyle, opacity: node.online ? 1 : 0.55 },
      })),
    [topology.data?.nodes],
  )

  const edges = useMemo<Edge[]>(
    () =>
      (topology.data?.links ?? []).map((link) => {
        const total = link.upBytes + link.downBytes
        return {
          id: link.id,
          source: link.fromNodeId,
          target: link.toNodeId,
          label: `${formatBytes(total)} · ${link.sessions} ${t('traffic.relayedSessions')}`,
          markerEnd: { type: MarkerType.ArrowClosed },
          animated: link.sessions > 0,
          style: {
            stroke: 'oklch(0.6204 0.195 253.83)',
            strokeWidth: Math.min(6, 1 + Math.log10(total + 1)),
          },
          labelStyle: { fill: 'var(--muted)', fontSize: 11 },
        } satisfies Edge
      }),
    [topology.data?.links, t],
  )

  function nodeName(id: string): string {
    return topology.data?.nodes.find((node) => node.id === id)?.name ?? id.slice(0, 8)
  }

  const totalBytes =
    topology.data?.links.reduce((sum, link) => sum + link.upBytes + link.downBytes, 0) ?? 0
  const totalSessions = topology.data?.links.reduce((sum, link) => sum + link.sessions, 0) ?? 0

  const userItems = [
    { id: ALL, label: t('common.all') },
    ...(users.data ?? []).map((user) => ({ id: user.id, label: user.name })),
  ]
  const nodeItems = [
    { id: ALL, label: t('common.all') },
    ...(topology.data?.nodes ?? []).map((node) => ({ id: node.id, label: node.name })),
  ]
  const statusItems = [
    { id: ALL, label: t('common.all') },
    ...STATUSES.map((value) => ({ id: value, label: value })),
  ]

  const detail = logs.data?.items.find((item) => String(item.id) === selected) ?? null
  const error = topology.error ?? logs.error

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">{t('traffic.title')}</h1>
          <p className="text-sm text-muted">{t('traffic.subtitle')}</p>
        </div>
        <div className="flex items-center gap-2">
          {/* React Aria 的受控 API 是 selectedKey/onSelectionChange（已在浏览器
              验证回显与查询会更新），不要按旧版文档改成 value/onChange。 */}
          <Select
            aria-label={t('traffic.window')}
            className="w-40"
            selectedKey={String(windowSec)}
            onSelectionChange={(key) => setWindowSec(Number(key))}
          >
            <Select.Trigger>
              <Select.Value />
              <Select.Indicator />
            </Select.Trigger>
            <Select.Popover>
              <ListBox>
                {WINDOWS.map((item) => (
                  <ListBox.Item key={item.value} id={String(item.value)} textValue={t(item.label)}>
                    {t(item.label)}
                    <ListBox.ItemIndicator />
                  </ListBox.Item>
                ))}
              </ListBox>
            </Select.Popover>
          </Select>
          <Button
            variant="outline"
            className="gap-1"
            onPress={() => {
              void topology.refetch()
              void logs.refetch()
            }}
          >
            <RefreshCw className="h-4 w-4" /> {t('common.refresh')}
          </Button>
        </div>
      </div>

      {error && (
        <div className="rounded-md border border-danger/40 bg-danger/10 p-3 text-sm text-danger">
          {error.message}
        </div>
      )}

      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <Card.Header className="pb-2">
            <Card.Title className="text-sm font-medium text-muted">
              {t('traffic.windowTraffic')}
            </Card.Title>
          </Card.Header>
          <Card.Content>
            <div className="text-2xl font-semibold">{formatBytes(totalBytes)}</div>
          </Card.Content>
        </Card>
        <Card>
          <Card.Header className="pb-2">
            <Card.Title className="text-sm font-medium text-muted">
              {t('traffic.relayedSessions')}
            </Card.Title>
          </Card.Header>
          <Card.Content>
            <div className="text-2xl font-semibold">{totalSessions}</div>
          </Card.Content>
        </Card>
        <Card>
          <Card.Header className="pb-2">
            <Card.Title className="text-sm font-medium text-muted">
              {t('traffic.topologyNodes')}
            </Card.Title>
          </Card.Header>
          <Card.Content>
            <div className="text-2xl font-semibold">{topology.data?.nodes.length ?? 0}</div>
            <p className="text-xs text-muted">
              {topology.data?.links.length ?? 0} {t('common.links')}
            </p>
          </Card.Content>
        </Card>
      </div>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('traffic.topology')}</Card.Title>
        </Card.Header>
        <Card.Content>
          <div className="h-[460px] w-full overflow-hidden rounded-lg border border-border bg-background">
            <ReactFlow
              key={windowSec}
              nodes={nodes}
              edges={edges}
              fitView
              nodesDraggable={false}
              nodesConnectable={false}
              elementsSelectable={false}
              proOptions={{ hideAttribution: true }}
              colorMode="dark"
            >
              <Background gap={20} size={1} />
              <Controls showInteractive={false} />
            </ReactFlow>
          </div>
        </Card.Content>
      </Card>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">{t('traffic.logs')}</Card.Title>
        </Card.Header>
        <Card.Content className="space-y-4">
          <div className="grid gap-3 md:grid-cols-5">
            <div className="space-y-1">
              <Select
                aria-label={t('traffic.user')}
                selectedKey={userId}
                onSelectionChange={(key) => {
                  setOffset(0)
                  setUserId(key as string)
                }}
              >
                <Label className="text-xs">{t('traffic.user')}</Label>
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {userItems.map((item) => (
                      <ListBox.Item key={item.id} id={item.id} textValue={item.label}>
                        {item.label}
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
            </div>
            <div className="space-y-1">
              <Select
                aria-label={t('traffic.entryNode')}
                selectedKey={nodeId}
                onSelectionChange={(key) => {
                  setOffset(0)
                  setNodeId(key as string)
                }}
              >
                <Label className="text-xs">{t('traffic.entryNode')}</Label>
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {nodeItems.map((item) => (
                      <ListBox.Item key={item.id} id={item.id} textValue={item.label}>
                        {item.label}
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
            </div>
            <div className="space-y-1">
              <Select
                aria-label={t('common.status')}
                selectedKey={status}
                onSelectionChange={(key) => {
                  setOffset(0)
                  setStatus(key as string)
                }}
              >
                <Label className="text-xs">{t('common.status')}</Label>
                <Select.Trigger>
                  <Select.Value />
                  <Select.Indicator />
                </Select.Trigger>
                <Select.Popover>
                  <ListBox>
                    {statusItems.map((item) => (
                      <ListBox.Item key={item.id} id={item.id} textValue={item.label}>
                        {item.label}
                        <ListBox.ItemIndicator />
                      </ListBox.Item>
                    ))}
                  </ListBox>
                </Select.Popover>
              </Select>
            </div>
            <div className="space-y-1 md:col-span-2">
              <Label className="text-xs">{t('traffic.targetKeyword')}</Label>
              <div className="flex gap-2">
                <Input
                  aria-label={t('traffic.targetKeyword')}
                  variant="secondary"
                  className="flex-1"
                  value={targetInput}
                  onChange={(event) => setTargetInput(event.target.value)}
                  placeholder="example.com:443"
                />
                <Button
                  variant="outline"
                  className="shrink-0 gap-1"
                  onPress={() => {
                    setOffset(0)
                    setTarget(targetInput.trim())
                  }}
                >
                  <Filter className="h-4 w-4" /> {t('traffic.filter')}
                </Button>
              </div>
            </div>
          </div>

          <Separator />

          <Table>
            <Table.Content
              aria-label={t('traffic.logs')}
              // 行点击打开详情；React Aria 回调提供的是行 key（日志 id 字符串）。
              onRowAction={(key) => setSelected(String(key))}
            >
              <Table.Header>
                <Table.Column isRowHeader>{t('common.time')}</Table.Column>
                <Table.Column>{t('traffic.user')}</Table.Column>
                <Table.Column>{t('traffic.entryNode')}</Table.Column>
                <Table.Column>{t('common.target')}</Table.Column>
                <Table.Column>{t('common.protocol')}</Table.Column>
                <Table.Column>{t('common.duration')}</Table.Column>
                <Table.Column>{t('common.upload')}</Table.Column>
                <Table.Column>{t('common.download')}</Table.Column>
                <Table.Column>{t('common.status')}</Table.Column>
                <Table.Column>{t('common.path')}</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {logs.isLoading ? t('common.loading') : t('traffic.empty')}
                  </div>
                )}
              >
                {(logs.data?.items ?? []).map((log) => (
                  <Table.Row key={log.id} id={String(log.id)} className="cursor-pointer">
                    <Table.Cell className="whitespace-nowrap text-xs text-muted">
                      {formatRelative(log.startedAt, locale)}
                    </Table.Cell>
                    <Table.Cell className="font-mono text-xs">{log.userId.slice(0, 8)}…</Table.Cell>
                    <Table.Cell className="text-xs">{nodeName(log.entryNodeId)}</Table.Cell>
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
                    <Table.Cell>
                      <div className="flex flex-wrap gap-1">
                        {log.path.map((id) => (
                          <Chip key={id} size="sm" variant="soft" color="default">
                            {nodeName(id)}
                          </Chip>
                        ))}
                      </div>
                    </Table.Cell>
                  </Table.Row>
                ))}
              </Table.Body>
            </Table.Content>
          </Table>

          <div className="flex items-center justify-between text-xs text-muted">
            <span>{t('traffic.total', { total: logs.data?.total ?? 0 })}</span>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                isDisabled={offset === 0}
                onPress={() => setOffset(Math.max(0, offset - PAGE_SIZE))}
              >
                {t('traffic.prev')}
              </Button>
              <span>
                {Math.floor(offset / PAGE_SIZE) + 1} /{' '}
                {Math.max(1, Math.ceil((logs.data?.total ?? 0) / PAGE_SIZE))}
              </span>
              <Button
                variant="outline"
                size="sm"
                isDisabled={offset + PAGE_SIZE >= (logs.data?.total ?? 0)}
                onPress={() => setOffset(offset + PAGE_SIZE)}
              >
                {t('traffic.next')}
              </Button>
            </div>
          </div>
        </Card.Content>
      </Card>

      <Modal.Backdrop isOpen={Boolean(detail)} onOpenChange={(open) => !open && setSelected(null)}>
        <Modal.Container>
          <Modal.Dialog className="sm:max-w-xl">
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>{t('traffic.detail')}</Modal.Heading>
              <p className="text-sm text-muted">
                {detail ? formatTime(detail.startedAt, locale) : ''}
              </p>
            </Modal.Header>
            <Modal.Body>
              {detail && (
                <div className="space-y-3 text-sm">
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <span className="text-muted">{t('traffic.labelUser')}</span>
                      {detail.userId}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelEntry')}</span>
                      {nodeName(detail.entryNodeId)}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelTarget')}</span>
                      {detail.target}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelProtocol')}</span>
                      {detail.command}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelDuration')}</span>
                      {detail.durationMs} ms
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelStatus')}</span>
                      {detail.status}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelUpload')}</span>
                      {formatBytes(detail.upBytes)}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelDownload')}</span>
                      {formatBytes(detail.downBytes)}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelClient')}</span>
                      {detail.clientAddr || '—'}
                    </div>
                    <div>
                      <span className="text-muted">{t('traffic.labelSession')}</span>
                      {detail.sessionId}
                    </div>
                  </div>
                  <div>
                    <div className="mb-1 text-muted">{t('common.path')}</div>
                    <div className="flex flex-wrap gap-1">
                      {detail.path.map((id) => (
                        <Chip key={id} size="sm" variant="soft" color="default">
                          {nodeName(id)}
                        </Chip>
                      ))}
                    </div>
                  </div>
                  {detail.error && (
                    <div className="rounded-md border border-danger/40 bg-danger/10 p-2 text-xs text-danger">
                      {detail.error}
                    </div>
                  )}
                </div>
              )}
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  )
}
