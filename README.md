# rust-vless

Rust 版 VLESS over WebSocket 代理服务，包含主节点（控制面）与子节点（数据面）。主节点提供 Web 管理端、Turso SQLite 存储、gRPC 控制通道；任意节点都可以作为其他节点的中转，操作员通过 Web 拓扑图配置有向连线。

## 特性

- **主节点 / 子节点**：子节点主动连接主节点，控制端口无需公网可达。
- **HTTP + RPC**：每个节点（含主节点）同时提供 HTTP（VLESS/WS、健康检查）与 RPC（控制、中转）。
- **可编排中转**：操作员在「连接」页面拖拽建立有向连线，支持多跳、优先级、RPC/HTTP 两种传输；自动拒绝成环。
- **多用户**：admin/user 管理账号 + 按 owner 隔离的 VLESS 代理用户；每个账号有自己的面板。
- **首次安装页面**：主节点未创建管理员时自动进入 `/setup`，无需在启动参数里明文写密码。
- **订阅导入**：用户面板提供 V2Ray（base64 URI）与 Clash（YAML）订阅，直接粘贴到客户端即可。
- **流量日志**：访问日志写入独立 JSONL 文件，用 DuckDB 过滤、分页查询，不占用业务数据库。
- **版本管理**：`version.txt` 为唯一版本源；节点上报版本，管理员可一键通知所有在线节点升级。
- **国际化**：`zh.ts` 为唯一事实来源，`en.ts` 用 `Record<MessageKey, string>` 约束；`t()` 只接受已声明的 key，漏翻或多出 key 都会编译失败。
- **前端**：Vite + React + TanStack Router（文件路由）+ HeroUI v3 + Tailwind CSS v4（Vite 插件）+ `@xyflow/react`；浏览器通过 gRPC-Web 调用主节点。
- **RPC 查询**：`client.listUsers.useQuery()` / `client.createUser.useMutation()` 的用法与 tRPC 一致，类型由 `connect.ts` 的函数签名自动推导，不维护路由表。
- **测试**：Rust 单元/集成测试 + `bun test` 纯函数测试，CI 与发布流水线齐全。

## 架构

```text
浏览器 ──gRPC-Web──▶ Master
                 HTTP: Web UI / JSON API / 订阅 / VLESS WS / health
                 RPC : WebAdmin（gRPC-Web）+ NodeControl + Relay
                 Turso SQLite (master.db)
                 DuckDB 查询 access_logs.jsonl（访问日志）
                 ├── 控制 RPC（节点主动拨号）
                 └── 中转 RPC / HTTP（按连线）

Node A ──连线──▶ Node B ──▶ 出口直连目标
```

- 入口节点负责用户鉴权、会话生命周期、访问记录和用户流量统计。
- 中转节点按出边 priority 转发；无出边或全部失败时直连目标（可关闭回退）。
- `visited` + `max_hops` 运行时防环；创建连线时也会拒绝形成环的边。
- 访问日志按 JSON Lines 追加到独立文件，查询时加载进内存表由 DuckDB 执行 SQL；保留策略按天裁剪文件。

## 目录

```text
rust-vless/
├── version.txt                  # 发布版本（唯一来源）
├── proto/rpc.proto              # gRPC 协议
├── crates/common               # VLESS 解析、用户注册表、版本号
├── crates/proto                # tonic 生成代码
├── crates/dataplane            # 共享数据面、中转、访问记录
├── crates/master               # 主节点（服务层、gRPC-Web、订阅、DuckDB）
├── crates/node                 # 子节点（含自升级指令处理）
├── web                         # TanStack 文件路由 + HeroUI + Connect gRPC-Web
├── scripts/install.sh          # 一键安装脚本
├── Dockerfile                  # 主节点 / 子节点镜像
├── .github/workflows           # CI 与 Release
└── docs                        # 设计文档
```

## 快速开始

### 方式一：一键安装

Linux（含 systemd 服务）：

```bash
curl -fsSL https://raw.githubusercontent.com/Atticus6/rust-vless/main/scripts/install.sh | bash -s -- --systemd
```

macOS（不使用 systemd，只安装二进制与前端）：

```bash
curl -fsSL https://raw.githubusercontent.com/Atticus6/rust-vless/main/scripts/install.sh | bash
```

脚本会下载最新 release，安装 `rust-vless-master` / `rust-vless-node` / `rust-vless-upgrade-node`，
并把前端放到 `/usr/local/share/rust-vless/web`。主节点首次启动后打开 Web 地址，用启动日志中的安装码完成安装页面。

