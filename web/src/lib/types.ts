export type Role = 'admin' | 'user'

export type AdminView = {
  id: string
  email: string
  role: Role
  createdAt: number
}

export type LoginResponse = {
  token: string
  admin: AdminView
}

export type Stats = {
  nodesTotal: number
  nodesOnline: number
  usersTotal: number
  usersEnabled: number
  adminsTotal: number
  upBytes: number
  downBytes: number
  activeConnections: number
  linksTotal: number
}

export type NodeView = {
  id: string
  name: string
  status: string
  version: string
  httpUrl: string
  rpcUrl: string
  isMaster: boolean
  lastSeen: number
  createdAt: number
  activeConnections: number
  uptimeSec: number
}

export type LinkView = {
  id: string
  fromNodeId: string
  toNodeId: string
  fromName: string
  toName: string
  transport: 'rpc' | 'http'
  priority: number
  enabled: boolean
  createdAt: number
}

export type ProxyUserView = {
  id: string
  name: string
  enabled: boolean
  ownerId: string
  ownerEmail: string
  createdAt: number
  upBytes: number
  downBytes: number
}

export type AccessLogView = {
  id: number
  sessionId: string
  userId: string
  entryNodeId: string
  target: string
  command: 'tcp' | 'udp'
  startedAt: number
  endedAt: number
  durationMs: number
  upBytes: number
  downBytes: number
  status: 'ok' | 'error' | 'timeout' | 'denied'
  error: string
  path: string[]
  clientAddr: string
}

export type AccessLogPage = {
  items: AccessLogView[]
  total: number
  limit: number
  offset: number
}

export type TopologyNode = {
  id: string
  name: string
  isMaster: boolean
  online: boolean
  activeConnections: number
}

export type TopologyLink = {
  id: string
  fromNodeId: string
  toNodeId: string
  transport: 'rpc' | 'http'
  upBytes: number
  downBytes: number
  sessions: number
}

export type TrafficTopology = {
  windowSec: number
  nodes: TopologyNode[]
  links: TopologyLink[]
}

export type TopologyResponse = {
  nodes: NodeView[]
  links: LinkView[]
}

export type CreateNodeResponse = {
  node: NodeView
  token: string
}
