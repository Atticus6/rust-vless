# 数据模型（本地 Turso SQLite）

主节点使用 `turso` crate 打开本地文件（默认 `master.db`），不连接 Turso Cloud。
所有表在启动时通过 `CREATE TABLE IF NOT EXISTS` 初始化。

## 1. admins

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | 管理账号 UUID |
| email | TEXT UNIQUE | 小写邮箱 |
| password_hash | TEXT | Argon2id |
| role | TEXT | `admin` / `user` |
| created_at | INTEGER | 毫秒时间戳 |

## 2. sessions

| 列 | 类型 | 说明 |
|---|---|---|
| token | TEXT PK | 32 字节随机 hex |
| admin_id | TEXT | 关联 admins.id |
| expires_at | INTEGER | 毫秒时间戳 |

## 3. nodes

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | 节点 UUID |
| name | TEXT | 显示名 |
| token | TEXT UNIQUE | 控制 RPC token |
| status | TEXT | `online` / `offline` |
| version | TEXT | 节点版本 |
| addresses | TEXT | 旧字段，JSON 数组；保留兼容 |
| http_url | TEXT | 公网 HTTP 地址 |
| rpc_url | TEXT | 公网 RPC 地址 |
| is_master | INTEGER | 1 = 主节点，不可删除 |
| last_seen | INTEGER | 最近心跳 |
| created_at | INTEGER | 创建时间 |

## 4. node_links

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | 连线 UUID |
| from_node_id | TEXT | 源节点 |
| to_node_id | TEXT | 目标节点 |
| transport | TEXT | `rpc` / `http` |
| token | TEXT | 连线独立 token |
| priority | INTEGER | 越小越优先 |
| enabled | INTEGER | 1/0 |
| created_at | INTEGER | 创建时间 |

约束：`UNIQUE(from_node_id, to_node_id)`；禁止自连。

## 5. proxy_users

| 列 | 类型 | 说明 |
|---|---|---|
| id | TEXT PK | VLESS UUID |
| name | TEXT | 显示名 |
| enabled | INTEGER | 1/0 |
| owner_id | TEXT | 所属 admin |
| created_at | INTEGER | 创建时间 |

## 6. traffic_samples / traffic_totals

```sql
CREATE TABLE traffic_samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  node_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  up_bytes INTEGER NOT NULL,
  down_bytes INTEGER NOT NULL,
  reported_at INTEGER NOT NULL
);

CREATE TABLE traffic_totals (
  user_id TEXT PRIMARY KEY,
  up_bytes INTEGER NOT NULL DEFAULT 0,
  down_bytes INTEGER NOT NULL DEFAULT 0
);
```

只由入口节点上报；中转节点不重复累加用户流量。

## 7. access_logs

```sql
CREATE TABLE access_logs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL,
  user_id TEXT NOT NULL,
  entry_node_id TEXT NOT NULL,
  target TEXT NOT NULL,
  command TEXT NOT NULL,
  started_at INTEGER NOT NULL,
  ended_at INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  up_bytes INTEGER NOT NULL DEFAULT 0,
  down_bytes INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  error TEXT NOT NULL DEFAULT '',
  path TEXT NOT NULL DEFAULT '[]',
  client_addr TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_access_user ON access_logs(user_id, started_at);
CREATE INDEX idx_access_node ON access_logs(entry_node_id, started_at);
```

`path` 为节点 ID JSON 数组；`status` 为 `ok/error/timeout/denied`。

## 8. link_traffic_samples / link_traffic_totals

```sql
CREATE TABLE link_traffic_samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  link_id TEXT NOT NULL,
  up_bytes INTEGER NOT NULL,
  down_bytes INTEGER NOT NULL,
  sessions INTEGER NOT NULL DEFAULT 0,
  reported_at INTEGER NOT NULL
);
CREATE INDEX idx_link_samples ON link_traffic_samples(link_id, reported_at);

CREATE TABLE link_traffic_totals (
  link_id TEXT PRIMARY KEY,
  up_bytes INTEGER NOT NULL DEFAULT 0,
  down_bytes INTEGER NOT NULL DEFAULT 0,
  sessions INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
);
```

## 9. settings

| 列 | 类型 | 说明 |
|---|---|---|
| key | TEXT PK | 配置键 |
| value | TEXT | 值 |

用途：保存稳定的 `master_node_id`，避免主节点身份在重启后变化。

## 10. 保留策略

- `access_logs` 默认保留 30 天。
- `traffic_samples` / `link_traffic_samples` 默认保留 30 天。
- 主节点后台每日清理；可通过环境变量调整天数。