### 方式二：Docker Compose

```bash
docker compose up -d
```

主节点监听 `8080`（Web/API）与 `50051`（gRPC）。
子节点镜像：`ghcr.io/atticus6/rust-vless:node`，启动参数见下方「创建子节点」。

### 方式三：源码构建

```bash
cargo build --release -p rust-vless-master -p rust-vless-node

cd web
bun install
bun run build
cd ..
```

### 启动主节点

```bash
RUST_VLESS_DB=master.db \
RUST_VLESS_HTTP_ADDR=0.0.0.0:8080 \
RUST_VLESS_GRPC_ADDR=0.0.0.0:50051 \
RUST_VLESS_WEB_DIR=$PWD/web/dist \
RUST_VLESS_MASTER_PUBLIC_HTTP_URL=http://<master>:8080 \
RUST_VLESS_MASTER_PUBLIC_RPC_URL=http://<master>:50051 \
./target/release/rust-vless-master
```

打开 `http://<master>:8080` 会自动进入安装页面，创建第一个管理员账号后直接登录。
首次启动日志会打印一次性**安装码**（也可以用 `RUST_VLESS_SETUP_TOKEN` 指定），安装页面必须输入它，
避免远程访问者抢先创建管理员。如果希望完全自动化，可以设置
`RUST_VLESS_ADMIN_EMAIL` / `RUST_VLESS_ADMIN_PASSWORD`，首次启动时会自动创建该账号并跳过安装页面。

### 创建子节点

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
RUST_VLESS_UPGRADE_COMMAND=/usr/local/share/rust-vless/node/rust-vless-upgrade-node \
./target/release/rust-vless-node
```

节点连上后状态变为在线，可在「连接」页面拖拽节点建立有向连线。

### 客户端连接

- **订阅（推荐）**：登录后打开「我的面板」，复制 V2Ray 或 Clash 订阅地址导入客户端。
- **手动**：VLESS over WebSocket

```text
地址: ws://<入口节点>:8080/ws
UUID: 代理用户列表中的 UUID
```

主节点本身也可以作为入口：`ws://<master>:8080/ws`。

## 订阅接口

| 路径 | 内容 |
|---|---|
| `/sub/{token}` | V2Ray 订阅：每行一个 `vless://` URI，整体 base64 |
| `/sub/{token}/clash` | Clash YAML（proxies / proxy-groups / rules） |

token 在首次访问「我的面板」或 `/api/subscription` 时生成，之后保持不变。
订阅只包含当前账号名下已启用的代理用户；服务地址取 `RUST_VLESS_MASTER_PUBLIC_HTTP_URL`，
未配置时使用请求的 `Host` 头（反向代理后请配置公网地址或 `X-Forwarded-Proto`）。

## 版本与升级

- `version.txt` 是唯一版本源，编译时通过 `include_str!` 打进二进制；节点在控制通道握手时上报该版本。
- 「版本」页面展示主节点、所有节点与最新版本；「一键更新所有节点」会向在线节点下发 `Upgrade` 指令。
- 节点收到指令后执行 `RUST_VLESS_UPGRADE_COMMAND`，目标版本通过环境变量
  `RUST_VLESS_TARGET_VERSION` 传入。安装脚本自带的 `rust-vless-upgrade-node` 会下载对应
  release、以原子重命名覆盖自身二进制并以 `SIGTERM` 结束进程，由 systemd / Docker 以新版本重启；
  无守护进程时需手动重新启动。节点回报的 `ok=true` 只表示命令已启动，最终版本以重连后的上报为准。
- 最新版本默认等于主节点版本；可通过 `RUST_VLESS_LATEST_VERSION` 指向其他版本。

## 配置

### 主节点

