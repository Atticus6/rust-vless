/**
 * 主节点 `WebAdmin` 服务的浏览器客户端。
 *
 * 这里使用 `@connectrpc/connect-web` 的 gRPC-Web transport（`createGrpcWebTransport`）
 * 发送请求，而不是 Connect 协议；服务端由 tonic-web 解码。本模块是前端唯一
 * 接触 protobuf 消息的地方：它把 protobuf 的 64 位整数（`bigint`）转成普通
 * number、把枚举转成 UI 使用的字符串联合类型，页面因此不需要直接引用生成代码。
 */

import { Code, ConnectError, createClient, type Interceptor } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import {
  LinkTransport,
  WebAdmin,
  WebRole,
  type AccessLogPage as PbAccessLogPage,
  type AccessLogView as PbAccessLogView,
  type AdminView as PbAdminView,
  type LinkView as PbLinkView,
  type NodeView as PbNodeView,
  type ProxyUserView as PbProxyUserView,
  type StatsView as PbStatsView,
  type TopologyResponse as PbTopologyResponse,
  type TrafficTopologyView as PbTrafficTopology,
} from '@/gen/rpc_pb'
import { clearAuth, getToken } from './auth'
import type {
  AccessLogPage,
  AccessLogView,
  AdminView,
  CreateNodeResponse,
  LinkView,
  LoginResponse,
  NodeView,
  ProxyUserView,
  Role,
  Stats,
  TrafficTopology,
  TopologyResponse,
} from './types'

/** 带 gRPC 状态码的 UI 错误；调用方可据此区分「登录过期」与业务失败。 */
export class RpcError extends Error {
  readonly code: Code

  constructor(message: string, code: Code = Code.Unknown) {
    super(message)
    this.name = 'RpcError'
    this.code = code
  }
}

/**
 * 给每个请求附加会话 token。
 *
 * 安全边界：token 会发给本模块配置的 RPC 基地址。默认同源（生产由主节点
 * 托管 SPA 与 gRPC-Web，开发由 Vite 代理），因此设置
 * `VITE_RPC_BASE_URL` 时必须确认该地址确为主节点；任何第三方域名都会收到
 * 用户的 Bearer token。token 在每次调用时读取，登出/重新登录无需重建客户端。
 */
const authInterceptor: Interceptor = (next) => async (request) => {
  const token = getToken()
  if (token) request.header.set('Authorization', `Bearer ${token}`)
  return next(request)
}

/**
 * 计算 RPC 基地址。
 *
 * 未配置时使用同源地址：开发环境由 Vite 代理 `/rustvless.rpc.v1`，生产
 * 环境由主节点同时提供静态页面与 gRPC-Web 路由。配置值必须包含 scheme
 * （如 `https://master.example.com`），不能带尾部路径。
 */
function defaultBaseUrl(): string {
  const configured = import.meta.env.VITE_RPC_BASE_URL
  if (configured) return configured
  return typeof window === 'undefined' ? '' : window.location.origin
}

/** gRPC-Web transport 单例；baseUrl 在应用启动时确定。 */
export const transport = createGrpcWebTransport({
  baseUrl: defaultBaseUrl(),
  interceptors: [authInterceptor],
})

/** `WebAdmin` 服务的类型化客户端，供本模块内部及测试使用。 */
export const client = createClient(WebAdmin, transport)

/**
 * 把 transport 失败统一成 `RpcError`。
 *
 * 已鉴权调用返回 `Unauthenticated` 表示会话过期：清空本地登录态并跳转
 * `/login`。`redirectOnAuthFailure=false` 用于登录请求本身，密码错误只弹
 * toast，不触发跳转。非 Connect 错误保留原始 message，状态码记为 Unknown。
 *
 * `navigate` 默认为 `window.location.assign`，测试可注入替代实现，避免
 * 修改 jsdom 中不可配置的 `window.location`。
 */
export function normalizeRpcError(
  error: unknown,
  redirectOnAuthFailure = true,
  navigate: (path: string) => void = (path) => window.location.assign(path),
): RpcError {
  if (error instanceof ConnectError) {
    if (error.code === Code.Unauthenticated && redirectOnAuthFailure) {
      clearAuth()
      if (typeof window !== 'undefined' && window.location.pathname !== '/login') {
        navigate('/login')
      }
    }
    return new RpcError(error.rawMessage || error.message, error.code)
  }
  return new RpcError(error instanceof Error ? error.message : String(error))
}

