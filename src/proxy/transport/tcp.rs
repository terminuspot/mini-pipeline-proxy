use tokio::net::TcpStream;
use anyhow::Result;

pub struct TcpTransport;

impl TcpTransport {
    pub fn new() -> Self { Self {} }
    pub async fn connect_addr(&self, addr: &str) -> Result<TcpStream> {
        let s = TcpStream::connect(addr).await?;
        Ok(s)
    }
}
