//! 局域网互传：UDP 广播发现 + TCP 流式传输。
//!
//! 发现：每 3 秒向 255.255.255.255:45454 广播一条公告（设备标识、显示名、IP、传输端口），
//! 同时监听该端口收集同网段其他实例的公告，10 秒没有更新即视为离线。
//! 传输：对方通过 TCP 45455 直连，接收方确认后发送方按 64KB 分块流式写出，
//! 长度全程使用 u64，不把整个文件读进内存；末尾附 sha256 供接收方校验。

use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, UdpSocket},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

/// 传输监听端口。
pub const TRANSFER_PORT: u16 = 45455;

const DISCOVERY_PORT: u16 = 45454;
const MAGIC: &[u8; 4] = b"DCX1";
const VERSION: u8 = 1;
const CHUNK_SIZE: usize = 64 * 1024;
/// 单文件上限 10GiB（含），超过直接拒绝。
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024 * 1024;
const MAX_FILE_TEXT: &str = "10GB";
const ANNOUNCE_EVERY: Duration = Duration::from_secs(3);
const PEER_TIMEOUT: Duration = Duration::from_secs(10);
const DECISION_TIMEOUT: Duration = Duration::from_secs(120);
/// 单次读写连续无进展超过该时长即判定连接中断，避免半开连接把任务永久挂起。
/// 注意这是"单块无进展"超时，不是整次传输的总时长限制，大文件可以传任意久。
const STALL_TIMEOUT: Duration = Duration::from_secs(120);
/// 期望接收方在传完后回 ACK 的最长等待时间（含其落盘 flush）。
const ACK_TIMEOUT: Duration = Duration::from_secs(180);
/// 磁盘空间预检时额外预留的余量，避免刚好写满产生半截文件。
const DISK_RESERVE: u64 = 64 * 1024 * 1024;
/// 进度事件最小间隔，避免大文件刷爆前端。
const PROGRESS_EVERY: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Announce {
    v: u8,
    device_id: String,
    name: String,
    ip: String,
    port: u16,
}

