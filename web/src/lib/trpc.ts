/**
 * tRPC 风格的查询层：**完全由类型推导**，不维护路由表。
 *
 * 把 protobuf 客户端（`./connect` 的 `client`）的每个方法包装成
 * 「既可调用、又带 hook」的对象：
 *
 * ```ts
 * client.listAccessLogs.useQuery({ limit: 50n })   // data: AccessLogPage
 * client.listNodes.useQuery()                      // 无参查询，input 可省略
 * client.createUser.useMutation()                  // mutateAsync({ name: 'phone' })
 * ```
 *
 * 推导来源是 protobuf 方法签名，因此 proto 改动后 hook 的入参与返回类型自动
 * 跟随；漏传必填字段、传错类型都会在编译期报错。
 *
 * 返回值由 `./mappings` 的通用 `toUi` 递归转换（bigint→number、枚举→字符串），
 * 因此 `data` 的类型来自 `UI_RESULT` 这张纯类型表，而不是逐方法手写转换函数。
 * mutation 成功后默认失效整个 `rpc` 根缓存（可用 `options.onSuccess` 覆盖）。
 */
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
  type UseMutationResult,
  type UseQueryOptions,
  type UseQueryResult,
} from '@tanstack/react-query'
import { client as raw } from './connect'
import { guardAuthError } from './guards'
import { toUi } from './mappings'
import type {
  AccessLogPage,
  AdminView,
  CreateNodeResponse,
  LinkView,
  NodeView,
  ProxyUserView,
  SetupStatus,
  Stats,
  SubscriptionInfo,
  TopologyResponse,
  TrafficTopology,
  VersionInfo,
} from './types'

/** 任意异步函数。 */
type ApiFn = (...args: never[]) => Promise<unknown>

/** 函数参数列表。 */
type ArgsOf<F> = F extends (...args: infer A) => unknown ? A : never
/** 函数 `Promise<T>` 中的 `T`。 */
type Resolved<F> = F extends (...args: never[]) => Promise<infer T> ? T : never
/**
 * 查询入参。
 *
 * 用 `Parameters<F>[0]`：无参函数得到 `undefined`（可省略），带可选参数的
 * 函数得到 `T | undefined`，不会把 `[x?: T]` 误判为无参。
 */
type QueryInput<F> = F extends (...args: never[]) => unknown ? Parameters<F>[0] : undefined
/** 映射函数：protobuf 消息 → UI 类型。 */
type Mapper<D, O> = (value: D) => O

/** `useQuery` 选项：`queryKey` / `queryFn` 由包装层提供。 */
type QueryOptions<T> = Omit<UseQueryOptions<T, Error, T, readonly unknown[]>, 'queryKey' | 'queryFn'>
/** mutation 选项：`mutationFn` 由包装层提供。 */
type MutateOptions<D, V> = Omit<UseMutationOptions<D, Error, V>, 'mutationFn'>

/** mutation 结果：`mutate` / `mutateAsync` 直接接收原方法的参数列表。 */
export type MutationResult<D, A extends readonly unknown[]> = Omit<
  UseMutationResult<D, Error, A>,
  'mutate' | 'mutateAsync'
> & {
  mutate: (...args: A) => void
  mutateAsync: (...args: A) => Promise<D>
}

/**
 * 包装后的方法。
 *
 * 直接调用与 hook 返回同一套 UI 类型（都经过 `toUi`），因此页面里
 * `await client.login(...)` 与 `client.login.useMutation()` 的 `data` 完全一致。
 */
export type WithHooks<F extends ApiFn, O> = ((...args: ArgsOf<F>) => Promise<O>) & {
  /** 以 TanStack Query 订阅该调用；`input` 来自 protobuf 方法的第一个参数。 */
  useQuery: (input?: QueryInput<F>, options?: QueryOptions<O>) => UseQueryResult<O>
  /** 把该调用作为 mutation；`mutate(...args)` 与直接调用该方法一致。 */
  useMutation: (options?: MutateOptions<O, ArgsOf<F>>) => MutationResult<O, ArgsOf<F>>
}

/**
 * 返回列表的方法：proto 用 `{ items: [...] }` 包装，UI 直接要数组。
 *
 * 只有这些方法需要额外解包，其余响应交给通用 `toUi` 递归转换。
 */
const LIST_FIELDS = {
  listNodes: 'nodes',
  listUsers: 'users',
  listAdmins: 'admins',
  listLinks: 'links',
  listAccessLogs: 'items',
} as const

