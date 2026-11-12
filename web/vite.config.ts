import path from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': path.resolve(__dirname, './src') },
  },
  server: {
    port: 5173,
    proxy: {
      // JSON API 保留给脚本使用；浏览器控制台走 gRPC-Web。
      '/api': 'http://127.0.0.1:8080',
      '/health': 'http://127.0.0.1:8080',
      // gRPC-Web 服务路径。开发环境代理保持同源，避免额外 CORS 预检，
      // 并让 Authorization 拦截器无需区分环境。
      '/rustvless.rpc.v1': 'http://127.0.0.1:8080',
    },
  },
})
