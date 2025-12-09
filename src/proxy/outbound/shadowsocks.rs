use crate::proxy::context::ConnectionContext;
use crate::proxy::protocol::Connection;
// 引入 config 模块中的定义
use crate::config::ShadowsocksConfig;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use shadowsocks::config::ServerType;
use shadowsocks::context::{Context as SsContext, SharedContext};
use shadowsocks::crypto::CipherKind;
use shadowsocks::net::UdpSocket;
use shadowsocks::relay::socks5::Address as SsAddress;
use shadowsocks::relay::tcprelay::ProxyClientStream;
use shadowsocks::relay::Address;
use shadowsocks::{ProxySocket, ServerAddr, ServerConfig};
use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

// #[derive(Clone)]
// pub struct ShadowsocksConfig {
//     pub server_addr: String,
//     pub server_port: u16,
//     pub password: String,
//     pub method: String, // "chacha20-ietf-poly1305" etc
// }

pub struct ShadowsocksOutbound {
    cfg: Arc<ShadowsocksConfig>,
    ctx: SharedContext,
}

impl ShadowsocksOutbound {

    pub fn new(cfg: ShadowsocksConfig) -> Result<Self> {
        let context = SsContext::new(ServerType::Local);

        Ok(Self {
            cfg: Arc::new(cfg),
            ctx: Arc::new(context),
        })
    }

    /// 构建 Shadowsocks 的 ServerConfig
    fn build_server_config(&self) -> Result<ServerConfig> {
        let method: CipherKind = self
            .cfg
            .method
            .parse()
            .map_err(|e| anyhow!("invalid cipher method: {}", self.cfg.method))?;

        // 拼接地址
        let svr_addr = if let Ok(addr) = self.cfg.server_addr.parse::<std::net::IpAddr>() {
            ServerAddr::SocketAddr(SocketAddr::new(addr, self.cfg.server_port))
        } else {
            ServerAddr::DomainName(self.cfg.server_addr.clone(), self.cfg.server_port)
        };

        // 构建 ServerConfig（注意这里返回 Result）
        let svr = ServerConfig::new(svr_addr, self.cfg.password.clone(), method)
            .map_err(|e| anyhow!("failed to build ServerConfig: {}", e))?;

        Ok(svr)
    }

    /// Connect to target via Shadowsocks server and return Connection trait object.
    /// ctx: ConnectionContext must include dst_host/dst_port.
    /// TCP 连接
    pub async fn connect_tcp(&self, ctx: &ConnectionContext) -> Result<Box<dyn Connection>> {
        let host = ctx
            .dst_host
            .clone()
            .ok_or_else(|| anyhow!("missing dst_host in Context"))?;
        let port = ctx
            .dst_port
            .ok_or_else(|| anyhow!("missing dst_port in Context"))?;

        let svr_cfg = self.build_server_config()?;
        let target = SsAddress::DomainNameAddress(host, port);

        let ss_stream = ProxyClientStream::connect(self.ctx.clone(), &svr_cfg, &target)
            .await
            .map_err(|e| anyhow!("shadowsocks: connect_tcp to {} failed: {}", target, e))?;

        Ok(Box::new(ShadowsocksTcpConn {
            inner: Box::new(ss_stream),
        }))
    }

    /// UDP 连接
    pub async fn connect_udp(&self, ctx: &ConnectionContext) -> Result<Box<dyn Connection>> {
        let host = ctx
            .dst_host
            .clone()
            .ok_or_else(|| anyhow!("missing dst_host in Context"))?;
        let port = ctx
            .dst_port
            .ok_or_else(|| anyhow!("missing dst_port in Context"))?;

        let svr_cfg = self.build_server_config()?;
        let target = SsAddress::DomainNameAddress(host, port);
        // UDP 使用 ProxySocket
        // connect 实际上是绑定本地端口并准备好加密上下文，并不会产生真正的网络握手
        let socket = ProxySocket::connect(self.ctx.clone(), &svr_cfg)
            .await
            .map_err(|e| anyhow!("shadowsocks: connect_udp init failed: {}", e))?;

        // UDP 是无连接的，但 Connection trait 像是流式的。
        // 我们需要在 Wrapper 中保存 target，以便 send 时知道发给谁。
        Ok(Box::new(ShadowsocksUdpConn {
            inner: socket,
            target,
        }))
    }
}

/// A thin wrapper that implements your Connection trait over shadowsocks stream
/// TCP 封装
// 1. 定义复合 Trait
pub trait ProxyStream: AsyncRead + AsyncWrite + Unpin + Send + Sync {}
impl<T> ProxyStream for T where T: AsyncRead + AsyncWrite + Unpin + Send + Sync {}

pub struct ShadowsocksTcpConn {
    // 必须 Box 起来，因为 ProxyClientStream 是具体类型，但这里为了通用性
    pub inner: Box<dyn ProxyStream>,
}

#[async_trait]
impl Connection for ShadowsocksTcpConn {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        // Box<dyn ProxyStream> 实现了 AsyncWrite，可以直接调用
        self.inner.write_all(data).await?;
        Ok(())
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        // Box<dyn ProxyStream> 实现了 AsyncRead，可以直接调用
        let n = self.inner.read(buf).await?;
        Ok(n)
    }
}

/// UDP 封装
pub struct ShadowsocksUdpConn {
    pub inner: ProxySocket<UdpSocket>,
    pub target: Address,
}

#[async_trait]
impl Connection for ShadowsocksUdpConn {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        // 5. UDP 发送：使用 send 发送给特定的 target
        // Shadowsocks UDP 需要将目标地址封装在包头中
        self.inner.send(&self.target, data).await?;
        Ok(())
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        // 6. UDP 接收：recv 返回 (usize, Address)
        // Address 是发送方的源地址。在代理场景下，这通常是目标网站的地址。
        // 由于 Connection trait 的 recv 签名不包含返回地址，我们这里丢弃它。
        let (n, _src_addr, _rn) = self.inner.recv(buf).await?;
        Ok(n)
    }
}
