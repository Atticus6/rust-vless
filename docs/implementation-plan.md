# 实施计划与验收

## 1. 阶段

### 阶段 1：修复与数据层

- 修复 `/api/auth/login` 空响应问题。
- 补 HTTP API 集成测试（登录、节点、用户、权限）。
- Turso 迁移：`node_links`、`access_logs`、`link_traffic_samples`、`link_traffic_totals`、`settings`、`nodes.is_master/http_url/rpc_url`。

### 阶段 2：控制面

- 连线 CRUD 与拓扑计算。
- 按节点定向发送 `ConfigUpdate` / `TopologyUpdate`。
- 主节点自注册为拓扑节点。
- 节点连接时下发完整用户快照 + 自己的拓扑快照。

### 阶段 3：共享数据面

- 新增 `crates/dataplane`：frame/session/routing/router。
- 主节点启用 VLESS `/ws`；子节点启用 `/`、`/ws`。
- 会话级流量计数、访问记录、状态与错误。

### 阶段 4：中转

- RPC `Relay.Forward`。
- HTTP WebSocket 中转（link token / visited / report header）。
- 出边 priority、失败重试、直连回退。
- `RelayReady` / HTTP 元数据帧回传完整 path。
- 连线流量统计与上报。

### 阶段 5：访问记录与流量 API

- 访问记录批量写入、分页筛选、owner 隔离。
- 连线流量窗口聚合。
- 保留清理任务。

### 阶段 6：前端

- Vite + TanStack Router + shadcn 基础工程。
- 登录、总览、节点、连接、用户、团队、流量页面。
- React Flow 拓扑图与访问记录表。

### 阶段 7：验证与评审

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `npm run build`
- 端到端测试
- CodeRabbit 评审与修复

## 2. 测试矩阵

| 场景 | 期望 |
|---|---|
| 登录正确/错误密码 | 200 + token / 401 |
| user 访问 admin API | 403 |
| 创建节点后 token 只返回一次 | 列表不含 token |
| 子节点连接主节点 | 状态 online |
| 用户启用/禁用 | 节点配置即时更新 |
| RPC 单跳中转 | 数据经下一跳往返 |
| HTTP 单跳中转 | 数据经下一跳往返 |
| A→B→C 多跳 | 数据经 C 出口 |
| A→B→A 环 | denied，记录访问日志 |
| 无效 link token | 拒绝，不建立连接 |
| 下一跳下线 | 尝试下一条出边；已发送首包则不回退重放 |
| 访问记录 | 用户/目标/字节/耗时/status/path 正确 |
| 流量拓扑 | 连线流量与会话数增长 |
| user 访问记录隔离 | 只看自己 |
| 访问日志保留 | 过期数据被清理 |

## 3. 端到端步骤

```bash
# 1. 构建
cd rust-vless
cargo build --workspace
cd web && npm install && npm run build && cd ..

# 2. 启动主节点
RUST_VLESS_DB=/tmp/rv.db \
RUST_VLESS_HTTP_ADDR=127.0.0.1:18080 \
RUST_VLESS_GRPC_ADDR=127.0.0.1:15051 \
./target/debug/rust-vless-master

# 3. 启动两个子节点（token 从 UI/API 创建节点获取）
RUST_VLESS_MASTER_GRPC=http://127.0.0.1:15051 \
RUST_VLESS_NODE_ID=... RUST_VLESS_NODE_TOKEN=... \
RUST_VLESS_NODE_LISTEN=127.0.0.1:18081 \
RUST_VLESS_RPC_ADDR=127.0.0.1:15052 \
./target/debug/rust-vless-node

# 4. UI 登录，创建节点，连线 node-a → node-b
# 5. 创建代理用户，客户端连接 node-a /ws，目标为本地 echo 服务
# 6. 打开 /traffic 查看拓扑图与访问记录
```

## 4. CodeRabbit 流程

每个阶段完成后：

```bash
coderabbit review --agent
```

- 处理 P0/P1 后再进入下一阶段。
- 至少覆盖：RPC/拓扑、数据面/中转、访问记录/流量 API、前端。

## 5. 默认假设

- 连线为有向边，按 priority 顺序尝试。
- 无出边即出口；有出边但全部失败默认直连回退。
- 中转支持 `rpc` / `http`，默认 RPC。
- 用户流量与访问记录只由入口节点统计。
- 主节点身份由 `settings.master_node_id` 持久化。
- 保持 `rust-vless-master` / `rust-vless-node` 两个二进制。
