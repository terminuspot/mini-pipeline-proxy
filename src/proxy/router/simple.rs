use async_trait::async_trait;
use crate::proxy::context::ConnectionContext;
use anyhow::Result;

pub struct SimpleRouter {
    pub direct_outbound: String,
    pub proxy_outbound: String,
}

impl SimpleRouter {
    pub fn new(direct: String, proxy: String) -> Self {
        Self { direct_outbound: direct, proxy_outbound: proxy }
    }
}

#[async_trait]
impl crate::proxy::protocol::Router for SimpleRouter {
    async fn select_outbound(&self, ctx: &ConnectionContext) -> Result<String> {
        if let Some(h) = &ctx.dst_host {
            if h.contains("example") {
                return Ok(self.proxy_outbound.clone());
            }
        }
        Ok(self.direct_outbound.clone())
    }
}
