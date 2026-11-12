/**
 * 浏览器 RPC 路径的端到端冒烟测试。
 *
 * 使用与 React 应用相同的 Connect gRPC-Web transport 请求真实主节点，验证
 * proto 接线、gRPC-Web 帧、Authorization 元数据与错误码，而不是只测服务层。
 *
 * 用法：bun scripts/smoke.ts <base-url> <admin-password>
 */
import { ConnectError, createClient, type Interceptor } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import { LinkTransport, WebAdmin } from '../src/gen/rpc_pb'

const baseUrl = process.argv[2] ?? 'http://127.0.0.1:18080'
const password = process.argv[3]
if (!password) {
  console.error('usage: bun scripts/smoke.ts <base-url> <admin-password>')
  process.exit(2)
}

let token = ''
const auth: Interceptor = (next) => async (request) => {
  if (token) request.header.set('authorization', `Bearer ${token}`)
  return next(request)
}

const client = createClient(WebAdmin, createGrpcWebTransport({ baseUrl, interceptors: [auth] }))

function check(condition: unknown, message: string): void {
  if (!condition) throw new Error(`FAIL: ${message}`)
  console.log(`ok: ${message}`)
}

const login = await client.login({ email: 'admin@example.com', password })
check(login.token.length > 0, 'login returns a session token')
token = login.token
check(login.admin?.email === 'admin@example.com', 'login returns the admin view')

const me = await client.me({})
check(me.role === 1, 'me returns the admin role')

const before = await client.listNodes({})
check(before.nodes.length === 1, 'master node is listed')

const created = await client.createNode({ name: 'smoke-edge' })
check(Boolean(created.node?.id) && created.token.length > 0, 'createNode returns node and token')

const topology = await client.getTopology({})
const master = topology.nodes.find((node) => node.isMaster)
check(Boolean(master), 'topology includes the master')
const edge = topology.nodes.find((node) => node.name === 'smoke-edge')
check(Boolean(edge), 'topology includes the new node')

const link = await client.createLink({
  fromNodeId: master!.id,
  toNodeId: edge!.id,
  transport: LinkTransport.RPC,
  priority: 2n,
})
check(link.transport === LinkTransport.RPC && link.priority === 2n, 'createLink round-trips')

const logs = await client.listAccessLogs({ limit: 100_000n })
check(logs.limit <= 200n, 'access log limit is clamped')

// Requests without the interceptor must be rejected with UNAUTHENTICATED.
const anonymous = createClient(WebAdmin, createGrpcWebTransport({ baseUrl }))
try {
  await anonymous.listNodes({})
  throw new Error('FAIL: unauthenticated call unexpectedly succeeded')
} catch (error) {
  const code = error instanceof ConnectError ? error.code : -1
  check(code === 16, 'unauthenticated call is rejected with code 16')
}

console.log('PASS: web RPC smoke test')
