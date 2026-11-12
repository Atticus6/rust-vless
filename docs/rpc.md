# RPC 协议

所有节点同时运行 gRPC 服务端与（子节点）控制客户端。协议定义在 `proto/rpc.proto`。

## 1. 服务

```proto
service NodeControl {
  rpc Stream(stream NodeMessage) returns (stream MasterMessage);
}

service Relay {
  rpc Forward(stream RelayPacket) returns (stream RelayPacket);
}
```

- `NodeControl.Stream`：子节点 → 主节点，持久双向流。
- `Relay.Forward`：节点 → 节点，按连线转发 VLESS 帧。

## 2. 消息

```proto
message NodeMessage {
  oneof payload {
    Hello hello = 1;
    Heartbeat heartbeat = 2;
    TrafficReport traffic = 3;
    AccessLogBatch access_logs = 4;
    LinkTrafficReport link_traffic = 5;
  }
}

message MasterMessage {
  oneof payload {
    Accepted accepted = 1;
    ConfigUpdate config = 2;
    TopologyUpdate topology = 3;
  }
}

message Hello {
  string node_id = 1;
  string token = 2;
  string version = 3;
  string http_url = 4;
  string rpc_url = 5;
}

message Accepted {
  int64 server_time_ms = 1;
  string node_name = 2;
  int64 heartbeat_interval_sec = 3;
}

message Heartbeat {
  int64 timestamp_ms = 1;
  uint64 active_connections = 2;
  uint64 uptime_sec = 3;
  int32 cpu_percent = 4;
  uint64 memory_bytes = 5;
}

message TrafficReport { repeated UserTraffic users = 1; }
message UserTraffic {
  string user_id = 1;
  uint64 up_bytes = 2;
  uint64 down_bytes = 3;
}

message AccessLogBatch { repeated AccessLog logs = 1; }
message AccessLog {
  string session_id = 1;
  string user_id = 2;
  string entry_node_id = 3;
  string target = 4;
  string command = 5;
  int64 started_at_ms = 6;
  int64 ended_at_ms = 7;
  uint64 up_bytes = 8;
  uint64 down_bytes = 9;
  string status = 10;
  string error = 11;
  repeated string path = 12;
  string client_addr = 13;
}

message LinkTrafficReport { repeated LinkTraffic links = 1; }
message LinkTraffic {
  string link_id = 1;
  uint64 up_bytes = 2;
  uint64 down_bytes = 3;
  uint64 sessions = 4;
}

message ConfigUpdate {
  repeated VlessUser users = 1;
  int64 revision = 2;
}
message VlessUser { string id = 1; string name = 2; bool enabled = 3; }

message TopologyUpdate {
  repeated OutboundLink outbound = 1;
  repeated InboundLink inbound = 2;
  int64 revision = 3;
}
message OutboundLink {
  string link_id = 1;
  string to_node_id = 2;
  string to_name = 3;
  string transport = 4;
  string endpoint = 5;
  string token = 6;
  int64 priority = 7;
}
message InboundLink {
  string link_id = 1;
  string from_node_id = 2;
  string token = 3;
  string transport = 4;
}
```

## 3. 中转消息

```proto
message RelayPacket {
  oneof payload {
    RelayHello hello = 1;
    RelayReady ready = 2;
    bytes data = 3;
  }
}

message RelayHello {
  string link_token = 1;
  string from_node_id = 2;
  bytes vless_header = 3;
  repeated string visited = 4;
  bool report_traffic = 5;
}

message RelayReady { repeated string path = 1; }
```

### 3.1 RPC 中转流程

1. 源节点建立 `Relay.Forward` 流。
2. 发送 `RelayHello`（link token、from、原始 VLESS header、visited、report flag）。
3. 目标节点校验入边 token；解析 VLESS header；追加自身到 visited。
4. 目标节点决定继续中转或直连：
   - 继续中转：向下游发起新的 `Relay.Forward`。
   - 直连：连接目标，成功后向上游发送 `RelayReady { path }`。
5. 收到下游 `RelayReady` 的节点把完整 path 继续回传。
6. 之后双向 `data` 帧转发；入口节点把数据写回客户端 WebSocket。
7. 结束后关闭流，入口节点生成访问记录。

### 3.2 HTTP 中转流程

1. 源节点用 WebSocket 客户端连接目标 `ws://<http_url>/ws`。
2. 请求头携带：
   - `X-Rust-Vless-Link-Token`
   - `X-Rust-Vless-From`
   - `X-Rust-Vless-Visited`（逗号分隔）
   - `X-Rust-Vless-Report-Traffic: 0`
3. 首帧发送原始 VLESS header；之后转发二进制帧。
4. 目标节点在升级后先发送元数据帧（magic `RVL1` + JSON 字符串数组 `["node-id", ...]`），再发送正常 VLESS 响应与数据。
5. 源节点消费元数据帧得到完整 path，不把它转发给客户端。

## 4. 控制流时序

```text
Node                                 Master
 | -- Hello(node_id, token, urls) -----> |
 | <-- Accepted ----------------------- |
 | <-- ConfigUpdate(users) ------------ |
 | <-- TopologyUpdate(out/in links) --- |
 | -- Heartbeat(10s) ------------------> |
 | -- TrafficReport(10s) --------------> |
 | -- AccessLogBatch(5s/100) ----------> |
 | -- LinkTrafficReport(10s) ----------> |
 | <-- ConfigUpdate(用户变更时) -------- |
 | <-- TopologyUpdate(连线变更时) ------ |
```

- 所有下发按节点定向，不使用全局广播。
- 节点重连后先收到完整用户快照与自己的拓扑快照。

## 5. 鉴权

- 控制流：`nodes.id + nodes.token`。
- 中转流：`node_links.from_node_id + to_node_id + token`。
- 节点本地缓存入边 token，不每次回源主节点。
- 失效重连：子节点 5 秒退避重试。
