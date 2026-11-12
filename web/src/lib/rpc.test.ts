/**
 * RPC 客户端单测。
 *
 * 覆盖 protobuf 消息到 UI 类型的映射、gRPC 错误到 `RpcError` 的归一化，以及
 * 登录/分页查询的请求参数转换。测试不启动服务端：客户端方法用 spy 替换，
 * 只验证本模块的边界行为（界面与主节点之间的传输由 scripts/smoke.ts 验证）。
 */
import { create } from '@bufbuild/protobuf'
import { Code, ConnectError } from '@connectrpc/connect'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  AccessLogPageSchema,
  AccessLogViewSchema,
  AdminViewSchema,
  LinkTransport,
  LinkViewSchema,
  LoginResponseSchema,
  NodeViewSchema,
  ProxyUserViewSchema,
  WebRole,
} from '@/gen/rpc_pb'
import { setAuth } from './auth'
import {
  accessLogs,
  client,
  login,
  mapLink,
  mapNode,
  mapRole,
  mapTransport,
  mapUser,
  normalizeRpcError,
  RpcError,
} from './rpc'

describe('enum mapping', () => {
  it('maps roles to the UI union', () => {
    expect(mapRole(WebRole.ADMIN)).toBe('admin')
    expect(mapRole(WebRole.USER)).toBe('user')
    expect(mapRole(WebRole.UNSPECIFIED)).toBe('user')
  })

  it('maps link transports to the UI union', () => {
    expect(mapTransport(LinkTransport.RPC)).toBe('rpc')
    expect(mapTransport(LinkTransport.HTTP)).toBe('http')
    expect(mapTransport(LinkTransport.UNSPECIFIED)).toBe('rpc')
  })
})

describe('message mapping', () => {
  it('converts 64-bit node fields to numbers', () => {
    const node = mapNode(
      create(NodeViewSchema, {
        id: 'node-1',
        name: 'tokyo',
        status: 'online',
        version: '0.1.0',
        httpUrl: 'http://tokyo:8080',
        rpcUrl: 'http://tokyo:50051',
        isMaster: false,
        lastSeen: 1_700_000_000_000n,
        createdAt: 1_600_000_000_000n,
        activeConnections: 42n,
        uptimeSec: 3_600n,
      }),
    )

    expect(node).toEqual({
      id: 'node-1',
      name: 'tokyo',
      status: 'online',
      version: '0.1.0',
      httpUrl: 'http://tokyo:8080',
      rpcUrl: 'http://tokyo:50051',
      isMaster: false,
      lastSeen: 1_700_000_000_000,
      createdAt: 1_600_000_000_000,
      activeConnections: 42,
      uptimeSec: 3_600,
    })
    expect(typeof node.activeConnections).toBe('number')
  })

  it('maps links including the transport enum and priority', () => {
    const link = mapLink(
      create(LinkViewSchema, {
        id: 'link-1',
        fromNodeId: 'a',
        toNodeId: 'b',
        fromName: 'A',
        toName: 'B',
        transport: LinkTransport.HTTP,
        priority: 7n,
        enabled: true,
        createdAt: 1n,
      }),
    )

    expect(link.transport).toBe('http')
    expect(link.priority).toBe(7)
    expect(link.createdAt).toBe(1)
  })

  it('converts user traffic counters', () => {
    const user = mapUser(
      create(ProxyUserViewSchema, {
        id: 'user-1',
        name: 'phone',
        enabled: true,
        ownerId: 'owner-1',
        ownerEmail: 'owner@example.com',
        createdAt: 10n,
        upBytes: 2n ** 40n,
        downBytes: 5n,
      }),
    )

    expect(user.upBytes).toBe(Number(2n ** 40n))
    expect(user.downBytes).toBe(5)
  })
})

