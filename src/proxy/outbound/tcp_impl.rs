use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use async_trait::async_trait;
use crate::proxy::protocol::Connection;
use anyhow::Result;
use std::any::Any;

pub struct TcpConnection {
    pub stream: TcpStream,
}

impl TcpConnection {
    pub fn new(s: TcpStream) -> Self { Self { stream: s } }
}

#[async_trait]
impl Connection for TcpConnection {
    fn as_any(&mut self) -> &mut dyn Any { self }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.stream.write_all(data).await?;
        Ok(())
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.stream.read(buf).await?;
        Ok(n)
    }
}
