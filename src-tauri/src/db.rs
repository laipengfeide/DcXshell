use rusqlite::{params, Connection, OptionalExtension};
use std::{fs, path::Path};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL
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
"#;

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA)?;
    migrate_connection_owner(&conn)?;
    Ok(conn)
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
