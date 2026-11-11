//! 操作日志：写入与命令脱敏。

use rusqlite::{params, Connection};

/// 写入一条操作日志。
///
/// 失败静默忽略：日志只是旁路记录，绝不能影响登录、连接、终端输入等主流程。
pub fn record(
    conn: &Connection,
    user_id: Option<i64>,
    session_id: &str,
    username: &str,
    action: &str,
    target: &str,
    ip: &str,
) {
    let _ = conn.execute(
        "INSERT INTO audit_logs(user_id, session_id, username, action, target, ip, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            user_id,
            session_id,
            username,
            action,
            target,
            ip,
            crate::db::now()
        ],
    );
}

/// 密码类字面量脱敏：保留少量首尾字符，中间打码。
pub fn mask_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    match chars.len() {
        0 => String::new(),
        1..=3 => "•••".to_string(),
        4..=6 => format!("{}•••", chars[0]),
        _ => format!("{}•••{}", chars[0], chars[chars.len() - 1]),
    }
}

/// 对命令中的常见密码形态脱敏后再入库：
/// `-pXXX`、`-p XXX`、`--password=XXX`、`--password XXX`、`passwd` 后的参数。
pub fn sanitize_command(raw: &str) -> String {
    let mut tokens: Vec<String> = raw.split_whitespace().map(|s| s.to_string()).collect();
    let mut redact_next = false;
    let mut passwd_tail = false;

    for token in tokens.iter_mut() {
        if passwd_tail {
            *token = "••••".to_string();
            continue;
        }
        if redact_next {
            *token = mask_secret(token);
            redact_next = false;
            continue;
        }

        let lower = token.to_ascii_lowercase();
        if lower == "-p" || lower == "--password" || lower == "--pass" || lower == "--passwd" {
            redact_next = true;
            continue;
        }
        if let Some((flag, value)) = lower.split_once('=') {
            if flag == "--password" || flag == "--pass" || flag == "--passwd" {
                *token = format!("{flag}={}", mask_secret(value));
                continue;
            }
        }
        // 只认小写 -p 前缀（-P 通常是端口一类的普通参数）
        if token.starts_with("-p") && !token.starts_with("--") && token.chars().count() > 2 {
            let rest: String = token.chars().skip(2).collect();
            *token = format!("-p{}", mask_secret(&rest));
            continue;
        }
        if lower == "passwd" {
            passwd_tail = true;
        }
    }

    tokens.join(" ")
}
