# rust-vless

Rust 版 VLESS over WebSocket 代理服务，包含主节点（控制面）与子节点（数据面）。主节点提供 Web 管理端、Turso SQLite 存储、gRPC 控制通道；任意节点都可以作为其他节点的中转，操作员通过 Web 拓扑图配置有向连线。

## 特性

- **主节点 / 子节点**：子节点主动连接主节点，控制端口无需公网可达。
- **HTTP + RPC**：每个节点（含主节点）同时提供 HTTP（VLESS/WS、健康检查）与 RPC（控制、中转）。
- **可编排中转**：操作员在 `/connections` 页面拖拽建立有向连线，支持多跳、优先级、RPC/HTTP 两种传输；自动拒绝成环。
- **多用户**：admin/user 管理账号 + 按 owner 隔离的 VLESS 代理用户。
- **本地 Turso SQLite**：数据库文件默认 `master.db`，不依赖外部服务。
- **访问记录**：每次代理连接记录用户、入口节点、目标、字节数、耗时、状态和实际路径。
- **流量拓扑图**：`/traffic` 页面展示节点间窗口流量、会话数与访问记录。
- **前端**：Vite + React + TanStack Router + HeroUI v3 + `@xyflow/react`；浏览器通过 gRPC-Web 调用主节点。

## 架构

```text
浏览器 ──gRPC-Web──▶ Master
                 HTTP: Web UI / JSON API / VLESS WS / health
                 RPC : WebAdmin（gRPC-Web）+ NodeControl + Relay
                 Turso SQLite (master.db)
                 ├── 控制 RPC（节点主动拨号）
                 └── 中转 RPC / HTTP（按连线）

Node A ──连线──▶ Node B ──▶ 出口直连目标
```

- 入口节点负责用户鉴权、会话生命周期、访问记录和用户流量统计。
- 中转节点按出边 priority 转发；无出边或全部失败时直连目标（可关闭回退）。
- `visited` + `max_hops` 运行时防环；创建连线时也会拒绝形成环的边。

## 目录

```text
rust-vless/
├── proto/rpc.proto              # gRPC 协议
├── crates/common               # VLESS 解析、用户注册表
├── crates/proto                # tonic 生成代码
├── crates/dataplane            # 共享数据面、中转、访问记录
├── crates/master               # 主节点
├── crates/node                 # 子节点
├── web                         # TanStack Router + HeroUI + Connect gRPC-Web
└── docs                        # 设计文档
```

## 快速开始

### 1. 构建

```bash
cd rust-vless
cargo build --release -p rust-vless-master -p rust-vless-node

cd web
bun install
bun run build
cd ..
```

### 2. 启动主节点

```bash
RUST_VLESS_DB=master.db \
RUST_VLESS_HTTP_ADDR=0.0.0.0:8080 \
RUST_VLESS_GRPC_ADDR=0.0.0.0:50051 \
RUST_VLESS_WEB_DIR=$PWD/web/dist \
RUST_VLESS_ADMIN_EMAIL=admin@example.com \
RUST_VLESS_ADMIN_PASSWORD='change-me-please' \
RUST_VLESS_MASTER_PUBLIC_HTTP_URL=http://<master>:8080 \
RUST_VLESS_MASTER_PUBLIC_RPC_URL=http://<master>:50051 \
./target/release/rust-vless-master
```

打开 `http://<master>:8080`，用 `RUST_VLESS_ADMIN_EMAIL` / `RUST_VLESS_ADMIN_PASSWORD` 登录。首次启动且 admins 表为空时会创建该账号。

### 3. 创建子节点

在「节点」页面创建节点，复制一次性 token 和启动命令：

```bash
RUST_VLESS_MASTER_GRPC=http://<master>:50051 \
RUST_VLESS_NODE_ID=<node-id> \
RUST_VLESS_NODE_TOKEN=<token> \
RUST_VLESS_NODE_NAME=tokyo \
RUST_VLESS_NODE_LISTEN=0.0.0.0:8080 \
RUST_VLESS_RPC_ADDR=0.0.0.0:50051 \
RUST_VLESS_PUBLIC_HTTP_URL=http://<node>:8080 \
RUST_VLESS_PUBLIC_RPC_URL=http://<node>:50051 \
./target/release/rust-vless-node
```

节点连上后状态变为在线，可在「连接」页面拖拽节点建立有向连线。

### 4. 客户端连接

VLESS over WebSocket：

```text
地址: ws://<入口节点>:8080/ws
UUID: 代理用户列表中的 UUID
```

主节点本身也可以作为入口：`ws://<master>:8080/ws`。

## 配置

### 主节点

