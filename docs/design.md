# rust-vless 总体设计

## 1. 目标

`rust-vless` 是 go-vless 的 Rust 重写版，采用主节点 / 子节点架构：

- 主节点：控制面 + 数据面 + Web 管理端。
- 子节点：数据面，主动连接主节点。
- 所有节点（含主节点）同时提供 HTTP 与 RPC。
- 任意节点都可以作为其他节点的中转，操作员通过 Web 连线配置有向拓扑。
- 主节点支持多用户：管理账号（admin/user）与 VLESS 代理用户（按 owner 隔离）。
- 数据库使用本地 Turso SQLite。
- 代理用户的每次连接都有访问记录，并在 Web 页面展示流量拓扑图。

## 2. 架构

```text
                       ┌────────────────────────────────────────────┐
 浏览器 ──HTTP────────▶│ Master                                     │
                       │ HTTP: Web UI / API / VLESS WS / health     │
                       │ RPC : NodeControl + Relay                  │
                       │ Turso SQLite (master.db)                   │
                       │ 自身也是拓扑中的节点                       │
                       └───────────────┬────────────────────────────┘
                                       │ 控制 RPC（节点主动拨号）
                                       │ 中转 RPC / HTTP（按连线）
                ┌──────────────────────┴───────────────────────┐
                ▼                                              ▼
        ┌───────────────┐      连线 A→B→C               ┌───────────────┐
        │ Node A        │ ─────────────────────────────▶ │ Node B        │
        │ HTTP + RPC    │                                │ HTTP + RPC    │
        └───────┬───────┘                                └───────┬───────┘
                │ VLESS/WS 客户端                               │ 出口直连
                ▼                                                ▼
```

### 2.1 节点角色

| 角色 | HTTP | RPC | 说明 |
|---|---|---|---|
| Master | Web UI、`/api/*`、`/ws` VLESS、`/health` | `NodeControl` + `Relay` | 控制面 + 数据面 |
| Child | `/`、`/ws` VLESS、`/health` | `Relay`（也挂 `NodeControl`） | 数据面 + 中转 |

- 子节点主动向主节点发起控制 RPC，控制端口无需公网可达。
- 节点间中转按连线主动拨号，因此中转目标需要 RPC/HTTP 地址可达。
- 中转链路当前为明文 HTTP/gRPC；不要跨公网直接使用，需放在可信内网或 TLS 隧道内。

### 2.2 组件

- `crates/common`：VLESS 协议解析、用户注册表、通用工具。
- `crates/proto`：tonic 生成的 gRPC 类型。
- `crates/dataplane`：帧传输抽象、VLESS 会话、路由/中转、HTTP 路由。
- `crates/master`：Turso、认证、HTTP API、控制 RPC、拓扑管理。
- `crates/node`：控制 RPC 客户端、节点数据面。
- `web`：Vite + React + TanStack Router + shadcn。

## 3. 连线与中转

### 3.1 有向图

操作员在 `/connections` 页面创建有向边 `A → B`。每条边包含：

- `transport`：`rpc` 或 `http`，默认 `rpc`。
- `priority`：越小越优先。
- `token`：该连线独立的鉴权 token。
- `enabled`：可临时禁用。

节点只使用自己的出边；目标节点使用入边 token 校验中转请求。

### 3.2 转发决策

1. 入口节点解析 VLESS 头并完成用户鉴权。
2. 若本节点有启用的出边，按 priority 依次尝试下一跳。
3. 连接成功：把原始 VLESS header 与后续字节流转发给下一跳。
4. 下一跳重复同样的决策，直到某个无出边的节点。
5. 无出边，或所有出边在发送 VLESS header 前就失败：直连目标（`allow_direct_fallback` 默认 true）。已经发出 header 的中转不会回退重放，避免首包重复和路径绕过。
6. 节点要作为纯出口：不配置出边即可。

### 3.3 多跳与防环

- 中转请求携带 `visited` 节点列表与原始 VLESS header。
- 每跳把自身加入 visited。
- 若自身已在 visited 或超过 `max_hops`（默认 8），拒绝并记录 `denied`。
- 入口节点是唯一写访问记录和用户流量的一方；中转节点只上报连线流量。

### 3.4 中转传输

- **RPC**：`Relay.Forward` 双向流；首包 `RelayHello`，随后是二进制帧。
- **HTTP**：源节点作为 WebSocket 客户端连接目标 `/ws`，通过请求头传 link token / visited / report flag；目标复用同一套 WS 会话处理。
- 两种传输共用 `dataplane::frame` 的读写抽象。

## 4. 访问记录

- 入口节点在会话结束时生成一条记录。
- 每 5 秒或 100 条批量上报主节点。
- 字段：session_id、user_id、entry_node_id、target、command、时间、耗时、上下行字节、status、error、path、client_addr。
- 会话字节为独立计数，不重复累加用户总流量。
- 默认保留 30 天，主节点后台清理。

## 5. 流量拓扑

- 每个节点对每条出边统计上下行字节与会话数。
- 每 10 秒上报增量，成功后清零。
- 主节点写 `link_traffic_samples` 并聚合 `link_traffic_totals`。
- `/traffic` 页面用 `@xyflow/react` 展示节点与有向边，边宽按窗口流量缩放。
- 节点卡片展示角色、在线状态、活跃连接、入口/出口流量。

## 6. 多用户与权限

- `admin`：管理全部节点、连线、代理用户、管理账号、访问记录。
- `user`：只管理自己名下的代理用户，只能看自己的流量与访问记录。
- 所有启用代理用户的 UUID 都会下发给所有节点。
- 禁止删除自己或最后一个 admin。

## 7. 安全

- Argon2id 密码哈希；会话 token 32 字节随机数。
- 节点 token 与连线 token 均为 32 字节随机数，只在创建时展示。
- 中转请求必须匹配入边 token、from_node_id、to_node_id。
- WS 消息上限 256KB；并发信号量；读写超时；最大跳数。
- 日志不打印密码、节点 token、连线 token。

## 8. 配置

主节点：

```bash
RUST_VLESS_DB=master.db
RUST_VLESS_HTTP_ADDR=0.0.0.0:8080
RUST_VLESS_GRPC_ADDR=0.0.0.0:50051
RUST_VLESS_WEB_DIR=web/dist
RUST_VLESS_ADMIN_EMAIL=admin@example.com
RUST_VLESS_ADMIN_PASSWORD=<初始密码>
RUST_VLESS_ACCESS_LOG_RETENTION_DAYS=30
```

子节点：

```bash
RUST_VLESS_MASTER_GRPC=http://master:50051
RUST_VLESS_NODE_ID=<节点 UUID>
RUST_VLESS_NODE_TOKEN=<创建时返回的 token>
RUST_VLESS_NODE_LISTEN=0.0.0.0:8080
RUST_VLESS_RPC_ADDR=0.0.0.0:50051
RUST_VLESS_PUBLIC_HTTP_URL=http://node-a:8080
RUST_VLESS_PUBLIC_RPC_URL=http://node-a:50051
RUST_VLESS_MAX_CONN=4096
```
