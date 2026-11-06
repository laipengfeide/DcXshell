export interface UserInfo {
  id: number
  username: string
  createdAt: string
}

export type AuthType = "password" | "key"

export interface ConnectionItem {
  id: number
  name: string
  groupName: string
  host: string
  port: number
  username: string
  authType: AuthType
  keyPath: string | null
  hasPassword: boolean
  hasPassphrase: boolean
  createdAt: string
  updatedAt: string
}

export interface ConnectionInput {
  id?: number | null
  name: string
  groupName?: string
  host: string
  port?: number
  username: string
  authType: AuthType
  password?: string
  keyPath?: string
  passphrase?: string
}

export interface TestResult {
  ok: boolean
  message: string
  latencyMs: number
}

export interface SessionInfo {
  clientKey: string
  sessionId: string
  name: string
  host: string
  port: number
  username: string
  connectedAt: number
}

export interface PingResult {
  alive: boolean
  latencyMs: number
  message?: string | null
}

export interface SessionOutputEvent {
  clientKey: string
  sessionId: string
  data: string
}

export interface SessionClosedEvent {
  clientKey: string
  sessionId: string
  reason: string
}

export type SessionStatus = "connecting" | "online" | "reconnecting" | "offline"

export interface RemoteEntry {
  name: string
  path: string
  isDir: boolean
  isLink: boolean
  size: number
  modified: number | null
}

export interface DirListing {
  path: string
  entries: RemoteEntry[]
  commonDirs: string[]
}

export interface TransferResult {
  name: string
  path: string
  size: number
}

export interface TransferEvent {
  clientKey: string
  direction: "upload" | "download"
  name: string
  transferred: number
  total: number
  done: boolean
}

