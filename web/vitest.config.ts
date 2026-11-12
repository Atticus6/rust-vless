/**
 * Web 单测配置。
 *
 * 使用 jsdom 渲染 React 组件，`setupFiles` 指向 src/test/setup.ts（清理 +
 * 浏览器 API stub）；样式与 Vite 构建无关，测试中不加载 CSS。
 */
import path from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@': path.resolve(__dirname, './src') },
  },
  test: {
    environment: 'jsdom',
    globals: false,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}'],
    restoreMocks: true,
  },
})
