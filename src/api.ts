import { invoke } from "@tauri-apps/api/core"
import type {
  AuditLogItem,
  ConnectionInput,
  ConnectionItem,
  DirListing,
  LanPeer,
  LanStatus,
  PingResult,
  SessionInfo,
  TestResult,
  TransferResult,
  UserInfo,
} from "./types"

export const authStatus = () => invoke<UserInfo | null>("auth_status")
export const register = (username: string, password: string) =>
  invoke<UserInfo>("register", { username, password })
export const login = (username: string, password: string) =>
  invoke<UserInfo>("login", { username, password })
export const logout = () => invoke<void>("logout")
export const guestEnter = () => invoke<string>("guest_enter")
export const guestLeave = () => invoke<void>("guest_leave")
export const currentIp = () => invoke<string>("current_ip")
export const changePassword = (oldPassword: string, newPassword: string) =>
  invoke<void>("change_password", { oldPassword, newPassword })
export const listAuditLogs = (limit = 500) =>
  invoke<AuditLogItem[]>("list_audit_logs", { limit })
export const logCommand = (clientKey: string, command: string) =>
  invoke<void>("log_command", { clientKey, command })

export const listConnections = () => invoke<ConnectionItem[]>("list_connections")
export const saveConnection = (input: ConnectionInput) =>
  invoke<ConnectionItem>("save_connection", { input })
export const deleteConnection = (id: number) => invoke<void>("delete_connection", { id })

export const testConnection = (input: ConnectionInput) =>
  invoke<TestResult>("test_connection", { input })

export const openSession = (clientKey: string, connectionId: number, cols: number, rows: number) =>
  invoke<SessionInfo>("open_session", { clientKey, connectionId, cols, rows })
export const writeSession = (clientKey: string, data: string) =>
  invoke<void>("write_session", { clientKey, data })
export const resizeSession = (clientKey: string, cols: number, rows: number) =>
  invoke<void>("resize_session", { clientKey, cols, rows })
export const closeSession = (clientKey: string) => invoke<void>("close_session", { clientKey })
export const pingSession = (clientKey: string) =>
  invoke<PingResult>("ping_session", { clientKey })
export const activeSessionCount = () => invoke<number>("active_session_count")
export const sftpList = (clientKey: string, path?: string) =>
  invoke<DirListing>("sftp_list", { clientKey, path: path ?? null })
export const sftpUpload = (clientKey: string, localPaths: string[], remoteDir: string) =>
  invoke<TransferResult[]>("sftp_upload", { clientKey, localPaths, remoteDir })
export const sftpDownload = (clientKey: string, remotePath: string, localPath: string) =>
  invoke<TransferResult>("sftp_download", { clientKey, remotePath, localPath })
export const sftpRemove = (clientKey: string, remotePath: string, isDir: boolean) =>
  invoke<void>("sftp_remove", { clientKey, remotePath, isDir })
export const sftpRename = (clientKey: string, remotePath: string, newName: string) =>
  invoke<TransferResult>("sftp_rename", { clientKey, remotePath, newName })
export const sftpCopy = (
  clientKey: string,
  sourcePath: string,
  targetDir: string,
  policy: "overwrite" | "rename"
) => invoke<TransferResult>("sftp_copy", { clientKey, sourcePath, targetDir, policy })
export const sftpExtract = (clientKey: string, remotePath: string, mode: "here" | "folder") =>
  invoke<string>("sftp_extract", { clientKey, remotePath, mode })

export const lanStatus = () => invoke<LanStatus>("lan_status")
export const lanPeers = () => invoke<LanPeer[]>("lan_peers")
export const lanSendFile = (path: string, peerId: string) =>
  invoke<string>("lan_send_file", { path, peerId })
export const lanRespond = (id: string, accept: boolean, saveDir?: string | null) =>
  invoke<void>("lan_respond", { id, accept, saveDir: saveDir ?? null })

export const openPath = (path: string) => invoke<void>("open_path", { path })