/** 执行一次 RPC 并把异常转换为 `RpcError`；`redirectOnAuthFailure` 含义同上。 */
async function guard<T>(operation: () => Promise<T>, redirectOnAuthFailure = true): Promise<T> {
  try {
    return await operation()
  } catch (error) {
    throw normalizeRpcError(error, redirectOnAuthFailure)
  }
}

/** protobuf 的 int64/uint64 解码为 `bigint`，UI 统一使用 number（超出 2^53 会丢精度，仅影响展示）。 */
function num(value: bigint | number): number {
  return typeof value === 'bigint' ? Number(value) : value
}

/** 把 protobuf `WebRole` 映射为 `'admin' | 'user'`；未知值按普通用户处理。 */
export function mapRole(role: WebRole): Role {
  return role === WebRole.ADMIN ? 'admin' : 'user'
}

/** 把 protobuf `LinkTransport` 映射为 `'rpc' | 'http'`；未知值回退到 rpc。 */
export function mapTransport(transport: LinkTransport): 'rpc' | 'http' {
  return transport === LinkTransport.HTTP ? 'http' : 'rpc'
}

/** 管理账号消息 → UI 类型；`createdAt` 为 Unix 毫秒。 */
export function mapAdmin(admin: PbAdminView): AdminView {
  return {
    id: admin.id,
    email: admin.email,
    role: mapRole(admin.role),
    createdAt: num(admin.createdAt),
  }
}

/** 节点消息 → UI 类型；时间字段为 Unix 毫秒，计数为 number。 */
export function mapNode(node: PbNodeView): NodeView {
  return {
    id: node.id,
    name: node.name,
    status: node.status,
    version: node.version,
    httpUrl: node.httpUrl,
    rpcUrl: node.rpcUrl,
    isMaster: node.isMaster,
    lastSeen: num(node.lastSeen),
    createdAt: num(node.createdAt),
    activeConnections: num(node.activeConnections),
    uptimeSec: num(node.uptimeSec),
  }
}

/** 中转连线消息 → UI 类型；`transport` 转字符串联合，`priority` 转 number。 */
export function mapLink(link: PbLinkView): LinkView {
  return {
    id: link.id,
    fromNodeId: link.fromNodeId,
    toNodeId: link.toNodeId,
    fromName: link.fromName,
    toName: link.toName,
    transport: mapTransport(link.transport),
    priority: num(link.priority),
    enabled: link.enabled,
    createdAt: num(link.createdAt),
  }
}

/** 代理用户消息 → UI 类型；流量计数为 number，时间为 Unix 毫秒。 */
export function mapUser(user: PbProxyUserView): ProxyUserView {
  return {
    id: user.id,
    name: user.name,
    enabled: user.enabled,
    ownerId: user.ownerId,
    ownerEmail: user.ownerEmail,
    createdAt: num(user.createdAt),
    upBytes: num(user.upBytes),
    downBytes: num(user.downBytes),
  }
}

/** 总览统计消息 → UI 类型；所有计数与字节数均为 number。 */
export function mapStats(stats: PbStatsView): Stats {
  return {
    nodesTotal: num(stats.nodesTotal),
    nodesOnline: num(stats.nodesOnline),
    usersTotal: num(stats.usersTotal),
    usersEnabled: num(stats.usersEnabled),
    adminsTotal: num(stats.adminsTotal),
    upBytes: num(stats.upBytes),
    downBytes: num(stats.downBytes),
    activeConnections: num(stats.activeConnections),
    linksTotal: num(stats.linksTotal),
  }
}

/** 访问记录消息 → UI 类型；未知协议按 tcp 处理，状态保留服务端字符串。 */
export function mapAccessLog(log: PbAccessLogView): AccessLogView {
  return {
    id: num(log.id),
    sessionId: log.sessionId,
    userId: log.userId,
    entryNodeId: log.entryNodeId,
    target: log.target,
    command: log.command === 'udp' ? 'udp' : 'tcp',
    startedAt: num(log.startedAt),
    endedAt: num(log.endedAt),
    durationMs: num(log.durationMs),
    upBytes: num(log.upBytes),
    downBytes: num(log.downBytes),
    status: log.status as AccessLogView['status'],
    error: log.error,
    path: [...log.path],
    clientAddr: log.clientAddr,
  }
}

