//! 本机局域网地址探测。

use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

/// 取本机内网出口 IPv4，供登录记录与后续局域网互传定位使用。
///
/// 通过 UDP connect 让系统选出默认路由对应的网卡（不会真正发包），
/// 多网卡时因此优先取有默认网关、可实际通信的那块；失败时回退到主机名解析。
pub fn local_ipv4() -> String {
    for target in ["223.5.5.5:80", "8.8.8.8:80", "1.1.1.1:80"] {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
            if sock.connect(target).is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    if let IpAddr::V4(v4) = addr.ip() {
                        if usable(v4) {
                            return v4.to_string();
                        }
                    }
                }
            }
        }
    }

    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_default();
    if !host.is_empty() {
        if let Ok(addrs) = (host.as_str(), 0u16).to_socket_addrs() {
            for addr in addrs {
                if let IpAddr::V4(v4) = addr.ip() {
                    if usable(v4) {
                        return v4.to_string();
                    }
                }
            }
        }
    }

    "127.0.0.1".to_string()
}

/// 过滤回环、未指定与链路本地地址。
fn usable(ip: Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified() && !ip.is_link_local()
}
