/**
 * bun test 的公共初始化。
 *
 * 前端只测纯函数，不加载 DOM；但会话与语言偏好依赖 `localStorage`，
 * 因此提供一个内存实现，并在每个用例结束后清空，避免状态跨用例泄漏。
 */
import { afterEach } from 'bun:test'

if (typeof globalThis.localStorage === 'undefined') {
  const data = new Map<string, string>()
  Object.defineProperty(globalThis, 'localStorage', {
    writable: true,
    value: {
      getItem: (key: string) => data.get(key) ?? null,
      setItem: (key: string, value: string) => {
        data.set(key, String(value))
      },
      removeItem: (key: string) => {
        data.delete(key)
      },
      clear: () => data.clear(),
      key: (index: number) => Array.from(data.keys())[index] ?? null,
      get length() {
        return data.size
      },
    } satisfies Storage,
  })
}

afterEach(() => {
  globalThis.localStorage?.clear()
})