/** 访问记录分页消息 → UI 类型；`total/limit/offset` 转 number。 */
export function mapAccessLogPage(page: PbAccessLogPage): AccessLogPage {
  return {
    items: page.items.map(mapAccessLog),
    total: num(page.total),
    limit: num(page.limit),
    offset: num(page.offset),
  }
}

/** 拓扑响应 → `{ nodes, links }`。 */
export function mapTopology(topology: PbTopologyResponse): TopologyResponse {
  return {
    nodes: topology.nodes.map(mapNode),
    links: topology.links.map(mapLink),
  }
}

/** 流量拓扑 → UI 类型；`windowSec` 单位为秒，字节数为 number。 */
export function mapTrafficTopology(topology: PbTrafficTopology): TrafficTopology {
  return {
    windowSec: num(topology.windowSec),
    nodes: topology.nodes.map((node) => ({
      id: node.id,
      name: node.name,
      isMaster: node.isMaster,
      online: node.online,
      activeConnections: num(node.activeConnections),
    })),
    links: topology.links.map((link) => ({
      id: link.id,
      fromNodeId: link.fromNodeId,
      toNodeId: link.toNodeId,
      transport: mapTransport(link.transport),
      upBytes: num(link.upBytes),
      downBytes: num(link.downBytes),
      sessions: num(link.sessions),
    })),
  }
}

/**
 * 使用邮箱/密码登录。
 *
 * 失败时抛出 `RpcError`（凭证错误为 `Unauthenticated`），且不会触发跳转，
 * 便于登录页展示错误。成功返回会话 token 与账号信息。
 */
export async function login(email: string, password: string): Promise<LoginResponse> {
  const response = await guard(() => client.login({ email, password }), false)
  if (!response.admin) throw new RpcError('登录响应缺少账号信息', Code.Internal)
  return { token: response.token, admin: mapAdmin(response.admin) }
}

/** 注销当前会话；服务端不可达时调用方仍应清空本地状态。 */
export async function logout(): Promise<void> {
  await guard(() => client.logout({}), false)
}

/** 读取当前账号；会话过期会触发跳转登录页。 */
export async function me(): Promise<AdminView> {
  return mapAdmin(await guard(() => client.me({})))
}

/** 读取总览统计（管理员看全量，普通账号只看自己名下的用户与全局计数）。 */
export async function stats(): Promise<Stats> {
  return mapStats(await guard(() => client.getStats({})))
}

/** 列出节点；需要 admin 角色，普通账号会得到 `PermissionDenied`。 */
export async function listNodes(): Promise<NodeView[]> {
  const response = await guard(() => client.listNodes({}))
  return response.nodes.map(mapNode)
}

/** 创建子节点并返回一次性 token；token 只出现这一次，调用方必须立即展示/保存。 */
export async function createNode(name: string): Promise<CreateNodeResponse> {
  const response = await guard(() => client.createNode({ name }))
  if (!response.node) throw new RpcError('创建节点响应缺少节点信息', Code.Internal)
  return { node: mapNode(response.node), token: response.token }
}

/** 局部更新节点；未提供的字段保持原值，URL 变更会立即下发新拓扑。 */
export async function updateNode(
  id: string,
  patch: { name?: string; httpUrl?: string; rpcUrl?: string },
): Promise<NodeView> {
  return mapNode(await guard(() => client.updateNode({ id, ...patch })))
}

/** 删除子节点；主节点自身会返回 `InvalidArgument`。 */
export async function deleteNode(id: string): Promise<void> {
  await guard(() => client.deleteNode({ id }))
}

/** 列出当前账号可见的代理用户。 */
export async function listUsers(): Promise<ProxyUserView[]> {
  const response = await guard(() => client.listUsers({}))
  return response.users.map(mapUser)
}

/** 创建代理用户；`ownerId` 仅对 admin 生效，普通账号只能创建自己的用户。 */
export async function createUser(name: string, ownerId?: string): Promise<ProxyUserView> {
  return mapUser(await guard(() => client.createUser({ name, ownerId })))
}

