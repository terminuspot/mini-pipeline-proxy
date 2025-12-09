use crate::proxy::context::ConnectionContext;
use crate::proxy::protocol::{Connection, InboundHandler};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use shadowsocks::relay::udprelay::{DatagramReceiveExt, DatagramSendExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Socks5Inbound {
    pipeline: Arc<crate::proxy::pipeline::Pipeline>,
}

impl Socks5Inbound {
    pub fn new(pipeline: Arc<crate::proxy::pipeline::Pipeline>) -> Self {
        Self { pipeline }
    }

    /// 解析 SOCKS5 UDP 头部
    /// 返回 (目标地址String, 目标端口, 数据负载在buf中的起始偏移量)
    fn parse_socks5_udp_packet(buf: &[u8]) -> Result<(String, u16, usize)> {
        if buf.len() < 4 {
            return Err(anyhow!("udp packet too short"));
        }

        let frag = buf[2];
        if frag != 0x00 {
            return Err(anyhow!("fragmented packets not supported"));
        }

        let atyp = buf[3];
        let mut idx = 4usize;

        let host = match atyp {
            0x01 => {
                // IPv4
                if buf.len() < idx + 4 {
                    return Err(anyhow!("ipv4 short"));
                }
                let ip =
                    std::net::Ipv4Addr::new(buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]);
                idx += 4;
                ip.to_string()
            }
            0x03 => {
                // Domain
                if buf.len() < idx + 1 {
                    return Err(anyhow!("domain len missing"));
                }
                let len = buf[idx] as usize;
                idx += 1;
                if buf.len() < idx + len {
                    return Err(anyhow!("domain short"));
                }
                let domain = String::from_utf8_lossy(&buf[idx..idx + len]).to_string();
                idx += len;
                domain
            }
            0x04 => {
                // IPv6
                if buf.len() < idx + 16 {
                    return Err(anyhow!("ipv6 short"));
                }
                let mut octets = [0u8; 16];
                octets.copy_from_slice(&buf[idx..idx + 16]);
                idx += 16;
                let ip = std::net::Ipv6Addr::from(octets);
                ip.to_string()
            }
            _ => return Err(anyhow!("invalid atyp")),
        };

        if buf.len() < idx + 2 {
            return Err(anyhow!("missing port"));
        }
        let port = u16::from_be_bytes([buf[idx], buf[idx + 1]]);
        idx += 2;

        Ok((host, port, idx))
    }

    /// 构建 SOCKS5 UDP 头部 (用于发回给客户端)
    fn build_socks5_udp_packet(payload: &[u8]) -> Vec<u8> {
        // 为了简化，回包通常只需指明 ATYP=IPv4, ADDR=0.0.0.0, PORT=0
        // 客户端通常只关心数据，不关心回包头部里的源地址（除非是严格的 P2P 应用）
        // 如果需要严格实现，需要从 ShadowsocksUdpConn 中透传来源地址，但之前的 Connection trait 不支持。
        let mut out = Vec::with_capacity(10 + payload.len());
        out.extend_from_slice(&[0x00, 0x00, 0x00, 0x01]); // RSV, RSV, FRAG, ATYP(IPv4)
        out.extend_from_slice(&[0, 0, 0, 0]); // IP
        out.extend_from_slice(&[0, 0]); // Port
        out.extend_from_slice(payload);
        out
    }

    /// UDP 关联的核心循环
    async fn run_udp_associate(
        &self,
        udp_socket: Arc<UdpSocket>,
        client_addr: SocketAddr,
    ) -> Result<()> {
        let mut buf = vec![0u8; 65535];

        // NAT 表：Map<"Host:Port", ShadowSocksConnection>
        // 用于复用连接，避免每次 UDP 包都重新握手
        // 注意：Box<dyn Connection> 是 Send，但我们需要 Arc<Mutex<..>> 来在任务间共享
        type NatMap = Arc<Mutex<HashMap<String, Arc<Mutex<Box<dyn Connection>>>>>>;
        let nat_table: NatMap = Arc::new(Mutex::new(HashMap::new()));

        loop {
            // 接收客户端发来的 UDP 包
            let (n, src) = udp_socket.recv_from(&mut buf).await?;

            // 安全检查：只处理握手时的那个客户端 IP
            if src != client_addr {
                continue;
            }

            // 解析 SOCKS5 头部
            let (dst_host, dst_port, payload_offset) =
                match Self::parse_socks5_udp_packet(&buf[..n]) {
                    Ok(res) => res,
                    Err(e) => {
                        eprintln!("SOCKS5 UDP parse error: {}", e);
                        continue;
                    }
                };

            let payload = buf[payload_offset..n].to_vec();
            let dest_key = format!("{}:{}", dst_host, dst_port);

            // 检查 NAT 表中是否有现成的连接
            let mut map = nat_table.lock().await;

            if let Some(conn) = map.get(&dest_key) {
                // 已存在连接，直接发送
                let mut conn_guard = conn.lock().await;
                let _ = conn_guard.send(&payload).await;
            } else {
                // 新的目标，建立新连接
                // 1. 构建 Context
                let mut ctx = ConnectionContext {
                    src_addr: client_addr,
                    dst_host: Some(dst_host.clone()),
                    dst_port: Some(dst_port),
                    original_dst: None,
                    inbound_type: "socks5-udp".to_string(), // 标记为 UDP
                    metadata: serde_json::json!({ "udp_session": true }),
                    outbound_tag: "".to_string(),
                };

                // 2. 调用 Pipeline 获取连接 (需要 Pipeline 实现 create_outbound_connection)
                match self.pipeline.create_outbound_connection(&mut ctx).await {
                    Ok(conn) => {
                        let conn = Arc::new(Mutex::new(conn));

                        // 1. 先把数据发出去
                        {
                            let mut conn_guard = conn.lock().await;
                            if let Err(e) = conn_guard.send(&payload).await {
                                eprintln!("Failed to send to outbound: {}", e);
                                continue;
                            }
                        }

                        // 2. 启动一个后台任务，负责接收从 SS 回来的数据，并发给客户端
                        let conn_clone = conn.clone();
                        let udp_socket_clone = udp_socket.clone();
                        let dest_key_clone = dest_key.clone();
                        let nat_table_clone = nat_table.clone();

                        tokio::spawn(async move {
                            let mut rx_buf = vec![0u8; 65535];
                            loop {
                                // 从 SS 接收
                                let res = {
                                    let mut guard = conn_clone.lock().await;
                                    guard.recv(&mut rx_buf).await
                                };

                                match res {
                                    Ok(n) if n > 0 => {
                                        // 封装回 SOCKS5 UDP 格式
                                        let packet = Self::build_socks5_udp_packet(&rx_buf[..n]);
                                        // 发回给客户端
                                        let _ =
                                            udp_socket_clone.send_to(&packet, client_addr).await;
                                    }
                                    _ => {
                                        // 接收错误或 EOF，关闭此会话
                                        let mut map = nat_table_clone.lock().await;
                                        map.remove(&dest_key_clone);
                                        break;
                                    }
                                }
                            }
                        });

                        // 3. 存入 NAT 表
                        map.insert(dest_key, conn);
                    }
                    Err(e) => {
                        eprintln!("Failed to connect to UDP target {}: {}", dest_key, e);
                    }
                }
            }
        }
    }

    async fn handle_udp_associate(
        &self,
        mut tcp_stream: TcpStream,
        client_addr: SocketAddr,
    ) -> Result<()> {
        // 1. 绑定本地 UDP 端口
        let udp_socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .context("bind udp failed")?;
        let local_addr = udp_socket.local_addr()?;
        let udp_socket = Arc::new(udp_socket);

        // 2. 回复客户端：绑定成功
        let mut reply = vec![0x05, 0x00, 0x00];
        match local_addr {
            SocketAddr::V4(v4) => {
                reply.push(0x01);
                reply.extend_from_slice(&v4.ip().octets());
                reply.extend_from_slice(&v4.port().to_be_bytes());
            }
            SocketAddr::V6(v6) => {
                reply.push(0x04);
                reply.extend_from_slice(&v6.ip().octets());
                reply.extend_from_slice(&v6.port().to_be_bytes());
            }
        }
        tcp_stream.write_all(&reply).await?;

        println!(
            "UDP Associate: Client {} <-> Local {}",
            client_addr, local_addr
        );

        // 3. 使用 tokio::select! 监控 TCP 断开或 UDP 逻辑出错
        // 只要 TCP 断开，UDP 任务就会被取消
        let mut tcp_buf = [0u8; 1];

        tokio::select! {
            // 监控 TCP 读取，如果返回 0 (EOF) 或 Error，则退出
            res = tcp_stream.read(&mut tcp_buf) => {
                match res {
                    Ok(0) => println!("UDP Associate: Client closed TCP, shutting down UDP."),
                    Err(e) => eprintln!("UDP Associate: TCP error: {}", e),
                    _ => {},
                }
            }
            // 运行 UDP 转发逻辑
            res = self.run_udp_associate(udp_socket, client_addr) => {
                if let Err(e) = res {
                    eprintln!("UDP Associate: Loop error: {}", e);
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl InboundHandler for Socks5Inbound {
    async fn handle_inbound(&self, mut stream: TcpStream) -> Result<()> {
        let peer_addr = stream.peer_addr()?;

        // Minimal socks5 handshake (NO AUTH only)
        let mut ver_buf = [0u8; 2];
        stream.read_exact(&mut ver_buf).await?;
        if ver_buf[0] != 0x05 {
            return Err(anyhow::anyhow!("invalid socks version"));
        }
        let nmethods = ver_buf[1] as usize;
        let mut methods = vec![0u8; nmethods];
        stream.read_exact(&mut methods).await?;

        // respond: no auth
        stream.write_all(&[0x05, 0x00]).await?;

        // request
        let mut header = [0u8; 4];
        stream.read_exact(&mut header).await?;
        let cmd = header[1];
        let atyp = header[3];

        let dst = match atyp {
            0x01 => {
                // IPv4
                let mut addr = [0u8; 4];
                stream.read_exact(&mut addr).await?;
                let mut portb = [0u8; 2];
                stream.read_exact(&mut portb).await?;
                let port = u16::from_be_bytes(portb);
                let ip = std::net::Ipv4Addr::from(addr);
                (Some(ip.to_string()), Some(port))
            }
            0x03 => {
                // domain
                let mut lenb = [0u8; 1];
                stream.read_exact(&mut lenb).await?;
                let len = lenb[0] as usize;
                let mut domain = vec![0u8; len];
                stream.read_exact(&mut domain).await?;
                let mut portb = [0u8; 2];
                stream.read_exact(&mut portb).await?;
                let port = u16::from_be_bytes(portb);
                (Some(String::from_utf8(domain).unwrap()), Some(port))
            }
            0x04 => {
                // IPv6 (not handled fully here)
                let mut addr = [0u8; 16];
                stream.read_exact(&mut addr).await?;
                let mut portb = [0u8; 2];
                stream.read_exact(&mut portb).await?;
                let port = u16::from_be_bytes(portb);
                let ip = std::net::Ipv6Addr::from(addr);
                (Some(ip.to_string()), Some(port))
            }
            _ => (None, None),
        };

        match cmd {
            0x01 => {
                // CONNECT
                stream
                    .write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await?;
                let ctx = ConnectionContext {
                    src_addr: stream.peer_addr()?,
                    dst_host: dst.0,
                    dst_port: dst.1,
                    original_dst: None,
                    inbound_type: "socks5".to_string(),
                    metadata: serde_json::json!({}),
                    outbound_tag: "".to_string(),
                };
                self.pipeline.process(ctx, stream).await
            }
            0x03 => {
                // UDP ASSOCIATE
                self.handle_udp_associate(stream, peer_addr).await
            }
            _ => {
                stream
                    .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                    .await?;
                Err(anyhow!("unsupported command"))
            }
        }
    }
}
