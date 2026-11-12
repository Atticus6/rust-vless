# HTTP API

主节点 HTTP 服务同时提供 Web、API、VLESS `/ws` 与健康检查。
认证使用 `Authorization: Bearer <session token>`。
`admin` 可访问全部资源；`user` 只能访问自己名下代理用户相关数据。

## 1. 认证

| 方法 | 路径 | 权限 | 请求 | 响应 |
|---|---|---|---|---|
| POST | `/api/auth/login` | 公开 | `{email,password}` | `{token, admin}` |
| POST | `/api/auth/logout` | 登录 | — | `{ok:true}` |
| GET | `/api/auth/me` | 登录 | — | `AdminView` |

`AdminView`：`{id,email,role,createdAt}`。

## 2. 统计

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/stats` | 登录 | `{nodesTotal,nodesOnline,usersTotal,usersEnabled,adminsTotal,upBytes,downBytes,activeConnections}` |

`user` 的 users/traffic 字段只统计自己名下。

## 3. 节点

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/nodes` | admin | 节点列表 |
| POST | `/api/nodes` | admin | `{name}` → `{node, token}`；token 只返回一次 |
| PATCH | `/api/nodes/{id}` | admin | `{name?,httpUrl?,rpcUrl?}` |
| DELETE | `/api/nodes/{id}` | admin | 删除节点；主节点禁止删除 |

`NodeView`：`{id,name,status,version,httpUrl,rpcUrl,isMaster,lastSeen,createdAt,activeConnections,uptimeSec}`。

## 4. 连线

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/links` | admin | 连线列表 |
| POST | `/api/links` | admin | `{fromNodeId,toNodeId,transport?,priority?}` |
| DELETE | `/api/links/{id}` | admin | 删除连线 |
| GET | `/api/topology` | admin | 节点 + 连线原始数据 |

`LinkView`：`{id,fromNodeId,toNodeId,fromName,toName,transport,priority,enabled,createdAt}`。

约束：from/to 必须存在且不同；同一对节点只允许一条连线。

## 5. 代理用户

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/users` | 登录 | 列表（owner 隔离） |
| POST | `/api/users` | 登录 | `{name, ownerId?}`；ownerId 仅 admin 可用 |
| PATCH | `/api/users/{id}` | 登录 | `{name?, enabled?}` |
| DELETE | `/api/users/{id}` | 登录 | 删除 |

`ProxyUserView`：`{id,name,enabled,ownerId,ownerEmail,createdAt,upBytes,downBytes}`。

## 6. 管理账号

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/admins` | admin | 列表 |
| POST | `/api/admins` | admin | `{email,password,role?}` |
| DELETE | `/api/admins/{id}` | admin | 禁止删除自己或最后一个 admin |

## 7. 访问记录

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/access-logs` | 登录 | 分页 + 筛选 |

Query：

- `userId`：代理用户 UUID
- `nodeId`：入口节点
- `status`：`ok/error/timeout/denied`
- `target`：目标关键字（LIKE）
- `from` / `to`：毫秒时间戳
- `limit`（默认 50，最大 200）/ `offset`

响应：`{items:[AccessLogView], total, limit, offset}`。

`AccessLogView`：`{id,sessionId,userId,entryNodeId,target,command,startedAt,endedAt,durationMs,upBytes,downBytes,status,error,path,clientAddr}`。

## 8. 流量拓扑

| 方法 | 路径 | 权限 | 说明 |
|---|---|---|---|
| GET | `/api/traffic/topology?window=3600` | 登录 | 拓扑图数据 |

响应：

```json
{
  "windowSec": 3600,
  "nodes": [
    {"id":"...","name":"master","isMaster":true,"online":true,"activeConnections":2}
  ],
  "links": [
    {"id":"...","fromNodeId":"...","toNodeId":"...","transport":"rpc",
     "upBytes":123,"downBytes":456,"sessions":3}
  ]
}
```

- 窗口支持 3600 / 86400 / 604800 秒。
- `user` 看到的是全局节点结构，但流量只统计自己名下用户的会话；如果无法按用户拆分连线流量，则对 user 隐藏数值，仅展示结构。

## 9. 健康检查

| 方法 | 路径 | 权限 | 响应 |
|---|---|---|---|
| GET | `/health` | 公开 | `{ok:true,service:"rust-vless-master",version:"..."}` |

## 10. 错误格式

```json
{"error":"message"}
```

状态码：400 参数错误、401 未登录、403 无权限、404 不存在、409 冲突、500 内部错误。
