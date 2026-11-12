import path from 'node:path'
import tailwindcss from '@tailwindcss/vite'
import { tanstackRouter } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [
    // 文件路由插件必须在 React 插件之前运行，以便先生成 routeTree.gen.ts。
    tanstackRouter({
      target: 'react',
      routesDirectory: './src/routes',
      generatedRouteTree: './src/routeTree.gen.ts',
      autoCodeSplitting: true,
    }),
    react(),
    // Tailwind v4 通过 Vite 插件编译，不再需要 PostCSS 配置。
    tailwindcss(),
  ],
  resolve: {
    tsconfigPaths: true,
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
      // 订阅接口由客户端（Clash/V2Ray）直接访问，开发时同样走代理。
      '/sub': 'http://127.0.0.1:8080',
    },
  },
})
