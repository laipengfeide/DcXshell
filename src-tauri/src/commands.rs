use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::Ordering,
        Arc, Mutex, MutexGuard,
    },
    time::Duration,
};

use rusqlite::{params, Connection, OptionalExtension};
use tauri::{AppHandle, Emitter, State};

use crate::{
    audit, auth, crypto::SecretBox, db, lan, models::*, ssh_client,
};

pub struct AppState {
    pub db: Mutex<Connection>,
    pub secrets: SecretBox,
    pub sessions: Mutex<HashMap<String, Arc<ssh_client::SshSession>>>,
    pub sftps: Mutex<HashMap<String, Arc<russh_sftp::client::SftpSession>>>,
    pub current_user: Mutex<Option<UserInfo>>,
    pub app: AppHandle,
    /// 本机局域网 IP（启动时探测，登录/访客进入时刷新）
    pub ip: Mutex<String>,
    /// 本机设备标识，访客日志与后续局域网互传用它标识来源
    pub device_id: String,
    /// 当前访客会话标识（访客进入后有值）
    pub guest_session: Mutex<Option<String>>,
    /// 局域网发现与互传
    pub lan: Arc<lan::LanState>,
}

impl AppState {
    pub fn new(
        conn: Connection,
        secrets: SecretBox,
        app: AppHandle,
        lan: Arc<lan::LanState>,
    ) -> Self {
        let user = auth::restore_session(&conn);
        let device_id = db::ensure_device_id(&conn);
        let ip = crate::net::local_ipv4();

        if let Some(u) = user.as_ref() {
            let _ = auth::touch_login(&conn, u.id, &ip);
            audit::record(
                &conn,
                Some(u.id),
                &device_id,
                &u.username,
                "login",
                "自动恢复登录",
                &ip,
            );
            lan.set_identity(&u.username, &ip);
        }

        Self {
            db: Mutex::new(conn),
            secrets,
            sessions: Mutex::new(HashMap::new()),
            sftps: Mutex::new(HashMap::new()),
            current_user: Mutex::new(user),
            app,
            ip: Mutex::new(ip),
            device_id,
            guest_session: Mutex::new(None),
            lan,
        }
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn is_guest(&self) -> bool {
        self.current_user
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
    }

    /// 当前登录用户的 id；访客（未登录）为 None。
    fn current_owner(&self) -> Option<i64> {
        self.current_user
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .map(|u| u.id)
    }

    pub fn ip(&self) -> String {
        self.ip.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// 重新探测本机局域网 IP 并缓存。
    fn refresh_ip(&self) -> String {
        let ip = crate::net::local_ipv4();
        *self.ip.lock().unwrap_or_else(|e| e.into_inner()) = ip.clone();
        ip
    }

    /// 写一条操作日志；内部忽略失败，不影响主流程。
    pub fn audit(&self, action: &str, target: &str) {
        let (user_id, username) = {
            let guard = self.current_user.lock().unwrap_or_else(|e| e.into_inner());
            match guard.as_ref() {
                Some(u) => (Some(u.id), u.username.clone()),
                None => (None, "访客".to_string()),
            }
        };
        let ip = self.ip();
        let conn = self.conn();
        audit::record(
            &conn,
            user_id,
            &self.device_id,
            &username,
            action,
            target,
            &ip,
        );
    }

    pub fn session(&self, client_key: &str) -> Option<Arc<ssh_client::SshSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(client_key)
            .cloned()
    }
}

fn fetch_item(conn: &Connection, id: i64) -> Result<ConnectionItem, String> {
    conn.query_row(
        "SELECT id, name, group_name, host, port, username, auth_type, key_path,
                password_enc, passphrase_enc, created_at, updated_at
         FROM connections WHERE id = ?1",
        params![id],
        |row| {
            let password_enc: Option<String> = row.get(8)?;
            let passphrase_enc: Option<String> = row.get(9)?;
            Ok(ConnectionItem {
                id: row.get(0)?,
                name: row.get(1)?,
                group_name: row.get(2)?,
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                auth_type: row.get(6)?,
                key_path: row.get(7)?,
                has_password: password_enc
                    .as_deref()
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
                has_passphrase: passphrase_enc
                    .as_deref()
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "连接不存在".to_string())
}

fn load_stored_secrets(
    state: &AppState,
    id: i64,
) -> (Option<String>, Option<String>, Option<String>) {
    let owner = state.current_owner();
    let stored = {
        let conn = state.conn();
        conn.query_row(
            "SELECT password_enc, key_path, passphrase_enc FROM connections
             WHERE id = ?1 AND owner_id IS ?2",
            params![id, owner],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten()
    };
    match stored {
        Some((pwd, key, pass)) => {
            let pwd = pwd
                .filter(|s| !s.is_empty())
                .and_then(|s| state.secrets.decrypt(&s).ok());
            let pass = pass
                .filter(|s| !s.is_empty())
                .and_then(|s| state.secrets.decrypt(&s).ok());
            (pwd, key.filter(|s| !s.is_empty()), pass)
        }
        None => (None, None, None),
    }
}

fn params_from_input(
    state: &AppState,
    input: &ConnectionInput,
) -> Result<ssh_client::ConnectParams, String> {
    let host = input.host.trim().to_string();
    if host.is_empty() {
        return Err("服务器地址不能为空".into());
    }
    let username = input.username.trim().to_string();
    if username.is_empty() {
        return Err("用户名不能为空".into());
    }
    let auth_type = if input.auth_type == "key" {
        "key".to_string()
    } else {
        "password".to_string()
    };
    let mut password = input
        .password
        .clone()
        .filter(|s| !s.is_empty());
    let mut key_path = input
        .key_path
        .clone()
        .filter(|s| !s.is_empty());
    let mut passphrase = input
        .passphrase
        .clone()
        .filter(|s| !s.is_empty());

    if let Some(id) = input.id {
        let (stored_pwd, stored_key, stored_pass) = load_stored_secrets(state, id);
        if password.is_none() {
            password = stored_pwd;
        }
        if key_path.is_none() {
            key_path = stored_key;
        }
        if passphrase.is_none() {
            passphrase = stored_pass;
        }
    }

    if auth_type == "key" && key_path.is_none() {
        return Err("私钥登录需要选择私钥文件".into());
    }

    Ok(ssh_client::ConnectParams {
        port: input.port.unwrap_or(22),
        host,
        username,
        auth_type,
        password,
        key_path,
        passphrase,
    })
}

fn params_from_db(state: &AppState, id: i64) -> Result<(ssh_client::ConnectParams, String), String> {
    let owner = state.current_owner();
    let row = {
        let conn = state.conn();
        conn.query_row(
            "SELECT name, host, port, username, auth_type, password_enc, key_path, passphrase_enc
             FROM connections WHERE id = ?1 AND owner_id IS ?2",
            params![id, owner],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u16>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "连接配置不存在".to_string())?
    };

    let (name, host, port, username, auth_type, pwd_enc, key_path, pass_enc) = row;
    let password = pwd_enc
        .filter(|s| !s.is_empty())
        .map(|s| state.secrets.decrypt(&s))
        .transpose()?;
    let passphrase = pass_enc
        .filter(|s| !s.is_empty())
        .map(|s| state.secrets.decrypt(&s))
        .transpose()?;

    Ok((
        ssh_client::ConnectParams {
            host,
            port,
            username,
            auth_type,
            password,
            key_path: key_path.filter(|s| !s.is_empty()),
            passphrase,
        },
        name,
    ))
}

// ---------------------------------------------------------------------------
// 账户
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn auth_status(state: State<'_, AppState>) -> Option<UserInfo> {
    state
        .current_user
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
pub fn register(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<UserInfo, String> {
    let ip = state.refresh_ip();
    let user = {
        let conn = state.conn();
        let user = auth::register(&conn, &username, &password)?;
        let _ = auth::touch_login(&conn, user.id, &ip);
        user
    };
    *state
        .current_user
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(user.clone());
    state.lan.set_identity(&user.username, &ip);
    state.audit("login", "注册并自动登录");
    Ok(user)
}

#[tauri::command]
pub fn login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<UserInfo, String> {
    let ip = state.refresh_ip();
    let user = {
        let conn = state.conn();
        let user = auth::login(&conn, &username, &password)?;
        let _ = auth::touch_login(&conn, user.id, &ip);
        user
    };
    *state
        .current_user
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(user.clone());
    state.lan.set_identity(&user.username, &ip);
    state.audit("login", "账号登录");
    Ok(user)
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) -> Result<(), String> {
    if state.current_owner().is_some() {
        state.audit("logout", "主动退出");
    }
    {
        let conn = state.conn();
        auth::logout(&conn)?;
    }
    *state
        .current_user
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    // 退出后不再对外公告，避免登录界面仍显示为在线可传
    let ip = state.ip();
    state.lan.set_identity("", &ip);
    Ok(())
}

/// 访客进入：记录会话标识与本机 IP，并写一条访客进入日志。
#[tauri::command]
pub fn guest_enter(state: State<'_, AppState>) -> Result<String, String> {
    let ip = state.refresh_ip();
    *state
        .guest_session
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(state.device_id.clone());
    state.lan.set_identity("访客", &ip);
    state.audit("login", "访客进入");
    Ok(ip)
}

/// 访客离开：结束本次访客会话并写退出日志。
#[tauri::command]
pub fn guest_leave(state: State<'_, AppState>) -> Result<(), String> {
    let active = state
        .guest_session
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if active.is_some() {
        state.audit("logout", "访客离开");
        let ip = state.ip();
        state.lan.set_identity("", &ip);
    }
    Ok(())
}

/// 当前本机局域网 IP。
#[tauri::command]
pub fn current_ip(state: State<'_, AppState>) -> String {
    state.ip()
}

/// 修改当前登录用户的密码（访客不可用）。
#[tauri::command]
pub fn change_password(
    state: State<'_, AppState>,
    old_password: String,
    new_password: String,
) -> Result<(), String> {
    let user_id = state
        .current_owner()
        .ok_or_else(|| "访客模式不支持修改密码".to_string())?;
    let conn = state.conn();
    auth::change_password(&conn, user_id, &old_password, &new_password)
}

/// 读取当前身份的操作日志（时间倒序）：登录用户只看自己，访客只看本机访客记录。
#[tauri::command]
pub fn list_audit_logs(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<AuditLogItem>, String> {
    let limit = limit.unwrap_or(500).clamp(1, 2000) as i64;
    let owner = state.current_owner();
    let conn = state.conn();
    let sql = if owner.is_some() {
        "SELECT id, action, target, ip, username, created_at FROM audit_logs
         WHERE user_id = ?1 ORDER BY id DESC LIMIT ?2"
    } else {
        "SELECT id, action, target, ip, username, created_at FROM audit_logs
         WHERE user_id IS NULL AND session_id = ?1 ORDER BY id DESC LIMIT ?2"
    };
    let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
    // 登录用户按 user_id 过滤；访客记录写入时以设备标识作为会话标识，这里按同一标识过滤
    let key = match owner {
        Some(id) => rusqlite::types::Value::Integer(id),
        None => rusqlite::types::Value::Text(state.device_id.clone()),
    };
    let rows = stmt
        .query_map(params![key, limit], |row| {
            Ok(AuditLogItem {
                id: row.get(0)?,
                action: row.get(1)?,
                target: row.get(2)?,
                ip: row.get(3)?,
                username: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// 记录一条终端命令（前端在用户回车后调用），密码形态先脱敏。
#[tauri::command]
pub fn log_command(state: State<'_, AppState>, client_key: String, command: String) {
    let sanitized = audit::sanitize_command(command.trim());
    if sanitized.is_empty() {
        return;
    }
    let target = match state.session(&client_key) {
        Some(session) => format!("{}（{}）: {}", session.name, session.host, sanitized),
        None => format!("未知会话: {sanitized}"),
    };
    state.audit("command", &target);
}

// ---------------------------------------------------------------------------
// 连接配置
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionItem>, String> {
    let owner = state.current_owner();
    let conn = state.conn();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, group_name, host, port, username, auth_type, key_path,
                    password_enc, passphrase_enc, created_at, updated_at
             FROM connections WHERE owner_id IS ?1 ORDER BY group_name, name, id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![owner], |row| {
            let password_enc: Option<String> = row.get(8)?;
            let passphrase_enc: Option<String> = row.get(9)?;
            Ok(ConnectionItem {
                id: row.get(0)?,
                name: row.get(1)?,
                group_name: row.get(2)?,
                host: row.get(3)?,
                port: row.get(4)?,
                username: row.get(5)?,
                auth_type: row.get(6)?,
                key_path: row.get(7)?,
                has_password: password_enc
                    .as_deref()
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
                has_passphrase: passphrase_enc
                    .as_deref()
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_connection(
    state: State<'_, AppState>,
    input: ConnectionInput,
) -> Result<ConnectionItem, String> {
    let owner = state.current_owner();
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err("连接名称不能为空".into());
    }
    let host = input.host.trim().to_string();
    if host.is_empty() {
        return Err("服务器地址不能为空".into());
    }
    let username = input.username.trim().to_string();
    if username.is_empty() {
        return Err("用户名不能为空".into());
    }
    let auth_type = if input.auth_type == "key" {
        "key".to_string()
    } else {
        "password".to_string()
    };
    let key_path = if auth_type == "key" {
        let path = input.key_path.clone().unwrap_or_default().trim().to_string();
        if path.is_empty() {
            return Err("私钥登录需要选择私钥文件".into());
        }
        Some(path)
    } else {
        None
    };
    let group_name = input
        .group_name
        .clone()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "默认分组".to_string());

    let password_enc = input
        .password
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| state.secrets.encrypt(s))
        .transpose()?;
    let passphrase_enc = input
        .passphrase
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| state.secrets.encrypt(s))
        .transpose()?;

    let conn = state.conn();
    let now = db::now();
    let id = match input.id {
        Some(id) => {
            let existing = conn
                .query_row(
                    "SELECT password_enc, passphrase_enc FROM connections
                     WHERE id = ?1 AND owner_id IS ?2",
                    params![id, owner],
                    |row| {
                        Ok((
                            row.get::<_, Option<String>>(0)?,
                            row.get::<_, Option<String>>(1)?,
                        ))
                    },
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "连接不存在".to_string())?;

            let final_password = if auth_type == "key" {
                None
            } else {
                password_enc.or(existing.0).or(None)
            };
            let final_passphrase = passphrase_enc.or(existing.1);

            conn.execute(
                "UPDATE connections
                 SET name = ?1, group_name = ?2, host = ?3, port = ?4, username = ?5,
                     auth_type = ?6, password_enc = ?7, key_path = ?8, passphrase_enc = ?9,
                     updated_at = ?10
                 WHERE id = ?11 AND owner_id IS ?12",
                params![
                    name,
                    group_name,
                    host,
                    input.port.unwrap_or(22),
                    username,
                    auth_type,
                    final_password,
                    key_path,
                    final_passphrase,
                    now,
                    id,
                    owner
                ],
            )
            .map_err(|e| e.to_string())?;
            id
        }
        None => {
            conn.execute(
                "INSERT INTO connections
                 (name, group_name, host, port, username, auth_type, password_enc, key_path,
                  passphrase_enc, owner_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
                params![
                    name,
                    group_name,
                    host,
                    input.port.unwrap_or(22),
                    username,
                    auth_type,
                    password_enc,
                    key_path,
                    passphrase_enc,
                    owner,
                    now
                ],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid()
        }
    };

    fetch_item(&conn, id)
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let owner = state.current_owner();
    {
        let conn = state.conn();
        conn.execute(
            "DELETE FROM connections WHERE id = ?1 AND owner_id IS ?2",
            params![id, owner],
        )
        .map_err(|e| e.to_string())?;
    }
    let targets: Vec<Arc<ssh_client::SshSession>> = state
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|s| s.conn_id == id)
        .cloned()
        .collect();
    for session in targets {
        let key = session.client_key.clone();
        state
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
        session.shutdown().await;
    }
    Ok(())
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    input: ConnectionInput,
) -> Result<TestResult, String> {
    let params = params_from_input(&state, &input)?;
    Ok(ssh_client::test(&params).await)
}

// ---------------------------------------------------------------------------
// SSH 会话
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn open_session(
    state: State<'_, AppState>,
    client_key: String,
    connection_id: i64,
    cols: u32,
    rows: u32,
) -> Result<SessionInfo, String> {
    let is_guest = state.is_guest();
    {
        let mut sessions = state.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.retain(|_, s| s.alive.load(Ordering::SeqCst));
        if is_guest && !sessions.contains_key(&client_key) && sessions.len() >= 3 {
            return Err("访客模式最多同时打开 3 个并行窗口，登录后可无限打开。".into());
        }
        if let Some(old) = sessions.remove(&client_key) {
            crate::sftp::clear_sftp(&state, &client_key);
            tokio::spawn(async move { old.shutdown().await });
        }
    }

    let (params, name) = params_from_db(&state, connection_id)?;
    let (host, port, username) = (params.host.clone(), params.port, params.username.clone());

    let established = tokio::time::timeout(
        Duration::from_secs(20),
        ssh_client::establish(&params, cols, rows),
    )
    .await
    .map_err(|_| "连接超时：20 秒内未能建立 SSH 会话".to_string())??;

    let session_id = uuid::Uuid::new_v4().to_string();
    let session = Arc::new(ssh_client::SshSession {
        id: session_id.clone(),
        client_key: client_key.clone(),
        conn_id: connection_id,
        name: name.clone(),
        host: host.clone(),
        port,
        username: username.clone(),
        connected_at: chrono::Local::now().timestamp(),
        alive: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        bytes_in: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        write: established.write,
        handle: tokio::sync::Mutex::new(established.handle),
    });

    {
        let mut sessions = state.sessions.lock().unwrap_or_else(|e| e.into_inner());
        sessions.insert(client_key.clone(), session.clone());
    }

    ssh_client::spawn_reader(state.app.clone(), session, established.read);

    state.audit("connect", &format!("{name}（{host}:{port}）"));

    Ok(SessionInfo {
        client_key,
        session_id,
        name,
        host,
        port,
        username,
        connected_at: chrono::Local::now().timestamp(),
    })
}

#[tauri::command]
pub async fn write_session(
    state: State<'_, AppState>,
    client_key: String,
    data: String,
) -> Result<(), String> {
    let session = state
        .session(&client_key)
        .ok_or_else(|| "会话不存在或已关闭".to_string())?;
    if !session.alive.load(Ordering::SeqCst) {
        return Err("连接已断开".into());
    }
    if data.is_empty() {
        return Ok(());
    }
    let bytes = bytes::Bytes::from(data.into_bytes());
    match session.write.data_bytes(bytes).await {
        Ok(()) => Ok(()),
        Err(e) => {
            session.alive.store(false, Ordering::SeqCst);
            Err(format!("发送失败：{e}"))
        }
    }
}

#[tauri::command]
pub async fn resize_session(
    state: State<'_, AppState>,
    client_key: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    if let Some(session) = state.session(&client_key) {
        let _ = session
            .write
            .window_change(cols.max(1), rows.max(1), 0, 0)
            .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn close_session(state: State<'_, AppState>, client_key: String) -> Result<(), String> {
    let session = state
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&client_key);
    if let Some(session) = session {
        session.shutdown().await;
    }
    crate::sftp::clear_sftp(&state, &client_key);
    Ok(())
}

#[tauri::command]
pub async fn ping_session(
    state: State<'_, AppState>,
    client_key: String,
) -> Result<PingResult, String> {
    let Some(session) = state.session(&client_key) else {
        return Ok(PingResult {
            alive: false,
            latency_ms: 0,
            message: Some("会话不存在".into()),
        });
    };
    let (alive, latency_ms) = session.ping().await;
    Ok(PingResult {
        alive,
        latency_ms,
        message: if alive {
            None
        } else {
            Some("心跳超时，连接可能已断开".into())
        },
    })
}

#[tauri::command]
pub fn active_session_count(state: State<'_, AppState>) -> usize {
    state
        .sessions
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter(|s| s.alive.load(Ordering::SeqCst))
        .count()
}

#[tauri::command]
pub fn notify_ready(state: State<'_, AppState>) {
    let _ = state.app.emit("dcx:ready", ());
}

// ---------------------------------------------------------------------------
// 局域网互传
// ---------------------------------------------------------------------------

/// 本机互传状态：显示名、IP、端口、监听是否正常、默认保存目录。
#[tauri::command]
pub fn lan_status(state: State<'_, AppState>) -> lan::LanStatus {
    state.lan.status()
}

/// 当前在线、可互传的用户。
#[tauri::command]
pub fn lan_peers(state: State<'_, AppState>) -> Vec<lan::PeerView> {
    state.lan.peers()
}

/// 向指定在线用户发送一个文件，返回传输标识。
#[tauri::command]
pub fn lan_send_file(
    state: State<'_, AppState>,
    path: String,
    peer_id: String,
) -> Result<String, String> {
    state.lan.spawn_send(path, peer_id)
}

/// 接收方回应传入的文件请求。
#[tauri::command]
pub fn lan_respond(
    state: State<'_, AppState>,
    id: String,
    accept: bool,
    save_dir: Option<String>,
) -> Result<(), String> {
    state.lan.resolve(
        &id,
        accept,
        save_dir.map(std::path::PathBuf::from),
    )
}

/// 在系统文件管理器里打开一个本地路径（可指向文件，会自动选中）。
/// Windows 下调用 `explorer /select,"<path>"`；其他平台用系统默认 open。
#[tauri::command]
pub fn open_path(path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("路径不存在：{path}"));
    }
    #[cfg(windows)]
    {
        // explorer /select,"<fullpath>" 会自动在资源管理器里定位并选中该文件/目录
        let arg = format!("/select,\"{}\"", p.display());
        std::process::Command::new("explorer")
            .arg(&arg)
            .spawn()
            .map_err(|e| format!("无法打开文件管理器：{e}"))?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("open 失败：{e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("xdg-open 失败：{e}"))?;
    }
    Ok(())
}
