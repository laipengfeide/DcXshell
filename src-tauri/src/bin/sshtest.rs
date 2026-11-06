//! 本地 SSH 测试服务器：用于端到端验证 Dc Xshell 的终端、心跳与自动重连。
//!
//! 启动：cargo run --bin sshtest
//! 账号：tester / test123（任意用户名的公钥也接受），端口 2222。

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use russh::{
    keys::PrivateKey,
    server::{self, Auth, Msg, Server as _, Session, VerifiedPublicKey},
    ChannelId, Preferred,
};
use tokio::net::TcpListener;

struct TestHandler {
    channel: Option<ChannelId>,
    streaming: Arc<AtomicBool>,
    input: String,
}

impl server::Handler for TestHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == "tester" && password == "test123" {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: russh::Channel<Msg>,
        reply: russh::server::ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channel = Some(channel.id());
        reply.accept().await;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = session.data(
            channel,
            "Dc Xshell 测试服务器\r\n输入 help 查看可用命令，输入 top 体验持续刷屏。\r\n\r\ntester@demo:~$ ",
        );
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // 逐字节缓冲，收到回车后把整行当作一条命令执行
        let mut echo = String::new();
        for &byte in data {
            match byte {
                b'\r' | b'\n' => {
                    echo.push_str("\r\n");
                    let line = self.input.trim().to_string();
                    self.input.clear();
                    if !echo.is_empty() {
                        let _ = session.data(channel, echo.clone());
                        echo.clear();
                    }
                    self.run_command(&line, channel, session).await;
                }
                0x03 => {
                    self.input.clear();
                    let _ = session.data(channel, "^C\r\ntester@demo:~$ ");
                }
                0x7f | 0x08 => {
                    if self.input.pop().is_some() {
                        echo.push_str("\x08 \x08");
                    }
                }
                b if b >= 0x20 => {
                    self.input.push(b as char);
                    echo.push(b as char);
                }
                _ => {}
            }
        }
        if !echo.is_empty() {
            let _ = session.data(channel, echo);
        }
        Ok(())
    }
}

impl TestHandler {
    async fn run_command(&mut self, line: &str, channel: ChannelId, session: &mut Session) {
        match line {
            "" => {
                let _ = session.data(channel, "tester@demo:~$ ");
            }
            "q" => {
                self.streaming.store(false, Ordering::SeqCst);
                let _ = session.data(channel, "已退出 top。\r\ntester@demo:~$ ");
            }
            "help" | "?" => {
                let _ = session.data(
                    channel,
                    "可用命令：\r\n  top      持续刷新的模拟监控\r\n  ls       列出文件\r\n  clear    清屏\r\n  exit     断开连接\r\n\r\ntester@demo:~$ ",
                );
            }
            "ls" | "ls -l" => {
                let _ = session.data(
                    channel,
                    "total 8\r\ndrwxr-xr-x 2 tester tester 4096 demo\r\n-rw-r--r-- 1 tester tester  128 notes.txt\r\n\r\ntester@demo:~$ ",
                );
            }
            "clear" => {
                let _ = session.data(channel, "\x1b[2J\x1b[H");
            }
            "exit" | "logout" => {
                let _ = session.data(channel, "再见！\r\n");
                let _ = session.handle().close(channel).await;
            }
            "top" => {
                self.streaming.store(true, Ordering::SeqCst);
                let handle = session.handle();
                let stop = self.streaming.clone();
                tokio::spawn(async move {
                    let mut tick: u64 = 0;
                    while stop.load(Ordering::SeqCst) {
                        let mut out = String::from("\x1b[H\x1b[2J");
                        out.push_str("top - demo - 每 0.5 秒刷新一次，按 q 退出\r\n");
                        out.push_str(&format!("运行时长 {} tick\r\n\r\n", tick));
                        out.push_str("  PID  USER      %CPU  %MEM   COMMAND\r\n");
                        for i in 1..=12 {
                            let cpu = ((tick + i as u64) % 90) + 5;
                            out.push_str(&format!(
                                " {:>4}  tester   {:>4}  {:>4}  process-{}\r\n",
                                i,
                                cpu,
                                (i * 2) % 40 + 1,
                                i
                            ));
                        }
                        let _ = handle.data(channel, out).await;
                        tick += 1;
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                });
            }
            other => {
                let _ = session.data(
                    channel,
                    format!("bash: {}: command not found\r\ntester@demo:~$ ", other),
                );
            }
        }
    }
}

#[derive(Clone)]
struct TestServer;

impl server::Server for TestServer {
    type Handler = TestHandler;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> TestHandler {
        TestHandler {
            channel: None,
            streaming: Arc::new(AtomicBool::new(false)),
            input: String::new(),
        }
    }
}

#[tokio::main]
async fn main() {
    let config = Arc::new(server::Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        auth_rejection_time: Duration::from_secs(1),
        auth_rejection_time_initial: Some(Duration::from_secs(0)),
        keys: vec![
            PrivateKey::from_openssh(include_str!("../../assets/sshtest_hostkey")).unwrap(),
        ],
        preferred: Preferred::default(),
        ..Default::default()
    });

    let socket = TcpListener::bind(("127.0.0.1", 2222)).await.unwrap();
    println!("Dc Xshell 测试 SSH 服务已启动：127.0.0.1:2222 (tester / test123)");
    let mut server = TestServer;
    server
        .run_on_socket(config, &socket)
        .await
        .unwrap();
}
