use async_trait::async_trait;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::sync::Arc;
use crate::proxy::context::ConnectionContext;
use crate::proxy::protocol::InboundHandler;
use anyhow::Result;

#[derive(Clone)]
pub struct Socks5Inbound {
    pipeline: Arc<crate::proxy::pipeline::Pipeline>,
}

impl Socks5Inbound {
    pub fn new(pipeline: Arc<crate::proxy::pipeline::Pipeline>) -> Self {
        Self { pipeline }
    }
}

#[async_trait]
impl InboundHandler for Socks5Inbound {
    async fn handle_inbound(&self, mut stream: TcpStream) -> Result<()> {
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
            0x01 => { // IPv4
                let mut addr = [0u8; 4];
                stream.read_exact(&mut addr).await?;
                let mut portb = [0u8; 2];
                stream.read_exact(&mut portb).await?;
                let port = u16::from_be_bytes(portb);
                let ip = std::net::Ipv4Addr::from(addr);
                (Some(ip.to_string()), Some(port))
            }
            0x03 => { // domain
                let mut lenb = [0u8;1];
                stream.read_exact(&mut lenb).await?;
                let len = lenb[0] as usize;
                let mut domain = vec![0u8; len];
                stream.read_exact(&mut domain).await?;
                let mut portb = [0u8; 2];
                stream.read_exact(&mut portb).await?;
                let port = u16::from_be_bytes(portb);
                (Some(String::from_utf8(domain).unwrap()), Some(port))
            }
            0x04 => { // IPv6 (not handled fully here)
                return Err(anyhow::anyhow!("IPv6 not supported in this demo"));
            }
            _ => (None, None),
        };

        if cmd != 0x01 {
            // only CONNECT supported in this demo
            stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0,0,0,0,0,0]).await?;
            return Err(anyhow::anyhow!("only CONNECT supported"));
        }

        // reply success (we will actually connect later in pipeline)
        // BND.ADDR and BND.PORT set to zero here
        stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0,0,0,0,0,0]).await?;

        // build context
        let ctx = ConnectionContext {
            src_addr: stream.peer_addr()?,
            dst_host: dst.0,
            dst_port: dst.1,
            original_dst: None,
            inbound_type: "socks5".to_string(),
            metadata: serde_json::json!({}),
            outbound_tag: "".to_string(),
        };

        // hand over to pipeline
        self.pipeline.process(ctx, stream).await
    }
}
