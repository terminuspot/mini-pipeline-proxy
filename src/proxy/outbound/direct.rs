use crate::proxy::protocol::Connection;
use anyhow::Result;
use async_trait::async_trait;
use std::any::Any;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

pub struct DirectOutbound;

impl DirectOutbound {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct DirectConnection {
    pub stream: TcpStream,
}

impl DirectConnection {
    pub fn new(s: TcpStream) -> Self {
        Self { stream: s }
    }
}

#[async_trait]
impl Connection for DirectConnection {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.stream.write_all(data).await?;
        Ok(())
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.stream.read(buf).await?;
        Ok(n)
    }
}

impl DirectOutbound {
    pub async fn connect(
        &self,
        ctx: &crate::proxy::context::ConnectionContext,
    ) -> Result<Box<dyn Connection>> {
        let host = ctx
            .dst_host
            .clone()
            .unwrap_or_else(|| "93.184.216.34".to_string());
        let port = ctx.dst_port.unwrap_or(80);
        let addr = format!("{}:{}", host, port);
        let s = TcpStream::connect(&addr).await?;

        Ok(Box::new(DirectConnection::new(s)))
    }
}
