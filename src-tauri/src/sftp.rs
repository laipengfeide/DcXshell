//! SFTP 文件传输：列目录、上传、下载。
//!
//! 复用当前 SSH 会话的连接句柄，按需打开 sftp 子系统通道，按 client_key 缓存。

use std::{
    path::Path as FsPath,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant, UNIX_EPOCH},
};

use russh::ChannelMsg;
use russh_sftp::client::SftpSession;
use serde::Serialize;
use tauri::{Emitter, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{commands::AppState, ssh_client::SshSession};

const CHUNK: usize = 64 * 1024;
const COMMON_DIRS: [&str; 6] = ["/home", "/tmp", "/var/log", "/opt", "/data", "/usr/local"];

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    pub modified: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub path: String,
    pub entries: Vec<RemoteEntry>,
    pub common_dirs: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferResult {
    pub name: String,
    pub path: String,
    pub size: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferEvent {
    pub client_key: String,
    pub direction: String,
    pub name: String,
    pub transferred: u64,
    pub total: u64,
    pub done: bool,
}

/// 取得（或首次创建）该会话对应的 SFTP 通道。
pub async fn ensure_sftp(
    state: &AppState,
    client_key: &str,
) -> Result<Arc<SftpSession>, String> {
    if let Some(cached) = state
        .sftps
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(client_key)
        .cloned()
    {
        return Ok(cached);
    }

    let session = state
        .session(client_key)
        .ok_or_else(|| "会话不存在或已关闭".to_string())?;
    if !session.alive.load(Ordering::SeqCst) {
        return Err("连接已断开，无法使用文件传输".into());
    }

    let channel = {
        let handle = session.handle.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|e| format!("打开 SFTP 通道失败：{e}"))?
    };

    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|_| "服务器未启用 SFTP 子系统，无法传输文件".to_string())?;

    let sftp = SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| {
            let text = e.to_string();
            if text.contains("sender dropped") || text.contains("subsystem") {
                "服务器未启用 SFTP 子系统（sftp-server 不可用），无法传输文件".to_string()
            } else {
                format!("SFTP 握手失败：{text}")
            }
        })?;
    sftp.set_timeout(30);
    let sftp = Arc::new(sftp);

    state
        .sftps
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(client_key.to_string(), sftp.clone());
    Ok(sftp)
}

pub(crate) fn clear_sftp(state: &AppState, client_key: &str) {
    let removed = state
        .sftps
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(client_key);
    if let Some(sftp) = removed {
        tokio::spawn(async move {
            let _ = sftp.close().await;
        });
    }
}

fn join_remote(dir: &str, name: &str) -> String {
    let base = dir.trim_end_matches('/');
    if base.is_empty() {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    }
}

fn friendly_err(context: &str, err: impl std::fmt::Display) -> String {
    let text = err.to_string();
    let lower = text.to_lowercase();
    let hint = if lower.contains("permission denied") {
        "权限不足，请确认该账号对目录有写入权限"
    } else if lower.contains("no such file") {
        "文件或目录不存在"
    } else if lower.contains("failure") || lower.contains("connection") {
        "SFTP 通道异常，请重新连接后重试"
    } else {
        ""
    };
    if hint.is_empty() {
        format!("{context}：{text}")
    } else {
        format!("{context}：{hint}（{text}）")
    }
}

