use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, path::Path};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    last_ip       TEXT,
    last_login_at TEXT
);

CREATE TABLE IF NOT EXISTS connections (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    name           TEXT NOT NULL,
    group_name     TEXT NOT NULL DEFAULT '默认分组',
    host           TEXT NOT NULL,
    port           INTEGER NOT NULL DEFAULT 22,
    username       TEXT NOT NULL,
    auth_type      TEXT NOT NULL DEFAULT 'password',
    password_enc   TEXT,
    key_path       TEXT,
    passphrase_enc TEXT,
    owner_id       INTEGER,
    sort_order     INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 操作日志：user_id 为 NULL 时表示访客，用 session_id（本机设备标识）区分
CREATE TABLE IF NOT EXISTS audit_logs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id    INTEGER,
    session_id TEXT NOT NULL DEFAULT '',
    username   TEXT NOT NULL DEFAULT '',
    action     TEXT NOT NULL,
    target     TEXT NOT NULL DEFAULT '',
    ip         TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_owner ON audit_logs(user_id, session_id, id DESC);
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA)?;
    migrate_connection_owner(&conn)?;
    migrate_user_columns(&conn)?;
    Ok(conn)
}

/// 为历史数据库补充 users.last_ip / users.last_login_at 列。
fn migrate_user_columns(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(users)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    if !columns.iter().any(|c| c == "last_ip") {
        conn.execute("ALTER TABLE users ADD COLUMN last_ip TEXT", [])?;
    }
    if !columns.iter().any(|c| c == "last_login_at") {
        conn.execute("ALTER TABLE users ADD COLUMN last_login_at TEXT", [])?;
    }
    Ok(())
}

/// 本机设备标识：首次启动生成后持久化，用于访客日志归属与后续局域网互传。
pub fn ensure_device_id(conn: &Connection) -> String {
    if let Some(id) = get_setting(conn, "device_id") {
        if !id.trim().is_empty() {
            return id;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = set_setting(conn, "device_id", &id);
    id
}

/// 为历史数据库补充 connections.owner_id 列（NULL 表示访客创建）。
///
/// 历史数据没有归属信息，升级时若存在上次登录用户（settings.active_user_id），
/// 则把这些历史连接归给该用户；否则保留 NULL，继续归属访客。
fn migrate_connection_owner(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(connections)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    if columns.iter().any(|c| c == "owner_id") {
        return Ok(());
    }

    conn.execute("ALTER TABLE connections ADD COLUMN owner_id INTEGER", [])?;

    let active_owner = get_setting(conn, "active_user_id")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|id| {
            conn.query_row("SELECT 1 FROM users WHERE id = ?1", params![id], |_| Ok(true))
                .optional()
                .ok()
                .flatten()
                .is_some()
        });
    if let Some(owner) = active_owner {
        conn.execute(
            "UPDATE connections SET owner_id = ?1 WHERE owner_id IS NULL",
            params![owner],
        )?;
    }
    Ok(())
}

pub fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn get_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    )
    .ok()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

pub fn del_setting(conn: &Connection, key: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", rusqlite::params![key])?;
    Ok(())
}
