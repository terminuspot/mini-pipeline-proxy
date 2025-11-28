use crate::proxy::protocol::Connection;
use crate::proxy::context::ConnectionContext;
use async_trait::async_trait;
use anyhow::Result;
use std::sync::Arc;

pub mod tcp_impl;

#[derive(Clone)]
pub struct SimpleOutboundManager {
    transport: Arc<crate::proxy::transport::tcp::TcpTransport>,
}

impl SimpleOutboundManager {
    pub fn new(transport: Arc<crate::proxy::transport::tcp::TcpTransport>) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl crate::proxy::protocol::OutboundManager for SimpleOutboundManager {
    async fn connect_box(&self, ctx: &ConnectionContext) -> Result<Box<dyn Connection>> {
        // For now only direct TCP outbound. Extend by inspecting ctx.outbound_tag.
        let host = ctx.dst_host.clone().unwrap_or_else(|| "93.184.216.34".to_string());
        let port = ctx.dst_port.unwrap_or(80);
        let addr = format!("{}:{}", host, port);
        let stream = self.transport.connect_addr(&addr).await?;
        Ok(Box::new(tcp_impl::TcpConnection::new(stream)))
    }
}