async fn list_dir(sftp: &SftpSession, target: &str) -> Result<DirListing, String> {
    let canonical = match sftp.canonicalize(target).await {
        Ok(p) => p,
        Err(_) => target.to_string(),
    };

    let mut entries: Vec<RemoteEntry> = Vec::new();
    let dirs = sftp
        .read_dir(&canonical)
        .await
        .map_err(|e| friendly_err("读取目录失败", e))?;

    for entry in dirs {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let meta = entry.metadata();
        let ft = meta.file_type();
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64);
        entries.push(RemoteEntry {
            path: join_remote(&canonical, &name),
            name,
            is_dir: ft.is_dir(),
            is_link: ft.is_symlink(),
            size: meta.len(),
            modified,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let mut common: Vec<String> = COMMON_DIRS.iter().map(|s| s.to_string()).collect();
    if let Ok(home) = sftp.canonicalize(".").await {
        if !home.is_empty() && !common.contains(&home) {
            common.insert(0, home);
        }
    }

    Ok(DirListing {
        path: canonical,
        entries,
        common_dirs: common,
    })
}

#[tauri::command]
pub async fn sftp_list(
    state: State<'_, AppState>,
    client_key: String,
    path: Option<String>,
) -> Result<DirListing, String> {
    let sftp = ensure_sftp(&state, &client_key).await?;
    let target = path
        .filter(|p| !p.trim().is_empty())
        .unwrap_or_else(|| "/home".to_string());
    match list_dir(&sftp, target.trim()).await {
        Ok(list) => Ok(list),
        Err(err) => {
            // 通道可能已失效，清掉缓存下次重建
            if err.contains("SFTP 通道异常") {
                clear_sftp(&state, &client_key);
            }
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn sftp_upload(
    state: State<'_, AppState>,
    client_key: String,
    local_paths: Vec<String>,
    remote_dir: String,
) -> Result<Vec<TransferResult>, String> {
    if local_paths.is_empty() {
        return Err("请选择要上传的文件".into());
    }
    let sftp = ensure_sftp(&state, &client_key).await?;
    let dir = if remote_dir.trim().is_empty() {
        "/home".to_string()
    } else {
        remote_dir.trim().to_string()
    };

    let mut results = Vec::new();
    for local in &local_paths {
        let file_name = FsPath::new(local)
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| format!("无法识别文件名：{local}"))?
            .to_string();
        let remote_path = join_remote(&dir, &file_name);

        let mut src = tokio::fs::File::open(local)
            .await
            .map_err(|e| format!("读取本地文件失败：{e}"))?;
        let total = src.metadata().await.map(|m| m.len()).unwrap_or(0);
        let mut dst = sftp
            .create(&remote_path)
            .await
            .map_err(|e| friendly_err("创建远端文件失败", e))?;

        let mut buf = vec![0u8; CHUNK];
        let mut transferred: u64 = 0;
        let mut last_emit = Instant::now();
        loop {
            let read = src
                .read(&mut buf)
                .await
                .map_err(|e| format!("读取本地文件失败：{e}"))?;
            if read == 0 {
                break;
            }
            dst.write_all(&buf[..read])
                .await
                .map_err(|e| friendly_err("写入远端文件失败", e))?;
            transferred += read as u64;
            if last_emit.elapsed() >= Duration::from_millis(120) {
                last_emit = Instant::now();
                emit_progress(&state, &client_key, "upload", &file_name, transferred, total, false);
            }
        }
        dst.flush().await.map_err(|e| friendly_err("写入远端文件失败", e))?;
        drop(dst);
        emit_progress(&state, &client_key, "upload", &file_name, transferred, total, true);

        results.push(TransferResult {
            name: file_name,
            path: remote_path,
            size: transferred,
        });
    }
    Ok(results)
}

#[tauri::command]
pub async fn sftp_download(
    state: State<'_, AppState>,
    client_key: String,
    remote_path: String,
    local_path: String,
) -> Result<TransferResult, String> {
    let sftp = ensure_sftp(&state, &client_key).await?;
    let file_name = FsPath::new(&remote_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download")
        .to_string();

    let mut src = sftp
        .open(remote_path.clone())
        .await
        .map_err(|e| friendly_err("打开远端文件失败", e))?;
    let total = sftp
        .metadata(&remote_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    if let Some(dir) = FsPath::new(&local_path).parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    let mut dst = tokio::fs::File::create(&local_path)
        .await
        .map_err(|e| format!("创建本地文件失败：{e}"))?;

    let mut buf = vec![0u8; CHUNK];
    let mut transferred: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let read = src
            .read(&mut buf)
            .await
            .map_err(|e| friendly_err("读取远端文件失败", e))?;
        if read == 0 {
            break;
        }
        dst.write_all(&buf[..read])
            .await
            .map_err(|e| format!("写入本地文件失败：{e}"))?;
        transferred += read as u64;
        if last_emit.elapsed() >= Duration::from_millis(120) {
            last_emit = Instant::now();
            emit_progress(&state, &client_key, "download", &file_name, transferred, total, false);
        }
    }
    dst.flush().await.map_err(|e| format!("写入本地文件失败：{e}"))?;
    drop(dst);
    emit_progress(&state, &client_key, "download", &file_name, transferred, total, true);

    Ok(TransferResult {
        name: file_name,
        path: local_path,
        size: transferred,
    })
}

#[tauri::command]
pub async fn sftp_remove(
    state: State<'_, AppState>,
    client_key: String,
    remote_path: String,
    is_dir: bool,
) -> Result<(), String> {
    let sftp = ensure_sftp(&state, &client_key).await?;
    let result = if is_dir {
        sftp.remove_dir(remote_path.clone()).await
    } else {
        sftp.remove_file(remote_path.clone()).await
    };
    result.map_err(|e| friendly_err("删除失败", e))
}

#[tauri::command]
pub async fn sftp_rename(
    state: State<'_, AppState>,
    client_key: String,
    remote_path: String,
    new_name: String,
) -> Result<TransferResult, String> {
    let sftp = ensure_sftp(&state, &client_key).await?;
    let name = new_name.trim().trim_matches('/').to_string();
    if name.is_empty() || name.contains('/') {
        return Err("名称不能为空，且不能包含 /".into());
    }
    let parent = remote_path
        .rsplit_once('/')
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();
    let new_path = join_remote(&parent, &name);
    if new_path == remote_path {
        return Err("新名称与原名称相同".into());
    }
    sftp
        .rename(remote_path, new_path.clone())
        .await
        .map_err(|e| friendly_err("重命名失败", e))?;
    Ok(TransferResult {
        name,
        path: new_path,
        size: 0,
    })
}

/// 目标目录下是否已存在同名项。
async fn remote_exists(sftp: &SftpSession, path: &str) -> bool {
    sftp.metadata(path).await.is_ok()
}

/// 生成不冲突的名称：name.txt → name(2).txt → name(3).txt
async fn next_free_name(sftp: &SftpSession, dir: &str, name: &str) -> Result<String, String> {
    let existing: Vec<String> = sftp
        .read_dir(dir)
        .await
        .map_err(|e| friendly_err("读取目标目录失败", e))?
        .map(|entry| entry.file_name())
        .collect();
    if !existing.iter().any(|n| n == name) {
        return Ok(name.to_string());
    }
    let (stem, ext) = match name.rfind('.') {
        Some(pos) if pos > 0 => (&name[..pos], &name[pos..]),
        _ => (name, ""),
    };
    for n in 2..1000 {
        let candidate = format!("{stem}({n}){ext}");
        if !existing.iter().any(|e| e == &candidate) {
            return Ok(candidate);
        }
    }
    Err("同名文件过多，无法生成新名称".into())
}

/// 在服务器内部复制文件。policy：`overwrite` 覆盖同名文件，`rename` 自动加 (2) 等序号。
#[tauri::command]
pub async fn sftp_copy(
    state: State<'_, AppState>,
    client_key: String,
    source_path: String,
    target_dir: String,
    policy: String,
) -> Result<TransferResult, String> {
    let sftp = ensure_sftp(&state, &client_key).await?;
    let file_name = source_path
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("copy")
        .to_string();

    let meta = sftp
        .metadata(source_path.clone())
        .await
        .map_err(|e| friendly_err("读取源文件失败", e))?;
    if meta.file_type().is_dir() {
        return Err("暂不支持复制目录，请逐个复制目录中的文件".into());
    }
    let total = meta.len();

    let dir = if target_dir.trim().is_empty() {
        "/home".to_string()
    } else {
        target_dir.trim().to_string()
    };

    let target_name = if policy == "rename" {
        next_free_name(&sftp, &dir, &file_name).await?
    } else {
        file_name.clone()
    };
    let dest = join_remote(&dir, &target_name);
    if dest == source_path {
        return Err("源文件与粘贴位置相同".into());
    }

    if policy != "rename" && remote_exists(&sftp, &dest).await {
        sftp.remove_file(dest.clone())
            .await
            .map_err(|e| friendly_err("覆盖原文件失败", e))?;
    }

    let mut src = sftp
        .open(source_path.clone())
        .await
        .map_err(|e| friendly_err("打开源文件失败", e))?;
    let mut dst = sftp
        .create(dest.clone())
        .await
        .map_err(|e| friendly_err("创建目标文件失败", e))?;

    let mut buf = vec![0u8; CHUNK];
    let mut transferred: u64 = 0;
    let mut last_emit = Instant::now();
    loop {
        let read = src
            .read(&mut buf)
            .await
            .map_err(|e| friendly_err("读取源文件失败", e))?;
        if read == 0 {
            break;
        }
        dst.write_all(&buf[..read])
            .await
            .map_err(|e| friendly_err("写入目标文件失败", e))?;
        transferred += read as u64;
        if last_emit.elapsed() >= Duration::from_millis(120) {
            last_emit = Instant::now();
            emit_progress(&state, &client_key, "copy", &target_name, transferred, total, false);
        }
    }
    dst.flush()
        .await
        .map_err(|e| friendly_err("写入目标文件失败", e))?;
    drop(dst);
    emit_progress(&state, &client_key, "copy", &target_name, transferred, total, true);

    Ok(TransferResult {
        name: target_name,
        path: dest,
        size: transferred,
    })
}

// ---------------------------------------------------------------------------
// 解压
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ArchiveKind {
    TarGz,
    TarBz2,
    TarXz,
    Tar,
    Zip,
    Gzip,
    Bzip2,
    Xz,
}

const ARCHIVE_SUFFIXES: [&str; 10] = [
    ".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".tbz2", ".tar", ".zip", ".gz", ".bz2", ".xz",
];

fn detect_archive(name: &str) -> Option<ArchiveKind> {
    let lower = name.to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        Some(ArchiveKind::TarGz)
    } else if lower.ends_with(".tar.bz2") || lower.ends_with(".tbz2") || lower.ends_with(".tbz") {
        Some(ArchiveKind::TarBz2)
    } else if lower.ends_with(".tar.xz") || lower.ends_with(".txz") {
        Some(ArchiveKind::TarXz)
    } else if lower.ends_with(".tar") {
        Some(ArchiveKind::Tar)
    } else if lower.ends_with(".zip") {
        Some(ArchiveKind::Zip)
    } else if lower.ends_with(".gz") {
        Some(ArchiveKind::Gzip)
    } else if lower.ends_with(".bz2") {
        Some(ArchiveKind::Bzip2)
    } else if lower.ends_with(".xz") {
        Some(ArchiveKind::Xz)
    } else {
        None
    }
}

/// 去掉压缩包后缀：app.tar.gz → app，data.txt.gz → data.txt。
fn archive_stem(name: &str) -> String {
    let lower = name.to_lowercase();
    for suffix in ARCHIVE_SUFFIXES {
        if lower.ends_with(suffix) && name.len() > suffix.len() {
            return name[..name.len() - suffix.len()].to_string();
        }
    }
    name.to_string()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn extract_command(kind: ArchiveKind, source: &str, dest_dir: &str, source_name: &str) -> String {
    let src = shell_quote(source);
    let dst = shell_quote(dest_dir);
    match kind {
        ArchiveKind::TarGz => format!("tar -xzf {src} -C {dst}"),
        ArchiveKind::TarBz2 => format!("tar -xjf {src} -C {dst}"),
        ArchiveKind::TarXz => format!("tar -xJf {src} -C {dst}"),
        ArchiveKind::Tar => format!("tar -xf {src} -C {dst}"),
        ArchiveKind::Zip => format!(
            "if command -v unzip >/dev/null 2>&1; then unzip -o -q {src} -d {dst}; \
             elif command -v python3 >/dev/null 2>&1; then python3 -m zipfile -e {src} {dst}; \
             else echo '未找到 unzip 或 python3，无法解压 zip 文件' >&2; exit 127; fi"
        ),
        ArchiveKind::Gzip => format!(
            "gzip -dc {src} > {}",
            shell_quote(&join_remote(dest_dir, &archive_stem(source_name)))
        ),
        ArchiveKind::Bzip2 => format!(
            "bzip2 -dc {src} > {}",
            shell_quote(&join_remote(dest_dir, &archive_stem(source_name)))
        ),
        ArchiveKind::Xz => format!(
            "xz -dc {src} > {}",
            shell_quote(&join_remote(dest_dir, &archive_stem(source_name)))
        ),
    }
}

/// 在服务器上执行一条命令，收集 stderr 与退出码。
async fn run_remote(session: &Arc<SshSession>, command: &str) -> Result<(), String> {
    let mut channel = {
        let handle = session.handle.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|e| format!("打开命令通道失败：{e}"))?
    };
    channel
        .exec(true, command.to_string())
        .await
        .map_err(|e| format!("发送解压命令失败：{e}"))?;

    let mut stderr = String::new();
    let mut exit: Option<u32> = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        match tokio::time::timeout_at(deadline, channel.wait()).await {
            Err(_) => {
                return Err("解压超时（超过 5 分钟），请检查压缩包大小与服务器负载".into())
            }
            Ok(None) => break,
            Ok(Some(msg)) => match msg {
                ChannelMsg::ExtendedData { data, ext } if ext == 1 => {
                    stderr.push_str(&String::from_utf8_lossy(&data));
                }
                ChannelMsg::ExitStatus { exit_status } => exit = Some(exit_status),
                _ => {}
            },
        }
    }

    match exit {
        Some(0) => Ok(()),
        Some(code) => Err(extract_error(code, &stderr)),
        None => Err("执行解压命令时连接被断开，请重新连接后重试".into()),
    }
}

fn extract_error(code: u32, stderr: &str) -> String {
    let text = stderr.trim();
    let lower = text.to_lowercase();
    if code == 127 || lower.contains("command not found") || lower.contains("未找到 unzip") {
        return "服务器缺少解压工具，请先安装 tar / unzip 后再试".into();
    }
    if lower.contains("permission denied") {
        return "解压失败：权限不足，请确认该账号对目标目录有写入权限".into();
    }
    if lower.contains("no space left") {
        return "解压失败：服务器磁盘空间不足".into();
    }
    if text.is_empty() {
        format!("解压失败（退出码 {code}）")
    } else {
        let brief: String = text.chars().take(200).collect();
        format!("解压失败：{brief}")
    }
}

/// `mode`：`here` 解压到压缩包所在目录，`folder` 解压到同名新建文件夹。
#[tauri::command]
pub async fn sftp_extract(
    state: State<'_, AppState>,
    client_key: String,
    remote_path: String,
    mode: String,
) -> Result<String, String> {
    let sftp = ensure_sftp(&state, &client_key).await?;
    let session = state
        .session(&client_key)
        .ok_or_else(|| "会话不存在或已关闭".to_string())?;
    if !session.alive.load(Ordering::SeqCst) {
        return Err("连接已断开，无法解压".into());
    }

    let name = remote_path
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "无法识别压缩包名称".to_string())?
        .to_string();
    let kind = detect_archive(&name).ok_or_else(|| {
        "暂不支持该文件类型，仅支持 .tar / .tar.gz / .tgz / .tar.bz2 / .tar.xz / .zip / .gz / .bz2 / .xz"
            .to_string()
    })?;

    let parent = remote_path
        .rsplit_once('/')
        .map(|(p, _)| p.to_string())
        .unwrap_or_default();

    let meta = sftp
        .metadata(remote_path.clone())
        .await
        .map_err(|e| friendly_err("读取压缩包失败", e))?;
    if meta.file_type().is_dir() {
        return Err("目录不能解压".into());
    }

    let dest_dir = if mode == "folder" {
        let folder = archive_stem(&name);
        let dest = join_remote(&parent, &folder);
        if sftp.metadata(dest.clone()).await.is_ok() {
            return Err(format!(
                "同名文件夹「{folder}」已存在，请先删除或重命名后再解压"
            ));
        }
        sftp.create_dir(dest.clone())
            .await
            .map_err(|e| friendly_err("创建解压目录失败", e))?;
        dest
    } else if parent.is_empty() {
        "/".to_string()
    } else {
        parent
    };

    let command = extract_command(kind, &remote_path, &dest_dir, &name);
    if let Err(err) = run_remote(&session, &command).await {
        if mode == "folder" {
            // 解压失败时清理刚创建的空目录，避免留下空壳
            let _ = sftp.remove_dir(dest_dir.clone()).await;
        }
        return Err(err);
    }
    Ok(dest_dir)
}

fn emit_progress(
    state: &AppState,
    client_key: &str,
    direction: &str,
    name: &str,
    transferred: u64,
    total: u64,
    done: bool,
) {
    let _ = state.app.emit(
        "dcx:transfer",
        TransferEvent {
            client_key: client_key.to_string(),
            direction: direction.to_string(),
            name: name.to_string(),
            transferred,
            total,
            done,
        },
    );
}
