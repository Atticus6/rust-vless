/**
 * protobuf 消息 → UI 类型的通用转换。
 *
 * 不手写每个消息的映射函数，而用一个递归函数处理所有 protobuf 消息：
 * - `bigint`（int64/uint64）→ `number`：UI 不使用 bigint；超出 2^53 会丢精度，
 *   仅影响展示；
 * - 枚举 → 字符串联合：只对语义明确的字段做（`role`、`transport`），其余保留
 *   数字原值；
 * - 数组与嵌套消息递归处理。
 *
 * protobuf 消息在运行时是普通对象，因此新增字段无需改这里。
 */
import { LinkTransport, WebRole } from '@/gen/rpc_pb'
import type { Role } from './types'

/**
 * 需要额外转换的字段名 → 转换函数。
 *
 * 只放语义明确的枚举字段；字段名在本 schema 内唯一，避免误伤。
 */
const FIELD_CONVERTERS: Record<string, (value: unknown) => unknown> = {
  // WebRole → 'admin' | 'user'；未知值按普通用户处理。
  role: (value) => (value === WebRole.ADMIN ? 'admin' : 'user') satisfies Role,
  // LinkTransport → 'rpc' | 'http'；未知值回退 rpc。
  transport: (value) => (value === LinkTransport.HTTP ? 'http' : 'rpc'),
}

/**
 * 把 protobuf 消息递归转换为 UI 友好对象。
 *
 * - `bigint` → `number`
 * - 数组、嵌套对象递归处理
 * - `null`/`undefined` 原样返回
 *
 * 返回值类型由调用方声明（`types.ts` 与 proto 字段一一对应）。
 */
export function toUi<T>(value: unknown): T {
  if (value === null || value === undefined) return value as T
  if (typeof value === 'bigint') return Number(value) as T
  if (Array.isArray(value)) return value.map((item) => toUi(item)) as T
  if (typeof value === 'object') {
    const source = value as Record<string, unknown>
    const out: Record<string, unknown> = {}
    for (const [key, item] of Object.entries(source)) {
      const convert = FIELD_CONVERTERS[key]
      out[key] = convert ? convert(item) : toUi(item)
    }
    return out as T
  }
  return value as T
}

/**
 * 转换可选消息字段（`T | undefined`）。
 *
 * protobuf 的可选消息在生成代码里是 `T | undefined`；缺失时返回 `{}` 而不是
 * `undefined`，调用方不必到处写非空断言。
 */
export function toUiMessage<T>(value: unknown): T {
  return toUi<T>(value ?? {})
}
