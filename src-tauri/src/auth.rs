use crate::{db, models::UserInfo};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use rusqlite::{params, Connection, OptionalExtension};

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserInfo> {
    Ok(UserInfo {
        id: row.get(0)?,
        username: row.get(1)?,
        created_at: row.get(2)?,
    })
}

pub fn register(conn: &Connection, username: &str, password: &str) -> Result<UserInfo, String> {
    let username = username.trim();
    if !(3..=24).contains(&username.chars().count()) {
        return Err("用户名长度需为 3 - 24 个字符".into());
    }
    if password.chars().count() < 6 {
        return Err("密码至少需要 6 位".into());
    }
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM users WHERE username = ?1",
            params![username],
            |_| Ok(true),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .unwrap_or(false);
    if exists {
        return Err("该用户名已被注册".into());
    }
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("密码加密失败: {e}"))?
        .to_string();
    let created_at = db::now();
    conn.execute(
        "INSERT INTO users(username, password_hash, created_at) VALUES (?1, ?2, ?3)",
        params![username, hash, created_at],
    )
    .map_err(|e| e.to_string())?;
    let user = UserInfo {
        id: conn.last_insert_rowid(),
        username: username.to_string(),
        created_at,
    };
    db::set_setting(conn, "active_user_id", &user.id.to_string()).map_err(|e| e.to_string())?;
    Ok(user)
}

pub fn login(conn: &Connection, username: &str, password: &str) -> Result<UserInfo, String> {
    let username = username.trim();
    let (id, hash, created_at): (i64, String, String) = conn
        .query_row(
            "SELECT id, password_hash, created_at FROM users WHERE username = ?1",
            params![username],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "用户名或密码错误".to_string())?;

    let parsed = PasswordHash::new(&hash).map_err(|e| format!("密码校验失败: {e}"))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| "用户名或密码错误".to_string())?;

    db::set_setting(conn, "active_user_id", &id.to_string()).map_err(|e| e.to_string())?;
    Ok(UserInfo {
        id,
        username: username.to_string(),
        created_at,
    })
}

pub fn restore_session(conn: &Connection) -> Option<UserInfo> {
    let id = db::get_setting(conn, "active_user_id")?;
    conn.query_row(
        "SELECT id, username, created_at FROM users WHERE id = ?1",
        params![id.parse::<i64>().ok()?],
        row_to_user,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn logout(conn: &Connection) -> Result<(), String> {
    db::del_setting(conn, "active_user_id").map_err(|e| e.to_string())
}