/// 前端展示用的在线用户。
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanStatus {
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub listening: bool,
    pub save_dir: String,
    /// 设置项自定义目录不可用、已回退系统下载目录
    pub save_dir_fallback: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IncomingEvent {
    id: String,
    from: String,
    ip: String,
    file_name: String,
    size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferEvent {
    id: String,
    /// send / receive
    direction: &'static str,
    peer: String,
    file_name: String,
    total: u64,
    transferred: u64,
    /// waiting / active / done / failed / declined
    status: &'static str,
    message: String,
    /// 完成后接收方的落盘绝对路径（仅 receive + done 时有值）
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
}

/// 接收方对一次传入请求的决定。
pub struct Decision {
    pub accept: bool,
    pub save_dir: Option<PathBuf>,
}

struct Identity {
    name: String,
    ip: String,
}

impl Identity {
    fn active(&self) -> bool {
        !self.name.trim().is_empty()
    }
}

struct Inner {
    peers: HashMap<String, (PeerView, Instant)>,
    pending: HashMap<String, oneshot::Sender<Decision>>,
}

pub struct LanState {
    app: AppHandle,
    device_id: String,
    identity: Mutex<Identity>,
    inner: Mutex<Inner>,
    listening: AtomicBool,
}

impl LanState {
    /// 启动发现与传输监听，返回可被 Tauri 托管的句柄。
    pub fn start(app: AppHandle, device_id: String, ip: String, name: String) -> Arc<Self> {
        let state = Arc::new(Self {
            app,
            device_id,
            identity: Mutex::new(Identity { name, ip }),
            inner: Mutex::new(Inner {
                peers: HashMap::new(),
                pending: HashMap::new(),
            }),
            listening: AtomicBool::new(false),
        });

        let discover = state.clone();
        std::thread::Builder::new()
            .name("dcx-lan-discovery".into())
            .spawn(move || discover.run_discovery())
            .ok();

        let listener = state.clone();
        tauri::async_runtime::spawn(async move { listener.run_transfer_listener().await });

        state
    }

    /// 切换当前身份（登录名/访客名）。空名称表示不对外公告。
    pub fn set_identity(&self, name: &str, ip: &str) {
        let mut guard = self.identity.lock().unwrap_or_else(|e| e.into_inner());
        guard.name = name.to_string();
        guard.ip = ip.to_string();
    }

    pub fn status(&self, save_dir: PathBuf, save_dir_fallback: bool) -> LanStatus {
        let (name, ip) = {
            let guard = self.identity.lock().unwrap_or_else(|e| e.into_inner());
            (guard.name.clone(), guard.ip.clone())
        };
        LanStatus {
            name,
            ip,
            port: TRANSFER_PORT,
            listening: self.listening.load(Ordering::SeqCst),
            save_dir: save_dir.to_string_lossy().to_string(),
            save_dir_fallback,
        }
    }

    pub fn peers(&self) -> Vec<PeerView> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        prune(&mut inner);
        let mut list: Vec<PeerView> = inner.peers.values().map(|(p, _)| p.clone()).collect();
        list.sort_by(|a, b| a.name.cmp(&b.name).then(a.ip.cmp(&b.ip)));
        list
    }

    /// 接收方回应对传入文件的决定。
    pub fn resolve(&self, id: &str, accept: bool, save_dir: Option<PathBuf>) -> Result<(), String> {
        let sender = {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.pending.remove(id)
        };
        sender
            .ok_or_else(|| "err.transfer_request_expired".to_string())?
            .send(Decision { accept, save_dir })
            .map_err(|_| "err.transfer_request_expired".to_string())
    }

    /// 发起一次发送，立即返回传输 id，进度通过事件推送。
    pub fn spawn_send(self: &Arc<Self>, path: String, peer_id: String) -> Result<String, String> {
        let peer = {
            let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner
                .peers
                .get(&peer_id)
                .map(|(p, _)| p.clone())
                .ok_or_else(|| "err.peer_offline".to_string())?
        };

        let meta = std::fs::metadata(&path).map_err(|e| format!("err.read_file|{e}"))?;
        if !meta.is_file() {
            return Err("err.single_file_only".into());
        }
        if meta.len() > MAX_FILE_SIZE {
            return Err(format!("err.file_too_large|{MAX_FILE_TEXT}"));
        }

        let file_name = Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".into());
        let id = uuid::Uuid::new_v4().to_string();

        self.emit(TransferEvent {
            id: id.clone(),
            direction: "send",
            peer: peer.name.clone(),
            file_name: file_name.clone(),
            total: meta.len(),
            transferred: 0,
            status: "waiting",
            message: "tr.waiting".into(),
            path: None,
        });

        let state = self.clone();
        let send_id = id.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = state.send_file(&send_id, &path, &peer, &file_name).await {
                state.emit(TransferEvent {
                    id: send_id,
                    direction: "send",
                    peer: peer.name,
                    file_name,
                    total: meta.len(),
                    transferred: 0,
                    status: "failed",
                    message: err,
                    path: None,
                });
            }
        });

        Ok(id)
    }

    fn default_save_dir(&self) -> PathBuf {
        self.app
            .path()
            .download_dir()
            .or_else(|_| self.app.path().home_dir())
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    fn emit(&self, event: TransferEvent) {
        let _ = self.app.emit("dcx:transfer", event);
    }

    // -----------------------------------------------------------------
    // 发现
    // -----------------------------------------------------------------

    fn run_discovery(self: Arc<Self>) {
        let socket = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) {
            Ok(s) => s,
            Err(err) => {
                eprintln!("[lan] failed to bind discovery port: {err}");
                return;
            }
        };
        let _ = socket.set_broadcast(true);
        let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));

        let mut last_announce: Option<Instant> = None;
        let mut last_peers: Vec<PeerView> = Vec::new();
        let mut buf = [0u8; 1024];

        loop {
            let due = last_announce
                .map(|t| t.elapsed() >= ANNOUNCE_EVERY)
                .unwrap_or(true);
            if due {
                self.announce(&socket);
                last_announce = Some(Instant::now());
            }

            if let Ok((len, src)) = socket.recv_from(&mut buf) {
                if let Ok(msg) = serde_json::from_slice::<Announce>(&buf[..len]) {
                    self.observe(msg, src);
                }
            }

            let snapshot = self.peers();
            if snapshot != last_peers {
                last_peers = snapshot.clone();
                let _ = self.app.emit("dcx:peers", snapshot);
            }
        }
    }

    fn announce(&self, socket: &UdpSocket) {
        let (name, ip) = {
            let guard = self.identity.lock().unwrap_or_else(|e| e.into_inner());
            if !guard.active() {
                return;
            }
            (guard.name.clone(), guard.ip.clone())
        };
        let msg = Announce {
            v: VERSION,
            device_id: self.device_id.clone(),
            name,
            ip,
            port: TRANSFER_PORT,
        };
        if let Ok(bytes) = serde_json::to_vec(&msg) {
            let _ = socket.send_to(&bytes, (Ipv4Addr::BROADCAST, DISCOVERY_PORT));
        }
    }

    fn observe(&self, msg: Announce, src: SocketAddr) {
        if msg.v != VERSION || msg.device_id == self.device_id || msg.name.trim().is_empty() {
            return;
        }
        let ip = if msg.ip.trim().is_empty() {
            src.ip().to_string()
        } else {
            msg.ip
        };
        let peer = PeerView {
            id: msg.device_id,
            name: msg.name,
            ip,
            port: if msg.port == 0 { TRANSFER_PORT } else { msg.port },
        };
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.peers.insert(peer.id.clone(), (peer, Instant::now()));
    }

    // -----------------------------------------------------------------
    // 接收
    // -----------------------------------------------------------------

    async fn run_transfer_listener(self: Arc<Self>) {
        let listener = match TcpListener::bind(("0.0.0.0", TRANSFER_PORT)).await {
            Ok(l) => l,
            Err(err) => {
                eprintln!("[lan] failed to listen on transfer port: {err}");
                return;
            }
        };
        self.listening.store(true, Ordering::SeqCst);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let state = self.clone();
                    tokio::spawn(async move {
                        if let Err(err) = state.receive_file(stream, addr).await {
                            eprintln!("[lan] receive failed: {err}");
                        }
                    });
                }
                Err(err) => {
                    eprintln!("[lan] failed to accept connection: {err}");
                    tokio::time::sleep(Duration::from_millis(300)).await;
                }
            }
        }
    }

    async fn receive_file(&self, mut stream: TcpStream, addr: SocketAddr) -> Result<(), String> {
        let mut magic = [0u8; 4];
        stream.read_exact(&mut magic).await.map_err(msg)?;
        if &magic != MAGIC {
            return Err("err.protocol_mismatch".into());
        }
        let version = stream.read_u8().await.map_err(msg)?;
        if version != VERSION {
            return Err(format!("err.protocol_version|{version}"));
        }
        let from = read_string(&mut stream).await?;
        let raw_name = read_string(&mut stream).await?;
        let total = stream.read_u64().await.map_err(msg)?;
        let file_name = safe_name(&raw_name);
        let peer_name = if from.trim().is_empty() {
            addr.ip().to_string()
        } else {
            from
        };

        if total > MAX_FILE_SIZE {
            let _ = stream.write_all(&[0]).await;
            self.emit(TransferEvent {
                id: uuid::Uuid::new_v4().to_string(),
                direction: "receive",
                peer: peer_name,
                file_name,
                total,
                transferred: 0,
                status: "failed",
                message: format!("err.file_too_large|{MAX_FILE_TEXT}"),
                path: None,
            });
            return Ok(());
        }

        let id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            inner.pending.insert(id.clone(), tx);
        }
        let _ = self.app.emit(
            "dcx:incoming",
            IncomingEvent {
                id: id.clone(),
                from: peer_name.clone(),
                ip: addr.ip().to_string(),
                file_name: file_name.clone(),
                size: total,
            },
        );

        let decision = match tokio::time::timeout(DECISION_TIMEOUT, rx).await {
            Ok(Ok(d)) => d,
            _ => {
                self.take_pending(&id);
                let _ = stream.write_all(&[0]).await;
                self.emit(TransferEvent {
                    id,
                    direction: "receive",
                    peer: peer_name,
                    file_name,
                    total,
                    transferred: 0,
                    status: "declined",
                    message: "tr.decision_timeout".into(),
                    path: None,
                });
                return Ok(());
            }
        };
        self.take_pending(&id);

        if !decision.accept {
            let _ = stream.write_all(&[0]).await;
            self.emit(TransferEvent {
                id,
                direction: "receive",
                peer: peer_name,
                file_name,
                total,
                transferred: 0,
                status: "declined",
                message: "tr.rejected".into(),
                path: None,
            });
            return Ok(());
        }

        stream.write_all(&[1]).await.map_err(msg)?;

        let dir = decision.save_dir.unwrap_or_else(|| self.default_save_dir());
        let result = self
            .store(&mut stream, &id, &peer_name, &file_name, total, &dir)
            .await;

        let ack: u8 = if result.is_ok() { 1 } else { 0 };
        let _ = stream.write_all(&[ack]).await;
        let _ = stream.flush().await;

        match result {
            Ok(path) => {
                self.emit(TransferEvent {
                    id,
                    direction: "receive",
                    peer: peer_name,
                    file_name,
                    total,
                    transferred: total,
                    status: "done",
                    message: "tr.received".into(),
                    path: Some(path.to_string_lossy().to_string()),
                });
                Ok(())
            }
            Err(err) => {
                self.emit(TransferEvent {
                    id,
                    direction: "receive",
                    peer: peer_name,
                    file_name,
                    total,
                    transferred: 0,
                    status: "failed",
                    message: err.clone(),
                    path: None,
                });
                Err(err)
            }
        }
    }

    fn take_pending(&self, id: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.pending.remove(id);
    }

    /// 流式落盘并校验摘要，返回最终保存路径。
    ///
    /// 任何失败（连接中断、磁盘写满、校验不符）都会删除已经写出的半截文件，
    /// 避免在接收目录里留下无法识别的残留。
    async fn store(
        &self,
        stream: &mut TcpStream,
        id: &str,
        peer: &str,
        file_name: &str,
        total: u64,
        dir: &Path,
    ) -> Result<PathBuf, String> {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| format!("err.create_save_dir|{e}"))?;

        // 先按可用空间判断能否装下，装不下直接失败，不白传几个 GB。
        if let Some(free) = available_bytes(dir) {
            if free < total.saturating_add(DISK_RESERVE) {
                return Err(format!(
                    "err.disk_insufficient|{}|{}",
                    human(free),
                    human(total)
                ));
            }
        }

        let target = unique_path(dir, file_name);
        let mut file = tokio::fs::File::create(&target)
            .await
            .map_err(|e| format!("err.create_file|{e}"))?;

        self.emit(TransferEvent {
            id: id.to_string(),
            direction: "receive",
            peer: peer.to_string(),
            file_name: file_name.to_string(),
            total,
            transferred: 0,
            status: "active",
            message: "tr.receiving".into(),
            path: None,
        });

        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut received = 0u64;
        let mut last = Instant::now();
        let mut failure: Option<String> = None;

        while received < total {
            let want = ((total - received) as usize).min(CHUNK_SIZE);
            // 单块无进展超时：网络断掉时 TCP 可能长时间不报错，这里主动收口。
            let n = match tokio::time::timeout(STALL_TIMEOUT, stream.read(&mut buf[..want])).await {
                Ok(Ok(n)) => n,
                Ok(Err(err)) => {
                    failure = Some(msg(err));
                    break;
                }
                Err(_) => {
                    failure = Some(format!(
                        "err.conn_stall_receive|{}",
                        STALL_TIMEOUT.as_secs()
                    ));
                    break;
                }
            };
            if n == 0 {
                failure = Some("err.conn_incomplete".into());
                break;
            }
            match tokio::time::timeout(STALL_TIMEOUT, file.write_all(&buf[..n])).await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    failure = Some(write_error(&err));
                    break;
                }
                Err(_) => {
                    failure = Some("err.write_timeout".into());
                    break;
                }
            }
            hasher.update(&buf[..n]);
            received += n as u64;
            if last.elapsed() >= PROGRESS_EVERY || received == total {
                last = Instant::now();
                self.emit(TransferEvent {
                    id: id.to_string(),
                    direction: "receive",
                    peer: peer.to_string(),
                    file_name: file_name.to_string(),
                    total,
                    transferred: received,
                    status: "active",
                    message: "tr.receiving".into(),
                    path: None,
                });
            }
        }

        let flushed = tokio::time::timeout(STALL_TIMEOUT, file.flush()).await;
        drop(file);

        if failure.is_none() {
            match flushed {
                Ok(Ok(())) => {}
                Ok(Err(err)) => failure = Some(write_error(&err)),
                Err(_) => failure = Some("err.write_timeout".into()),
            }
        }

        if let Some(msg) = failure {
            let _ = tokio::fs::remove_file(&target).await;
            return Err(msg);
        }

        let mut digest = [0u8; 32];
        let digest_read =
            tokio::time::timeout(STALL_TIMEOUT, stream.read_exact(&mut digest)).await;
        match digest_read {
            Ok(Ok(_)) => {}
            Ok(Err(err)) => {
                let _ = tokio::fs::remove_file(&target).await;
                return Err(msg(err));
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&target).await;
                return Err("err.conn_no_digest".into());
            }
        }
        if hasher.finalize().as_slice() != digest {
            let _ = tokio::fs::remove_file(&target).await;
            return Err("err.checksum_mismatch".into());
        }

        Ok(target)
    }

    // -----------------------------------------------------------------
    // 发送
    // -----------------------------------------------------------------

    async fn send_file(
        &self,
        id: &str,
        path: &str,
        peer: &PeerView,
        file_name: &str,
    ) -> Result<(), String> {
        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| format!("err.read_file|{e}"))?;
        let total = meta.len();
        let local_name = {
            let guard = self.identity.lock().unwrap_or_else(|e| e.into_inner());
            guard.name.clone()
        };

        let addr = format!("{}:{}", peer.ip, peer.port);
        let mut stream = tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(&addr))
            .await
            .map_err(|_| "err.connect_peer_timeout".to_string())?
            .map_err(|e| format!("err.connect_peer|{e}"))?;

        stream.write_all(MAGIC).await.map_err(msg)?;
        stream.write_all(&[VERSION]).await.map_err(msg)?;
        write_string(&mut stream, &local_name).await?;
        write_string(&mut stream, file_name).await?;
        stream.write_all(&total.to_be_bytes()).await.map_err(msg)?;
        stream.flush().await.map_err(msg)?;

        let mut decision = [0u8; 1];
        tokio::time::timeout(DECISION_TIMEOUT, stream.read_exact(&mut decision))
            .await
            .map_err(|_| "err.wait_decision_timeout".to_string())?
            .map_err(|e| format!("err.peer_disconnected|{e}"))?;

        if decision[0] != 1 {
            self.emit(TransferEvent {
                id: id.to_string(),
                direction: "send",
                peer: peer.name.clone(),
                file_name: file_name.to_string(),
                total,
                transferred: 0,
                status: "declined",
                message: "tr.declined".into(),
                path: None,
            });
            return Ok(());
        }

        self.emit(TransferEvent {
            id: id.to_string(),
            direction: "send",
            peer: peer.name.clone(),
            file_name: file_name.to_string(),
            total,
            transferred: 0,
            status: "active",
            message: "tr.sending".into(),
            path: None,
        });

        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| format!("err.read_file|{e}"))?;
        let mut buf = vec![0u8; CHUNK_SIZE];
        let mut hasher = Sha256::new();
        let mut sent = 0u64;
        let mut last = Instant::now();

        loop {
            let n = file.read(&mut buf).await.map_err(msg)?;
            if n == 0 {
                break;
            }
            // 单块无进展超时：对方断网或长时间不读时及时中止，避免一直挂着。
            match tokio::time::timeout(STALL_TIMEOUT, stream.write_all(&buf[..n])).await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => return Err(msg(err)),
                Err(_) => {
                    return Err(format!(
                        "err.conn_stall_send|{}",
                        STALL_TIMEOUT.as_secs()
                    ))
                }
            }
            hasher.update(&buf[..n]);
            sent += n as u64;
            if last.elapsed() >= PROGRESS_EVERY || sent == total {
                last = Instant::now();
                self.emit(TransferEvent {
                    id: id.to_string(),
                    direction: "send",
                    peer: peer.name.clone(),
                    file_name: file_name.to_string(),
                    total,
                    transferred: sent,
                    status: "active",
                    message: "tr.sending".into(),
                    path: None,
                });
            }
        }

        if sent != total {
            return Err("err.read_length_mismatch".into());
        }

        stream.write_all(&hasher.finalize()).await.map_err(msg)?;
        stream.flush().await.map_err(msg)?;

        let mut ack = [0u8; 1];
        tokio::time::timeout(ACK_TIMEOUT, stream.read_exact(&mut ack))
            .await
            .map_err(|_| "err.wait_ack_timeout".to_string())?
            .map_err(|e| format!("err.peer_disconnected|{e}"))?;

        if ack[0] == 1 {
            self.emit(TransferEvent {
                id: id.to_string(),
                direction: "send",
                peer: peer.name.clone(),
                file_name: file_name.to_string(),
                total,
                transferred: total,
                status: "done",
                message: "tr.sent".into(),
                path: None,
            });
            Ok(())
        } else {
            Err("err.receiver_save_failed".into())
        }
    }
}

