/**
 * 前端环境变量声明。
 *
 * 只声明浏览器构建时由 Vite 注入的 RPC 地址；该值决定 Bearer token 的发送
 * 目标，修改前必须确认指向可信的主节点。
 */
/// <reference types="vite/client" />

interface ImportMetaEnv {
  /**
   * Absolute base URL of the master's gRPC-Web endpoint (for example
   * `https://master.example.com`). Unset means same origin; Vite proxies the
   * service path in development.
   */
  readonly VITE_RPC_BASE_URL?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