| 变量 | 默认 | 说明 |
|---|---|---|
| `RUST_VLESS_DB` | `master.db` | 本地 Turso SQLite 文件 |
| `RUST_VLESS_HTTP_ADDR` | `0.0.0.0:8080` | Web/API/VLESS 监听 |
| `RUST_VLESS_GRPC_ADDR` | `0.0.0.0:50051` | 控制 + Relay gRPC |
| `RUST_VLESS_WEB_DIR` | `web/dist` | 前端构建目录 |
| `RUST_VLESS_ADMIN_EMAIL` | `admin@example.com` | 首次启动管理员 |
| `RUST_VLESS_ADMIN_PASSWORD` | 未设置时生成随机密码 | 首次启动密码；随机密码只在启动日志打印一次 |
| `RUST_VLESS_SESSION_TTL_SECS` | `604800` | 登录会话有效期 |
| `RUST_VLESS_MASTER_NODE_ID` | 自动生成并持久化 | 主节点在拓扑中的 ID |
| `RUST_VLESS_MASTER_NODE_NAME` | `master` | 主节点显示名 |
| `RUST_VLESS_MASTER_PUBLIC_HTTP_URL` | 空 | 主节点公网 HTTP 地址 |
| `RUST_VLESS_MASTER_PUBLIC_RPC_URL` | 空 | 主节点公网 RPC 地址 |
| `RUST_VLESS_MAX_CONN` | `4096` | 主节点数据面并发上限 |
| `RUST_VLESS_MAX_HOPS` | `8` | 最大中转跳数 |
| `RUST_VLESS_ALLOW_DIRECT_FALLBACK` | `true` | 出边全失败时是否直连 |
| `RUST_VLESS_ACCESS_LOG_RETENTION_DAYS` | `30` | 访问记录保留天数 |

### 子节点

| 变量 | 默认 | 说明 |
|---|---|---|
| `RUST_VLESS_MASTER_GRPC` | `http://127.0.0.1:50051` | 主节点 gRPC |
| `RUST_VLESS_NODE_ID` | 必填 | 节点 UUID |
| `RUST_VLESS_NODE_TOKEN` | 必填 | 创建节点时返回的 token |
| `RUST_VLESS_NODE_NAME` | `node` | 显示名 |
| `RUST_VLESS_NODE_LISTEN` | `0.0.0.0:8080` | VLESS/WS 监听 |
| `RUST_VLESS_RPC_ADDR` | `0.0.0.0:50051` | Relay RPC 监听 |
| `RUST_VLESS_PUBLIC_HTTP_URL` | 空 | 供其他节点 HTTP 中转 |
| `RUST_VLESS_PUBLIC_RPC_URL` | 空 | 供其他节点 RPC 中转 |
| `RUST_VLESS_MAX_CONN` | `4096` | 并发上限 |
| `RUST_VLESS_MAX_HOPS` | `8` | 最大跳数 |
| `RUST_VLESS_ALLOW_DIRECT_FALLBACK` | `true` | 出边全失败时是否直连 |

## 测试

```bash
cd rust-vless
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd web
bun install
bun run test
bun run build
```

修改 `proto/rpc.proto` 后重新生成前端类型：

```bash
cd web
bun run proto   # buf generate -> src/gen/rpc_pb.ts
```

已覆盖：

- VLESS 协议解析（IPv4/域名/IPv6、TCP/UDP、错误分支）
- 用户注册表替换与计数
- Turso 数据层：管理员/会话/节点/连线/用户/访问记录/连线流量
- HTTP API：登录、鉴权、节点、用户、连线、成环拒绝
- WebAdmin RPC：gRPC-Web 请求/响应帧、Bearer 鉴权、权限分级、分页边界
- 前端：RPC 消息映射（bigint→number、枚举→字符串）、错误归一化与 401 跳转、登录表单交互
- 端到端（手动）：直连、RPC 中转、HTTP 中转、多跳、主节点入口、成环拒绝、访问记录路径、流量拓扑
- Web RPC 冒烟（对运行中的主节点）：
  ```bash
  cd web
  export ADMIN_PASSWORD='your-admin-password'
  bun scripts/smoke.ts http://127.0.0.1:8080 "$ADMIN_PASSWORD"
  ```

## 安全说明

- 密码使用 Argon2id 哈希；会话、节点、连线 token 均为 32 字节随机数。
- 连线 token 只下发给源节点和入边目标；中转请求必须匹配 token + from + to。
- 中转链路当前为明文 HTTP/gRPC；不要跨公网直接使用 `http://` 端点，请放在 WireGuard、可信内网或 TLS 隧道内。
- 访问日志包含目标地址，注意保留周期与隐私要求。
- 初始管理员密码请立即修改。

## 已知限制

- 未集成 Cloudflare/Argo 隧道；节点需要可被客户端/其他节点访问的地址。
- 访问记录与用户流量只由入口节点统计；中转节点只上报连线流量。
- 控制面断线瞬间，已进入节点本地队列但尚未送达主节点的少量访问记录/流量可能丢失；缓冲区会跨重连保留。
- 前端构建产物约 1MB（HeroUI + React Aria + 拓扑图库，未做路由级拆包）。

## CodeRabbit

每次改动完成后、提交前必须审查（规则见 `AGENTS.md`）：

```bash
coderabbit review --uncommitted --include-untracked --base main
```

按 critical/major 优先修复评审意见；未能执行审查时登记到 `REVIEW_BACKLOG.md`。
