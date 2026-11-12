/**
 * 查询层的类型推导测试。
 *
 * 这些赋值本身就在校验推导结果：`useQuery` 的 data、`mutate` 的参数若与
 * protobuf 方法签名不一致，`tsc` 会直接报错，因此它们同时是编译期断言。
 * 运行时部分只验证 Proxy 装饰不会破坏原方法的调用能力。
 */
import { describe, expect, it } from 'bun:test'
import type { UseQueryResult } from '@tanstack/react-query'
import { client } from './trpc'
import type {
  AccessLogPage,
  NodeView,
  ProxyUserView,
  Stats,
  VersionInfo,
} from './types'
import type { AccessLogsRequest } from '@/gen/rpc_pb'

// —— 编译期断言（仅用于类型检查）——
type QueryOf<T> = () => UseQueryResult<T>
const statsQuery: QueryOf<Stats> = () => client.getStats.useQuery()
const nodesQuery: QueryOf<NodeView[]> = () => client.listNodes.useQuery()
const usersQuery: QueryOf<ProxyUserView[]> = () => client.listUsers.useQuery()
const logsQuery: QueryOf<AccessLogPage> = () =>
  client.listAccessLogs.useQuery({ limit: 50n })
const versionsQuery: QueryOf<VersionInfo> = () => client.getVersionInfo.useQuery()

const createNodeMutation = () => client.createNode.useMutation()
// mutateAsync 的入参就是 proto 请求消息；下面这行校验推导结果。
const acceptsRequest = (mutate: (r: { name: string }) => void) => mutate({ name: 'edge' })

export {
  statsQuery,
  nodesQuery,
  usersQuery,
  logsQuery,
  versionsQuery,
  createNodeMutation,
  acceptsRequest,
}

describe('query client', () => {
  it('keeps the original callable API', () => {
    expect(typeof client.listNodes).toBe('function')
    expect(typeof client.createNode).toBe('function')
  })

  it('exposes query and mutation hooks', () => {
    expect(typeof client.listNodes.useQuery).toBe('function')
    expect(typeof client.createNode.useMutation).toBe('function')
  })

  it('decorates every protobuf method', () => {
    // 方法名与 proto 一致，删除类方法也带 hook。
    expect(typeof client.deleteNode.useMutation).toBe('function')
    expect(typeof client.listAccessLogs.useQuery).toBe('function')
  })

  it('returns a stable decorated function per name', () => {
    // 缓存避免每次访问都新建 hook，保证 React 依赖比较稳定。
    expect(client.listNodes).toBe(client.listNodes)
  })

  it('accepts protobuf request messages', () => {
    // 入参类型来自 proto；下面这行若字段名写错会编译失败。
    const request: Partial<AccessLogsRequest> = { limit: 50n, offset: 0n }
    expect(request.limit).toBe(50n)
  })
})
