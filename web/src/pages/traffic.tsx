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
import { useCallback, useEffect, useMemo, useState } from 'react'
import { toast } from 'sonner'
import { formatBytes, formatRelative, formatTime } from '@/lib/format'
import { accessLogs, listUsers, trafficTopology } from '@/lib/rpc'
import type { AccessLogPage, AccessLogView, ProxyUserView, TrafficTopology } from '@/lib/types'

const WINDOWS = [
  { label: '1 小时', value: 3600 },
  { label: '24 小时', value: 86400 },
  { label: '7 天', value: 604800 },
]

/** React Aria selects cannot use an empty key, so "all" gets an explicit sentinel. */
const ALL = '__all__'
const ALL_ITEMS = [{ id: ALL, label: '全部' }] as const

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
  const [windowSec, setWindowSec] = useState(3600)
  const [topology, setTopology] = useState<TrafficTopology | null>(null)
  const [logs, setLogs] = useState<AccessLogPage | null>(null)
  const [users, setUsers] = useState<ProxyUserView[]>([])
  const [selected, setSelected] = useState<AccessLogView | null>(null)
  const [loading, setLoading] = useState(false)
  const [refresh, setRefresh] = useState(0)

  const [userId, setUserId] = useState(ALL)
  const [nodeId, setNodeId] = useState(ALL)
  const [status, setStatus] = useState(ALL)
  const [targetInput, setTargetInput] = useState('')
  const [target, setTarget] = useState('')
  const [offset, setOffset] = useState(0)
  const limit = 25

  const loadTopology = useCallback(async () => {
    try {
      setTopology(await trafficTopology(windowSec))
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载拓扑失败')
    }
  }, [windowSec])

  const loadLogs = useCallback(async () => {
    setLoading(true)
    try {
      setLogs(
        await accessLogs({
          userId: userId === ALL ? undefined : userId,
          nodeId: nodeId === ALL ? undefined : nodeId,
          status: status === ALL ? undefined : status,
          target: target || undefined,
          limit,
          offset,
        }),
      )
    } catch (error) {
      toast.error(error instanceof Error ? error.message : '加载访问记录失败')
    } finally {
      setLoading(false)
    }
  }, [userId, nodeId, status, target, offset])

  useEffect(() => {
    void loadTopology()
  }, [loadTopology])

  useEffect(() => {
    void loadLogs()
  }, [loadLogs, refresh])

  useEffect(() => {
    void listUsers()
      .then(setUsers)
      .catch(() => setUsers([]))
  }, [])

  const nodes = useMemo<Node[]>(
    () =>
      (topology?.nodes ?? []).map((node, index) => ({
        id: node.id,
        position: { x: (index % 3) * 290, y: Math.floor(index / 3) * 170 },
        data: {
          label: `${node.name}${node.isMaster && node.name.toLowerCase() !== 'master' ? ' · master' : ''}${node.online ? '' : ' · offline'}`,
        },
        style: {
          ...nodeStyle,
          opacity: node.online ? 1 : 0.55,
        },
      })),
    [topology?.nodes],
  )

  const edges = useMemo<Edge[]>(
    () =>
      (topology?.links ?? []).map((link) => {
        const total = link.upBytes + link.downBytes
        return {
          id: link.id,
          source: link.fromNodeId,
          target: link.toNodeId,
          label: `${formatBytes(total)} · ${link.sessions} 会话`,
          markerEnd: { type: MarkerType.ArrowClosed },
          animated: link.sessions > 0,
          style: {
            stroke: 'oklch(0.6204 0.195 253.83)',
            strokeWidth: Math.min(6, 1 + Math.log10(total + 1)),
          },
          labelStyle: { fill: 'var(--muted)', fontSize: 11 },
        } satisfies Edge
      }),
    [topology?.links],
  )

  function nodeName(id: string): string {
    return topology?.nodes.find((node) => node.id === id)?.name ?? id.slice(0, 8)
  }

  function applyFilters() {
    setOffset(0)
    setTarget(targetInput.trim())
    setRefresh((value) => value + 1)
  }

  const totalBytes =
    topology?.links.reduce((sum, link) => sum + link.upBytes + link.downBytes, 0) ?? 0
  const totalSessions = topology?.links.reduce((sum, link) => sum + link.sessions, 0) ?? 0

  const userItems = [
    ...ALL_ITEMS,
    ...users.map((user) => ({ id: user.id, label: user.name })),
  ]
  const nodeItems = [
    ...ALL_ITEMS,
    ...(topology?.nodes ?? []).map((node) => ({ id: node.id, label: node.name })),
  ]
  const statusItems = [
    ...ALL_ITEMS,
    ...STATUSES.map((value) => ({ id: value, label: value })),
  ]

  return (
    <div className="space-y-6">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">流量</h1>
          <p className="text-sm text-muted">节点间流量拓扑与代理用户访问记录</p>
        </div>
        <div className="flex items-center gap-2">
          {/* React Aria 的受控 API 是 selectedKey/onSelectionChange（已在浏览器
              验证回显与查询会更新），不要按旧版文档改成 value/onChange。 */}
          <Select
            aria-label="统计窗口"
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
                  <ListBox.Item key={item.value} id={String(item.value)} textValue={item.label}>
                    {item.label}
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
              void loadTopology()
              setRefresh((value) => value + 1)
            }}
          >
            <RefreshCw className="h-4 w-4" /> 刷新
          </Button>
        </div>
      </div>

      <div className="grid gap-4 sm:grid-cols-3">
        <Card>
          <Card.Header className="pb-2">
            <Card.Title className="text-sm font-medium text-muted">窗口流量</Card.Title>
          </Card.Header>
          <Card.Content>
            <div className="text-2xl font-semibold">{formatBytes(totalBytes)}</div>
          </Card.Content>
        </Card>
        <Card>
          <Card.Header className="pb-2">
            <Card.Title className="text-sm font-medium text-muted">中转会话</Card.Title>
          </Card.Header>
          <Card.Content>
            <div className="text-2xl font-semibold">{totalSessions}</div>
          </Card.Content>
        </Card>
        <Card>
          <Card.Header className="pb-2">
            <Card.Title className="text-sm font-medium text-muted">拓扑节点</Card.Title>
          </Card.Header>
          <Card.Content>
            <div className="text-2xl font-semibold">{topology?.nodes.length ?? 0}</div>
            <p className="text-xs text-muted">{topology?.links.length ?? 0} 条连线</p>
          </Card.Content>
        </Card>
      </div>

      <Card>
        <Card.Header>
          <Card.Title className="text-base">流量拓扑图</Card.Title>
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
          <Card.Title className="text-base">访问记录</Card.Title>
        </Card.Header>
        <Card.Content className="space-y-4">
          <div className="grid gap-3 md:grid-cols-5">
            <div className="space-y-1">
              <Select
                aria-label="用户"
                selectedKey={userId}
                onSelectionChange={(key) => {
                  setOffset(0)
                  setUserId(key as string)
                }}
              >
                <Label className="text-xs">用户</Label>
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
                aria-label="入口节点"
                selectedKey={nodeId}
                onSelectionChange={(key) => {
                  setOffset(0)
                  setNodeId(key as string)
                }}
              >
                <Label className="text-xs">入口节点</Label>
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
                aria-label="状态"
                selectedKey={status}
                onSelectionChange={(key) => {
                  setOffset(0)
                  setStatus(key as string)
                }}
              >
                <Label className="text-xs">状态</Label>
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
              <Label className="text-xs">目标关键字</Label>
              <div className="flex gap-2">
                <Input variant="secondary"
                  aria-label="目标关键字"
                  className="flex-1"
                  value={targetInput}
                  onChange={(event) => setTargetInput(event.target.value)}
                  placeholder="example.com:443"
                />
                <Button variant="outline" className="shrink-0 gap-1" onPress={applyFilters}>
                  <Filter className="h-4 w-4" /> 筛选
                </Button>
              </div>
            </div>
          </div>

          <Separator />

          <Table>
            <Table.Content
              aria-label="访问记录"
              // Row clicks open the detail modal. React Aria reports the row
              // key, which is the numeric log id cast to a string.
              onRowAction={(key) => {
                const log = logs?.items.find((item) => String(item.id) === String(key))
                if (log) setSelected(log)
              }}
            >
              <Table.Header>
                <Table.Column isRowHeader>时间</Table.Column>
                <Table.Column>用户</Table.Column>
                <Table.Column>入口节点</Table.Column>
                <Table.Column>目标</Table.Column>
                <Table.Column>协议</Table.Column>
                <Table.Column>耗时</Table.Column>
                <Table.Column>上行</Table.Column>
                <Table.Column>下行</Table.Column>
                <Table.Column>状态</Table.Column>
                <Table.Column>路径</Table.Column>
              </Table.Header>
              <Table.Body
                renderEmptyState={() => (
                  <div className="py-8 text-center text-sm text-muted">
                    {loading ? '加载中…' : '没有访问记录'}
                  </div>
                )}
              >
                {(logs?.items ?? []).map((log) => (
                  <Table.Row key={log.id} id={String(log.id)} className="cursor-pointer">
                    <Table.Cell className="whitespace-nowrap text-xs text-muted">
                      {formatRelative(log.startedAt)}
                    </Table.Cell>
                    <Table.Cell className="font-mono text-xs">{log.userId.slice(0, 8)}…</Table.Cell>
                    <Table.Cell className="text-xs">{nodeName(log.entryNodeId)}</Table.Cell>
                    <Table.Cell className="max-w-48 truncate font-mono text-xs">{log.target}</Table.Cell>
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
            <span>共 {logs?.total ?? 0} 条</span>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                isDisabled={offset === 0}
                onPress={() => setOffset(Math.max(0, offset - limit))}
              >
                上一页
              </Button>
              <span>
                {Math.floor(offset / limit) + 1} /{' '}
                {Math.max(1, Math.ceil((logs?.total ?? 0) / limit))}
              </span>
              <Button
                variant="outline"
                size="sm"
                isDisabled={offset + limit >= (logs?.total ?? 0)}
                onPress={() => setOffset(offset + limit)}
              >
                下一页
              </Button>
            </div>
          </div>
        </Card.Content>
      </Card>

      <Modal.Backdrop
        isOpen={Boolean(selected)}
        onOpenChange={(open) => !open && setSelected(null)}
      >
        <Modal.Container>
          <Modal.Dialog className="sm:max-w-xl">
            <Modal.CloseTrigger />
            <Modal.Header>
              <Modal.Heading>访问详情</Modal.Heading>
              <p className="text-sm text-muted">{selected ? formatTime(selected.startedAt) : ''}</p>
            </Modal.Header>
            <Modal.Body>
              {selected && (
                <div className="space-y-3 text-sm">
                  <div className="grid grid-cols-2 gap-2">
                    <div>
                      <span className="text-muted">用户：</span>
                      {selected.userId}
                    </div>
                    <div>
                      <span className="text-muted">入口：</span>
                      {nodeName(selected.entryNodeId)}
                    </div>
                    <div>
                      <span className="text-muted">目标：</span>
                      {selected.target}
                    </div>
                    <div>
                      <span className="text-muted">协议：</span>
                      {selected.command}
                    </div>
                    <div>
                      <span className="text-muted">耗时：</span>
                      {selected.durationMs} ms
                    </div>
                    <div>
                      <span className="text-muted">状态：</span>
                      {selected.status}
                    </div>
                    <div>
                      <span className="text-muted">上行：</span>
                      {formatBytes(selected.upBytes)}
                    </div>
                    <div>
                      <span className="text-muted">下行：</span>
                      {formatBytes(selected.downBytes)}
                    </div>
                    <div>
                      <span className="text-muted">客户端：</span>
                      {selected.clientAddr || '—'}
                    </div>
                    <div>
                      <span className="text-muted">Session：</span>
                      {selected.sessionId}
                    </div>
                  </div>
                  <div>
                    <div className="mb-1 text-muted">路径</div>
                    <div className="flex flex-wrap gap-1">
                      {selected.path.map((id) => (
                        <Chip key={id} size="sm" variant="soft" color="default">
                          {nodeName(id)}
                        </Chip>
                      ))}
                    </div>
                  </div>
                  {selected.error && (
                    <div className="rounded-md border border-danger/40 bg-danger/10 p-2 text-xs text-danger">
                      {selected.error}
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
