# 前端设计

## 1. 技术栈

- Vite + React + TypeScript
- TanStack Router：代码式路由（`createRouter` + `createRoute`），不使用文件路由插件
- Tailwind CSS + shadcn 组件
- `@xyflow/react`：拓扑图与连线编辑
- lucide-react：图标
- sonner：Toast 提示

## 2. 路由

```text
/login            登录
/                 总览（受保护）
/nodes            节点管理（admin）
/connections      拓扑连线（admin）
/users            代理用户
/team             管理账号（admin）
/traffic          流量拓扑图 + 访问记录
```

- 受保护路由统一在 `_app` 布局下，`beforeLoad` 检查 token，无 token 重定向 `/login`。
- API 返回 401 时清理 token 并跳转登录。

## 3. 页面

### 3.1 登录

- 邮箱 + 密码。
- 登录成功保存 token 到 `localStorage`，跳转首页。
- 默认管理员由主节点首次启动时创建。

### 3.2 总览

- 卡片：在线节点 / 总节点、代理用户、累计流量、活跃连接。
- 最近节点状态表。
- 快捷入口：创建节点、添加用户、查看流量拓扑。

### 3.3 节点

- 表格：名称、角色（Master/Child）、状态、HTTP 地址、RPC 地址、版本、活跃连接、最近心跳。
- 创建节点弹窗：输入名称 → 展示一次性 token 与子节点启动命令（含复制按钮）。
- 编辑：名称、公网 HTTP/RPC 地址。
- 删除：主节点不可删除。

### 3.4 连接（拓扑）

- React Flow 画布：节点为自定义 shadcn 卡片，支持拖拽连线。
- 连线弹窗：选择 transport（RPC/HTTP）、priority。
- 边标签：transport、窗口流量、会话数。
- 表格视图：from → to、transport、priority、创建时间、删除。
- 保存后调用 `/api/links`，主节点实时下发拓扑。

### 3.5 代理用户

- 表格：名称、UUID（可复制）、启用状态、owner、上行/下行。
- 创建/编辑/删除；普通 user 只能看到自己的用户。
- 启用/禁用即时下发节点。

### 3.6 团队

- 管理账号列表：邮箱、角色、创建时间。
- 创建 admin/user 账号；删除（禁止自己/最后一个 admin）。

### 3.7 流量

- 顶部统计卡：总流量、总会话、活跃用户、Top 目标。
- 流量拓扑图：
  - 节点卡片：角色、在线、活跃连接。
  - 有向边：RPC/HTTP、窗口流量、会话数；边宽按对数缩放。
  - 时间窗口：1h / 24h / 7d。
- 访问记录表：
  - 筛选：用户、入口节点、状态、目标、时间范围。
  - 列：时间、用户、入口节点、目标、协议、耗时、上行、下行、状态、路径。
  - 分页；行点击打开详情抽屉（session_id、完整 path、错误）。

## 4. shadcn 组件

至少包含：

```text
components/ui/
  button.tsx
  card.tsx
  input.tsx
  label.tsx
  table.tsx
  badge.tsx
  dialog.tsx
  select.tsx
  dropdown-menu.tsx
  tabs.tsx
  textarea.tsx
  separator.tsx
  sonner.tsx
```

主题：深色 slate 基底 + emerald 强调色；CSS 变量与 shadcn 默认变量保持一致。

## 5. API 客户端

- `lib/api.ts`：封装 `fetch`，自动附加 `Authorization`。
- 统一错误处理：非 2xx 抛出 `ApiError`，401 触发登出。
- 类型与 Rust DTO 的 camelCase 字段保持一致。
- 开发环境 Vite 代理 `/api` 到 `http://127.0.0.1:8080`。

## 6. 构建与部署

```bash
cd web
npm install
npm run build
```

输出 `web/dist`，主节点通过 `RUST_VLESS_WEB_DIR` 托管；SPA 未命中回退 `index.html`。