describe('error normalization', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('keeps the gRPC code and the server message', () => {
    const error = normalizeRpcError(new ConnectError('invalid email', Code.InvalidArgument))
    expect(error).toBeInstanceOf(RpcError)
    expect(error.code).toBe(Code.InvalidArgument)
    expect(error.message).toBe('invalid email')
  })

  it('clears the session and redirects when an authenticated call expires', () => {
    const navigate = vi.fn()
    setAuth('expired-token', {
      id: 'admin-1',
      email: 'admin@example.com',
      role: 'admin',
      createdAt: 1,
    })

    const error = normalizeRpcError(
      new ConnectError('unauthorized', Code.Unauthenticated),
      true,
      navigate,
    )

    expect(error.code).toBe(Code.Unauthenticated)
    expect(window.localStorage.getItem('rv_token')).toBeNull()
    expect(navigate).toHaveBeenCalledWith('/login')
  })

  it('does not redirect for login failures', () => {
    const navigate = vi.fn()
    setAuth('still-valid', {
      id: 'admin-1',
      email: 'admin@example.com',
      role: 'admin',
      createdAt: 1,
    })

    normalizeRpcError(new ConnectError('unauthorized', Code.Unauthenticated), false, navigate)

    expect(window.localStorage.getItem('rv_token')).toBe('still-valid')
    expect(navigate).not.toHaveBeenCalled()
  })

  it('wraps non-connect errors', () => {
    const error = normalizeRpcError(new Error('boom'))
    expect(error.message).toBe('boom')
    expect(error.code).toBe(Code.Unknown)
  })
})

describe('login', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('returns the token and mapped account', async () => {
    vi.spyOn(client, 'login').mockResolvedValue(
      create(LoginResponseSchema, {
        token: 'session-token',
        admin: create(AdminViewSchema, {
          id: 'admin-1',
          email: 'admin@example.com',
          role: WebRole.ADMIN,
          createdAt: 123n,
        }),
      }),
    )

    await expect(login('admin@example.com', 'password123')).resolves.toEqual({
      token: 'session-token',
      admin: {
        id: 'admin-1',
        email: 'admin@example.com',
        role: 'admin',
        createdAt: 123,
      },
    })
  })

  it('surfaces a bad password as an unauthenticated RpcError', async () => {
    vi.spyOn(client, 'login').mockRejectedValue(
      new ConnectError('unauthorized', Code.Unauthenticated),
    )

    await expect(login('admin@example.com', 'wrong')).rejects.toMatchObject({
      name: 'RpcError',
      code: Code.Unauthenticated,
      message: 'unauthorized',
    })
  })
})

describe('accessLogs', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('converts every provided filter to protobuf bigint and maps the page', async () => {
    const spy = vi.spyOn(client, 'listAccessLogs').mockResolvedValue(
      create(AccessLogPageSchema, {
        items: [
          create(AccessLogViewSchema, {
            id: 7n,
            sessionId: 'session-1',
            userId: 'user-1',
            entryNodeId: 'node-1',
            target: 'example.com:443',
            command: 'tcp',
            startedAt: 1_000n,
            endedAt: 1_500n,
            durationMs: 500n,
            upBytes: 10n,
            downBytes: 20n,
            status: 'ok',
            path: ['node-1'],
            clientAddr: '203.0.113.9:1234',
          }),
        ],
        total: 1n,
        limit: 25n,
        offset: 50n,
      }),
    )

    const page = await accessLogs({
      userId: 'user-1',
      nodeId: 'node-1',
      status: 'ok',
      target: 'example.com',
      from: 1_000,
      to: 2_000,
      limit: 25,
      offset: 50,
    })

    expect(spy).toHaveBeenCalledWith({
      userId: 'user-1',
      nodeId: 'node-1',
      status: 'ok',
      target: 'example.com',
      from: 1_000n,
      to: 2_000n,
      limit: 25n,
      offset: 50n,
    })
    expect(page.total).toBe(1)
    expect(page.limit).toBe(25)
    expect(page.offset).toBe(50)
    expect(page.items[0]).toMatchObject({
      id: 7,
      sessionId: 'session-1',
      durationMs: 500,
      upBytes: 10,
      downBytes: 20,
      status: 'ok',
      command: 'tcp',
      path: ['node-1'],
    })
  })

  it('keeps unset filters absent so the server applies its defaults', async () => {
    const spy = vi
      .spyOn(client, 'listAccessLogs')
      .mockResolvedValue(create(AccessLogPageSchema, { limit: 50n, total: 0n }))

    const page = await accessLogs({})

    expect(spy).toHaveBeenCalledWith({
      userId: undefined,
      nodeId: undefined,
      status: undefined,
      target: undefined,
      from: undefined,
      to: undefined,
      limit: undefined,
      offset: undefined,
    })
    expect(page.items).toEqual([])
    expect(page.limit).toBe(50)
  })
})
