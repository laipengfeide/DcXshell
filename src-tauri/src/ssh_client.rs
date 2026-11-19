use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use russh::{
    client::{self, Msg},
    keys::{load_secret_key, PrivateKeyWithHashAlg, PublicKeyOrCertificate},
    ChannelMsg, ChannelReadHalf, ChannelWriteHalf, Disconnect,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::models::TestResult;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputEvent {
    pub client_key: String,
    pub session_id: String,
    pub data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedEvent {
    pub client_key: String,
    pub session_id: String,
    pub reason: String,
}

pub struct ClientHandler;

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

#[derive(Clone)]
pub struct ConnectParams {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub passphrase: Option<String>,
}

pub struct Established {
    pub handle: client::Handle<ClientHandler>,
    pub read: ChannelReadHalf,
    pub write: Arc<ChannelWriteHalf<Msg>>,
}

pub struct SshSession {
    pub id: String,
    pub client_key: String,
    pub conn_id: i64,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub connected_at: i64,
    pub alive: Arc<AtomicBool>,
    pub bytes_in: Arc<AtomicU64>,
    pub write: Arc<ChannelWriteHalf<Msg>>,
    pub handle: tokio::sync::Mutex<client::Handle<ClientHandler>>,
}

impl SshSession {
    pub async fn ping(&self) -> (bool, u64) {
        if !self.alive.load(Ordering::SeqCst) {
            return (false, 0);
        }
        let started = Instant::now();
        let handle = self.handle.lock().await;
        let result = tokio::time::timeout(Duration::from_secs(5), handle.send_ping()).await;
        drop(handle);
        match result {
            Ok(Ok(())) => (true, started.elapsed().as_millis() as u64),
            _ => {
                self.alive.store(false, Ordering::SeqCst);
                (false, 0)
            }
        }
    }

    pub async fn shutdown(&self) {
        self.alive.store(false, Ordering::SeqCst);
        let _ = self.write.close().await;
        let mut handle = self.handle.lock().await;
        let _ = handle
            .disconnect(Disconnect::ByApplication, "", "zh-CN")
            .await;
    }
}

fn friendly_error(err: &russh::Error) -> String {
    let text = err.to_string();
    let lower = text.to_lowercase();
    if lower.contains("refused") {
        return "err.conn_refused".into();
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return "err.conn_timeout".into();
    }
    if lower.contains("dns") || lower.contains("name or service not known") {
        return "err.dns_failed".into();
    }
    if lower.contains("reset") {
        return "err.conn_reset".into();
    }
    format!("err.ssh_connect|{text}")
}

pub async fn establish(params: &ConnectParams, cols: u32, rows: u32) -> Result<Established, String> {
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(120)),
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });

    let mut handle = tokio::time::timeout(
        Duration::from_secs(15),
        client::connect(
            config,
            (params.host.as_str(), params.port),
            ClientHandler,
        ),
    )
    .await
    .map_err(|_| "err.handshake_timeout|15".to_string())?
    .map_err(|e| friendly_error(&e))?;

    let auth = match params.auth_type.as_str() {
        "key" => {
            let path = params.key_path.as_deref().unwrap_or_default();
            if path.is_empty() {
                return Err("err.key_required".into());
            }
            let passphrase = params.passphrase.as_deref().filter(|s| !s.is_empty());
            let key = load_secret_key(path, passphrase)
                .map_err(|e| format!("err.read_key|{e}"))?;
            let hash_alg = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|e| friendly_error(&e))?
                .flatten();
            handle
                .authenticate_publickey(
                    params.username.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
                )
                .await
                .map_err(|e| friendly_error(&e))?
        }
        _ => handle
            .authenticate_password(
                params.username.clone(),
                params.password.clone().unwrap_or_default(),
            )
            .await
            .map_err(|e| friendly_error(&e))?,
    };

    if !auth.success() {
        return Err("err.auth_failed".into());
    }

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| format!("err.channel_open|{e}"))?;
    channel
        .request_pty(
            false,
            "xterm-256color",
            cols.max(1),
            rows.max(1),
            0,
            0,
            &[],
        )
        .await
        .map_err(|e| format!("err.pty_request|{e}"))?;
    channel
        .request_shell(false)
        .await
        .map_err(|e| format!("err.shell_request|{e}"))?;

    let (read, write) = channel.split();
    Ok(Established {
        handle,
        read,
        write: Arc::new(write),
    })
}

pub async fn test(params: &ConnectParams) -> TestResult {
    let started = Instant::now();
    let latency = |t: Instant| -> u64 { t.elapsed().as_millis() as u64 };
    let result = tokio::time::timeout(Duration::from_secs(15), establish(params, 80, 24)).await;
    match result {
        Err(_) => TestResult {
            ok: false,
            message: "err.handshake_timeout|15".into(),
            latency_ms: latency(started),
        },
        Ok(Err(message)) => TestResult {
            ok: false,
            message,
            latency_ms: latency(started),
        },
        Ok(Ok(est)) => {
            let mut handle = est.handle;
            let _ = handle
                .disconnect(Disconnect::ByApplication, "", "zh-CN")
                .await;
            TestResult {
                ok: true,
                message: "test.success".into(),
                latency_ms: latency(started),
            }
        }
    }
}

fn flush_output(app: &AppHandle, session: &SshSession, buf: &mut Vec<u8>) {
    if buf.is_empty() {
        return;
    }
    let text = String::from_utf8_lossy(buf).to_string();
    buf.clear();
    let _ = app.emit(
        "dcx:output",
        OutputEvent {
            client_key: session.client_key.clone(),
            session_id: session.id.clone(),
            data: text,
        },
    );
}

/// 持续读取远端输出。使用 16ms 聚合刷新，保证 top / tail -f 等持续刷屏的命令
/// 既不会丢失数据，也不会把 IPC 打满。
pub fn spawn_reader(app: AppHandle, session: Arc<SshSession>, mut read: ChannelReadHalf) {
    tokio::spawn(async move {
        let mut buf: Vec<u8> = Vec::with_capacity(16 * 1024);
        let mut ticker = tokio::time::interval(Duration::from_millis(16));
        let mut reason = "err.connection_closed".to_string();

        loop {
            tokio::select! {
                msg = read.wait() => {
                    match msg {
                        Some(ChannelMsg::Data { data }) => {
                            session.bytes_in.fetch_add(data.len() as u64, Ordering::Relaxed);
                            buf.extend_from_slice(&data);
                        }
                        Some(ChannelMsg::ExtendedData { data, ext }) => {
                            if ext == 1 {
                                buf.extend_from_slice(&data);
                            }
                        }
                        Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                            reason = "err.remote_closed".into();
                            break;
                        }
                        Some(_) => {}
                        None => {
                            reason = "err.connection_closed".into();
                            break;
                        }
                    }
                    if buf.len() >= 4096 {
                        flush_output(&app, &session, &mut buf);
                    }
                }
                _ = ticker.tick() => {
                    if !buf.is_empty() {
                        flush_output(&app, &session, &mut buf);
                    }
                }
            }
        }

        // 只有“自然掉线”才通知前端；主动关闭 / 被新会话替换时 alive 已提前置为 false，
        // 避免重连过程中收到旧会话的 closed 事件造成误判。
        let was_alive = session.alive.swap(false, Ordering::SeqCst);
        flush_output(&app, &session, &mut buf);
        if was_alive {
            let _ = app.emit(
                "dcx:closed",
                ClosedEvent {
                    client_key: session.client_key.clone(),
                    session_id: session.id.clone(),
                    reason,
                },
            );
        }
    });
}
