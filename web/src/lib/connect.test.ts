/**
 * 通用映射与错误拦截器测试。
 *
 * `toUi` 是唯一的转换入口：递归把 bigint 转 number、枚举转字符串联合；
 * `errorInterceptor` 把 transport 错误统一成 `RpcError`。
 * 两者都是纯函数/纯包装，不需要服务端。
 */
import { describe, expect, it } from 'bun:test'
import { create } from '@bufbuild/protobuf'
import { Code, ConnectError } from '@connectrpc/connect'
import {
  AdminViewSchema,
  LinkTransport,
  LinkViewSchema,
  NodeViewSchema,
  ProxyUserViewSchema,
  WebRole,
} from '@/gen/rpc_pb'
import { RpcError, errorInterceptor } from './connect'
import { toUi, toUiMessage } from './mappings'
import type { LinkView, NodeView, ProxyUserView } from './types'

describe('toUi', () => {
  it('converts bigint to number recursively', () => {
    const node = toUi<NodeView>(
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

    expect(node.lastSeen).toBe(1_700_000_000_000)
    expect(node.activeConnections).toBe(42)
    expect(node.uptimeSec).toBe(3_600)
    expect(typeof node.activeConnections).toBe('number')
    expect(node.name).toBe('tokyo')
  })

  it('maps enum fields to UI unions', () => {
    const link = toUi<LinkView>(
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

  it('maps role and keeps unknown enum values safe', () => {
    const admin = toUi<{ role: string }>(
      create(AdminViewSchema, {
        id: 'admin-1',
        email: 'admin@example.com',
        role: WebRole.ADMIN,
        createdAt: 1n,
      }),
    )
    expect(admin.role).toBe('admin')

    // 未知枚举值按安全默认处理，不抛错。
    expect(toUi<{ role: string }>({ role: 99 }).role).toBe('user')
    expect(toUi<{ transport: string }>({ transport: 99 }).transport).toBe('rpc')
  })

  it('converts nested arrays', () => {
    const users = toUi<ProxyUserView[]>([
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
    ])

    expect(users[0].upBytes).toBe(Number(2n ** 40n))
    expect(users[0].downBytes).toBe(5)
    expect(users[0].enabled).toBe(true)
  })

  it('leaves primitives, null and undefined untouched', () => {
    expect(toUi<number>(1)).toBe(1)
    expect(toUi<string>('x')).toBe('x')
    expect(toUi<boolean>(false)).toBe(false)
    expect(toUi<null>(null)).toBeNull()
    expect(toUi<undefined>(undefined)).toBeUndefined()
  })

  it('toUiMessage replaces a missing optional message with an empty object', () => {
    expect(toUiMessage<{ id?: string }>(undefined)).toEqual({})
    expect(toUiMessage<{ id: string }>({ id: 'x' })).toEqual({ id: 'x' })
  })
})

describe('errorInterceptor', () => {
  /** 拦截器只关心成功/失败，用最小桩对象满足类型。 */
  const stubRequest = { header: new Headers() } as never
  const stubResponse = { stream: false } as never

  /** 让拦截器处理一个必定失败的下游调用。 */
  async function runWith(error: unknown): Promise<unknown> {
    const handler = errorInterceptor(() => Promise.reject(error))
    return handler(stubRequest).catch((thrown: unknown) => thrown)
  }

  it('converts ConnectError into RpcError keeping the code', async () => {
    const thrown = await runWith(new ConnectError('unauthorized', Code.Unauthenticated))
    expect(thrown).toBeInstanceOf(RpcError)
    expect((thrown as RpcError).code).toBe(Code.Unauthenticated)
    expect((thrown as RpcError).message).toBe('unauthorized')
  })

  it('wraps non-connect errors with Code.Unknown', async () => {
    const thrown = await runWith(new Error('boom'))
    expect(thrown).toBeInstanceOf(RpcError)
    expect((thrown as RpcError).code).toBe(Code.Unknown)
    expect((thrown as RpcError).message).toBe('boom')
  })

  it('passes successful responses through', async () => {
    const handler = errorInterceptor(() => Promise.resolve(stubResponse))
    await expect(handler(stubRequest)).resolves.toBe(stubResponse)
  })
})