| 变量 | 默认 | 说明 |
|---|---|---|
| `RUST_VLESS_DB` | `master.db` | 本地 Turso SQLite 文件 |
| `RUST_VLESS_HTTP_ADDR` | `0.0.0.0:8080` | Web/API/订阅/VLESS 监听 |
| `RUST_VLESS_GRPC_ADDR` | `0.0.0.0:50051` | 控制 + Relay gRPC |
| `RUST_VLESS_WEB_DIR` | `web/dist` | 前端构建目录 |
| `RUST_VLESS_ADMIN_EMAIL` | `admin@example.com` | 自动创建管理员时使用 |
| `RUST_VLESS_ADMIN_PASSWORD` | 空 | 为空时进入安装页面；设置后首次启动自动创建管理员 |
| `RUST_VLESS_SETUP_TOKEN` | 启动时随机生成并打印 | 首次安装码；未安装时安装页面必须提交 |
| `RUST_VLESS_SESSION_TTL_SECS` | `604800` | 登录会话有效期 |
| `RUST_VLESS_MASTER_NODE_ID` | 自动生成并持久化 | 主节点在拓扑中的 ID |
| `RUST_VLESS_MASTER_NODE_NAME` | `master` | 主节点显示名 |
| `RUST_VLESS_MASTER_PUBLIC_HTTP_URL` | 空 | 主节点公网 HTTP 地址（订阅链接使用） |
| `RUST_VLESS_MASTER_PUBLIC_RPC_URL` | 空 | 主节点公网 RPC 地址 |
| `RUST_VLESS_MAX_CONN` | `4096` | 主节点数据面并发上限 |
| `RUST_VLESS_MAX_HOPS` | `8` | 最大中转跳数 |
| `RUST_VLESS_ALLOW_DIRECT_FALLBACK` | `true` | 出边全失败时是否直连 |
| `RUST_VLESS_ACCESS_LOG_FILE` | `access_logs.jsonl` | 访问日志文件（JSON Lines） |
| `RUST_VLESS_ACCESS_LOG_RETENTION_DAYS` | `30` | 访问记录保留天数 |
| `RUST_VLESS_LATEST_VERSION` | 主节点版本 | 对外公布的最新版本 |

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
| `RUST_VLESS_UPGRADE_COMMAND` | 空 | 收到升级指令时执行的命令 |

## 测试

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd web
bun install
bun run typecheck   # bun check + 自动生成文件路由
bun test            # bun 内置测试运行器（happy-dom）
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
- Turso 数据层：管理员/会话/节点/连线/用户/流量
- 访问日志存储：DuckDB 过滤、分页、可见性、保留裁剪
- HTTP API：登录、鉴权、节点、用户、连线、成环拒绝、订阅
- WebAdmin RPC：gRPC-Web 帧、Bearer 鉴权、权限分级、首次安装、版本查询、一键升级
- 前端：只测纯函数（RPC 映射、错误归一化、格式化、i18n 存储边界），不跑端到端

> 前端不做 E2E：浏览器路径由 Rust 侧的 gRPC-Web 帧测试覆盖，前端单测只验证
> 纯函数与消息转换，避免依赖运行中的主节点。

## 发布

- 推送 `v*` 标签触发 `.github/workflows/release.yml`，校验标签与 `version.txt` 一致后：
  构建 Linux x86_64/aarch64、macOS arm64、Windows x86_64 压缩包，创建 GitHub Release，
  并推送 `ghcr.io/<owner>/rust-vless`（master）与 `:node`（子节点）镜像。
- CI（`.github/workflows/ci.yml`）在每次 push / PR 上执行 Rust 与 Web 的全部检查。

## 安全说明

- 密码使用 Argon2id；会话、节点、连线、订阅 token 均为 32 字节随机数。
- 连线 token 只下发给源节点和入边目标；中转请求必须匹配 token + from + to。
- 中转链路当前为明文 HTTP/gRPC；不要跨公网直接使用 `http://` 端点，请放在 WireGuard、可信内网或 TLS 隧道内。
- 订阅地址即凭证，泄露后可被他人使用；必要时删除账号重新生成。
- 访问日志包含目标地址，注意保留周期与隐私要求。
- 初始管理员密码请立即修改。

## 已知限制

- 未集成 Cloudflare/Argo 隧道；节点需要可被客户端/其他节点访问的地址。
- 访问记录与用户流量只由入口节点统计；中转节点只上报连线流量。
- 控制面断线瞬间，已进入节点本地队列但尚未送达主节点的少量访问记录/流量可能丢失；缓冲区会跨重连保留。
- DuckDB 查询会把当前日志文件加载进内存表；日志量特别大时应缩短保留天数。
- 前端构建产物约 1MB（HeroUI + React Aria + 拓扑图库，已按路由拆包）。

## CodeRabbit

每次改动完成后、提交前必须审查（规则见 `AGENTS.md`）：

```bash
coderabbit review --uncommitted --include-untracked --base main
```

按 critical/major 优先修复评审意见；未能执行审查时登记到 `REVIEW_BACKLOG.md`。