fn prune(inner: &mut Inner) {
    inner
        .peers
        .retain(|_, (_, seen)| seen.elapsed() < PEER_TIMEOUT);
}

fn msg(err: std::io::Error) -> String {
    format!("err.network|{err}")
}

/// 把落盘错误转成用户能看懂的话：Windows 的磁盘已满错误单独提示。
fn write_error(err: &std::io::Error) -> String {
    // 112 = ERROR_DISK_FULL，39 = ERROR_HANDLE_DISK_FULL
    if matches!(err.raw_os_error(), Some(112) | Some(39)) {
        return "err.disk_full_write".into();
    }
    format!("err.write_failed|{err}")
}

fn human(size: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = size as f64;
    let mut index = 0;
    while value >= 1024.0 && index < UNITS.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{size} B")
    } else {
        format!("{value:.1} {}", UNITS[index])
    }
}

/// 目标目录所在卷的可用字节数；拿不到返回 None（此时退化为写入时再报错）。
#[cfg(windows)]
fn available_bytes(dir: &Path) -> Option<u64> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    let wide: Vec<u16> = OsStr::new(dir)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_to_caller = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            &mut total,
            &mut total_free,
        )
    };
    if ok != 0 {
        Some(free_to_caller)
    } else {
        None
    }
}

#[cfg(not(windows))]
fn available_bytes(_dir: &Path) -> Option<u64> {
    None
}

