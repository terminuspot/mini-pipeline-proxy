use async_trait::async_trait;
use tokio::net::TcpStream;
use crate::proxy::context::ConnectionContext;
use std::any::Any;
use anyhow::Result;

#[async_trait]
pub trait InboundHandler: Send + Sync + Clone + 'static {
    async fn handle_inbound(&self, stream: TcpStream) -> Result<()>;
}

#[async_trait]
pub trait OutboundManager: Send + Sync {
    async fn connect_box(&self, ctx: &ConnectionContext) -> Result<Box<dyn Connection>>;
}

#[async_trait]
pub trait Router: Send + Sync {
    async fn select_outbound(&self, ctx: &ConnectionContext) -> Result<String>;
}

#[async_trait]
pub trait Middleware: Send + Sync {
    async fn before(&self, ctx: &mut ConnectionContext) -> Result<()>;
    async fn after(&self, ctx: &mut ConnectionContext) -> Result<()>;
}

/// Connection trait for outbound connections.
#[async_trait]
pub trait Connection: Send + Sync + Any {
    fn as_any(&mut self) -> &mut dyn Any;
    async fn send(&mut self, data: &[u8]) -> Result<()>;
    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize>;
}