/** 修改代理用户名称或启用状态；越权操作返回 `PermissionDenied`。 */
export async function updateUser(
  id: string,
  patch: { name?: string; enabled?: boolean },
): Promise<ProxyUserView> {
  return mapUser(await guard(() => client.updateUser({ id, ...patch })))
}

/** 删除代理用户及其流量记录；越权操作返回 `PermissionDenied`。 */
export async function deleteUser(id: string): Promise<void> {
  await guard(() => client.deleteUser({ id }))
}

/** 列出管理账号；需要 admin 角色。 */
export async function listAdmins(): Promise<AdminView[]> {
  const response = await guard(() => client.listAdmins({}))
  return response.admins.map(mapAdmin)
}

/** 创建管理账号；密码需 8–128 字节，重复邮箱返回 `AlreadyExists`。 */
export async function createAdmin(input: {
  email: string
  password: string
  role: Role
}): Promise<AdminView> {
  const role = input.role === 'admin' ? WebRole.ADMIN : WebRole.USER
  return mapAdmin(await guard(() => client.createAdmin({ ...input, role })))
}

/** 删除管理账号；不能删除自己或最后一个 admin。 */
export async function deleteAdmin(id: string): Promise<void> {
  await guard(() => client.deleteAdmin({ id }))
}

/** 列出中转连线；需要 admin 角色。 */
export async function listLinks(): Promise<LinkView[]> {
  const response = await guard(() => client.listLinks({}))
  return response.links.map(mapLink)
}

/** 创建有向连线；成环、重复或自连返回 `AlreadyExists`/`InvalidArgument`。 */
export async function createLink(input: {
  fromNodeId: string
  toNodeId: string
  transport: 'rpc' | 'http'
  priority: number
}): Promise<LinkView> {
  const transport = input.transport === 'http' ? LinkTransport.HTTP : LinkTransport.RPC
  return mapLink(
    await guard(() =>
      client.createLink({ ...input, transport, priority: BigInt(input.priority) }),
    ),
  )
}

/** 删除连线并立即下发新拓扑。 */
export async function deleteLink(id: string): Promise<void> {
  await guard(() => client.deleteLink({ id }))
}

/** 读取节点 + 连线拓扑；需要 admin 角色。 */
export async function topology(): Promise<TopologyResponse> {
  return mapTopology(await guard(() => client.getTopology({})))
}

/**
 * 分页查询访问记录。
 *
 * `from`/`to` 为 Unix 毫秒；省略 `limit` 时服务端取 50，上限 200；
 * `offset` 负数按 0 处理。普通账号只能看到自己用户的记录。
 */
export async function accessLogs(query: {
  userId?: string
  nodeId?: string
  status?: string
  target?: string
  from?: number
  to?: number
  limit?: number
  offset?: number
}): Promise<AccessLogPage> {
  const response = await guard(() =>
    client.listAccessLogs({
      userId: query.userId,
      nodeId: query.nodeId,
      status: query.status,
      target: query.target,
      from: query.from === undefined ? undefined : BigInt(query.from),
      to: query.to === undefined ? undefined : BigInt(query.to),
      limit: query.limit === undefined ? undefined : BigInt(query.limit),
      offset: query.offset === undefined ? undefined : BigInt(query.offset),
    }),
  )
  return mapAccessLogPage(response)
}

/**
 * 读取流量拓扑。
 *
 * `windowSec` 为聚合窗口（秒），省略时服务端默认 3600，取值会被限制在
 * 60–604800 之间。普通账号的连线计数为 0。
 */
export async function trafficTopology(windowSec?: number): Promise<TrafficTopology> {
  return mapTrafficTopology(
    await guard(() =>
      client.getTrafficTopology({
        window: windowSec === undefined ? undefined : BigInt(windowSec),
      }),
    ),
  )
}

/** 按用户汇总流量；当前页面未使用，保留给导出/脚本。 */
export async function trafficOverview() {
  const response = await guard(() => client.getTrafficOverview({}))
  return response.rows.map((row) => ({
    userId: row.userId,
    name: row.name,
    ownerEmail: row.ownerEmail,
    upBytes: num(row.upBytes),
    downBytes: num(row.downBytes),
  }))
}