/** 取某个方法对应的转换函数。 */
function mapperFor(name: string): Mapper<unknown, unknown> {
  const listField = (LIST_FIELDS as Record<string, string | undefined>)[name]
  if (listField) {
    return (value) => toUi((value as Record<string, unknown>)[listField])
  }
  return (value) => toUi(value)
}

/** 缓存 key 根。 */
const ROOT = 'rpc'

/**
 * 给单个方法挂上 hook。
 *
 * hook 内部才调用 `useQueryClient`，因此可以在模块加载阶段预先构建；
 * 真正的 React hook 调用发生在组件渲染期间（与 tRPC 一致）。
 */
function decorate<F extends ApiFn, O>(
  name: string,
  fn: F,
  map: Mapper<Resolved<F>, O>,
): WithHooks<F, O> {
  const call = fn as unknown as (...args: unknown[]) => Promise<Resolved<F>>

  const useQueryHook = (input?: QueryInput<F>, options?: QueryOptions<O>) =>
    useQuery({
      queryKey: [ROOT, name, input ?? null],
      queryFn: async () => {
        try {
          return map(await call(input))
        } catch (error) {
          // 会话过期时统一跳到登录页；其余错误交给 TanStack Query 的 error 状态。
          return guardAuthError(error)
        }
      },
      ...options,
    })

  const useMutationHook = (options?: MutateOptions<O, ArgsOf<F>>) => {
    const queryClient = useQueryClient()
    const result = useMutation({
      mutationFn: async (args: ArgsOf<F>) => {
        try {
          return map(await call(...(args as unknown[])))
        } catch (error) {
          return guardAuthError(error)
        }
      },
      // 跨表影响无法从签名推断，默认整体失效；调用方可用 options.onSuccess 覆盖。
      onSuccess: () => {
        void queryClient.invalidateQueries({ queryKey: [ROOT] })
      },
      ...options,
    })

    // `mutate(...args)` / `mutateAsync(...args)` 直接转发到原方法的参数列表。
    return {
      ...result,
      mutate: (...args: ArgsOf<F>) => result.mutate(args),
      mutateAsync: (...args: ArgsOf<F>) => result.mutateAsync(args),
    } as MutationResult<O, ArgsOf<F>>
  }

  // 直接调用同样经过 map，保证与 hook 返回一致。
  const direct = async (...args: ArgsOf<F>) => map(await call(...(args as unknown[])))

  return Object.assign(direct, {
    useQuery: useQueryHook,
    useMutation: useMutationHook,
  }) as unknown as WithHooks<F, O>
}

/**
 * 方法的 UI 返回类型。
 *
 * 纯类型声明，与 `types.ts` 对应；`toUi` 在运行时完成结构转换。未列出的方法
 * （返回 `Empty` 的删除/登出等）保持 protobuf 类型。
 */
type UI_RESULT = {
  me: AdminView
  getStats: Stats
  listNodes: NodeView[]
  createNode: CreateNodeResponse
  updateNode: NodeView
  listUsers: ProxyUserView[]
  createUser: ProxyUserView
  updateUser: ProxyUserView
  listAdmins: AdminView[]
  createAdmin: AdminView
  listLinks: LinkView[]
  createLink: LinkView
  getTopology: TopologyResponse
  listAccessLogs: AccessLogPage
  getTrafficTopology: TrafficTopology
  getSubscription: SubscriptionInfo
  getVersionInfo: VersionInfo
  updateAllNodes: { requested: number; online: number }
  // protobuf 的 admin 字段是可选的，保留 undefined 让调用方处理。
  login: { token: string; admin?: AdminView }
  setup: { token: string; admin?: AdminView }
  getSetupStatus: SetupStatus
  getTrafficOverview: Array<{
    userId: string
    name: string
    ownerEmail: string
    upBytes: number
    downBytes: number
  }>
}

/** 客户端类型：每个 proto 方法都带 hook，返回值取 `UI_RESULT`。 */
type Client = {
  [K in keyof typeof raw]: WithHooks<
    (typeof raw)[K] extends (...args: infer A) => unknown ? (...args: A) => Promise<unknown> : never,
    K extends keyof UI_RESULT ? UI_RESULT[K] : Awaited<ReturnType<(typeof raw)[K]>>
  >
}

/**
 * tRPC 客户端单例。
 *
 * 每个方法的 hook 都在模块加载时创建；hook 内部调用 React 的 `useQuery` /
 * `useMutation`，因此只能在组件或自定义 hook 中调用（与 tRPC 相同）。
 */
export const client = Object.fromEntries(
  Object.entries(raw).map(([name, value]) => [
    name,
    typeof value === 'function' ? decorate(name, value as ApiFn, mapperFor(name)) : value,
  ]),
) as Client

export type { UseMutationResult, UseQueryResult }