/// 校验目录能否作为接收目录：存在、是目录、可写（写入后立即删除探针文件）。
pub fn check_save_dir(dir: &Path) -> Result<(), String> {
    if !dir.exists() {
        return Err(format!("err.dir_missing|{}", dir.display()));
    }
    let meta = std::fs::metadata(dir).map_err(|e| format!("err.dir_check|{}|{e}", dir.display()))?;
    if !meta.is_dir() {
        return Err(format!("err.dir_not_directory|{}", dir.display()));
    }
    let probe = dir.join(format!(".dcx-write-test-{}", std::process::id()));
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(err) => Err(format!("err.dir_not_writable|{}|{err}", dir.display())),
    }
}

async fn read_string(stream: &mut TcpStream) -> Result<String, String> {
    let len = stream.read_u16().await.map_err(msg)? as usize;
    if len == 0 {
        return Ok(String::new());
    }
    if len > 4096 {
        return Err("err.field_too_long".into());
    }
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await.map_err(msg)?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

async fn write_string(stream: &mut TcpStream, value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    let len = bytes.len().min(4096) as u16;
    stream.write_all(&len.to_be_bytes()).await.map_err(msg)?;
    stream.write_all(&bytes[..len as usize]).await.map_err(msg)?;
    Ok(())
}

/// 去掉路径成分，只保留文件名，避免对方写入任意路径。
fn safe_name(raw: &str) -> String {
    let name = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(raw)
        .replace([':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();
    if name.is_empty() || name == "." || name == ".." {
        "received_file".to_string()
    } else {
        name
    }
}

/// 同名文件自动追加序号，避免覆盖已有文件。
fn unique_path(dir: &Path, file_name: &str) -> PathBuf {
    let base = Path::new(file_name);
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "received_file".into());
    let ext = base.extension().map(|s| s.to_string_lossy().to_string());

    let mut candidate = dir.join(file_name);
    let mut index = 1;
    while candidate.exists() {
        let name = match ext.as_deref() {
            Some(e) => format!("{stem} ({index}).{e}"),
            None => format!("{stem} ({index})"),
        };
        candidate = dir.join(name);
        index += 1;
        if index > 9999 {
            break;
        }
    }
    candidate
}
