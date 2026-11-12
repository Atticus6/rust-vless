/**
 * 主节点 gRPC-Web 传输与消息映射。
 *
 * 本模块只做三件事：
 * 1. 建立 transport 与 protobuf 客户端（`client`）；
 * 2. 附加会话 token、把 transport 错误统一成 `RpcError`；
 * 3. 提供 protobuf 消息 → UI 类型的纯映射函数。
 *
 * 这里**没有**按业务方法封装的 async 函数，也**不做**登录跳转或会话清理：
 * - 调用方（页面）直接用 `client.<proto方法>.useQuery()` / `.useMutation()`
 *   （见 `./trpc`），入参就是 protobuf 请求消息；
 * - 会话过期由路由守卫（`src/routes/_app.tsx`）统一处理。
 */

import { Code, ConnectError, createClient, type Interceptor } from '@connectrpc/connect'
import { createGrpcWebTransport } from '@connectrpc/connect-web'
import { WebAdmin } from '@/gen/rpc_pb'
import { getToken } from './auth'

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
export function rpcBaseUrl(): string {
  const configured = import.meta.env.VITE_RPC_BASE_URL
  if (configured) return configured
  return typeof window === 'undefined' ? '' : window.location.origin
}

/**
 * 拼接订阅地址。
 *
 * 订阅由主节点提供（`/sub/...`），而控制台可能部署在别的 origin（或由 Vite
 * 代理）。因此用 RPC 基地址而不是 `window.location.origin`，否则用户复制到的
 * 可能是前端页面地址。基地址为空（同源）时保持相对路径。
 */
export function subscriptionUrl(path: string): string {
  const base = rpcBaseUrl()
  return base ? `${base.replace(/\/$/, '')}${path}` : path
}

/**
 * 把 transport 失败统一成 `RpcError`。
 *
 * 作为拦截器挂在 transport 上，因此每个调用点都不需要 `try/catch`：
 * 所有方法抛出的错误已经是 `RpcError`。`Unauthenticated` 的
 * 跳转由路由守卫负责，这里只做类型归一。
 */
export const errorInterceptor: Interceptor = (next) => async (request) => {
  try {
    return await next(request)
  } catch (error) {
    if (error instanceof ConnectError) {
      throw new RpcError(error.rawMessage || error.message, error.code)
    }
    throw new RpcError(error instanceof Error ? error.message : String(error))
  }
}

/** gRPC-Web transport 单例；baseUrl 在应用启动时确定。 */
export const transport = createGrpcWebTransport({
  baseUrl: rpcBaseUrl(),
  interceptors: [authInterceptor, errorInterceptor],
})

/**
 * `WebAdmin` 服务的 protobuf 客户端。
 *
 * 直接按 proto 方法名调用（如 `client.listAccessLogs({ limit: 50n })`），
 * 返回 protobuf 消息。页面通过 `./trpc` 的同名 hook 使用它，输出会自动映射成
 * UI 类型。
 */
export const client = createClient(WebAdmin, transport)
