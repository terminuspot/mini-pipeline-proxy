use crate::proxy::protocol::Connection;
use crate::proxy::context::ConnectionContext;
use async_trait::async_trait;
use anyhow::Result;
use std::sync::Arc;

pub mod direct;
pub mod tls;
pub mod shadowsocks;
pub mod trojan;
pub mod vmess;
pub mod hysteria;

/// OutboundManager 聚合，各种类型的 outbound 在这里 dispatch
#[derive(Clone)]
pub struct OutboundManager {
    // transport/backends; 可按需扩展或通过配置注入
    pub direct: Arc<direct::DirectOutbound>,
    
}

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
        Ok(Box::new(direct::DirectConnection::new(stream)))
    }
}
