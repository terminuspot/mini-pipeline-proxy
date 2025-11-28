use async_trait::async_trait;
use crate::proxy::context::ConnectionContext;
use anyhow::Result;

pub struct LoggingMiddleware;

#[async_trait]
impl crate::proxy::protocol::Middleware for LoggingMiddleware {
    async fn before(&self, ctx: &mut ConnectionContext) -> Result<()> {
        println!("=> [before] inbound={} dst={:?}:{:?}", ctx.inbound_type, ctx.dst_host, ctx.dst_port);
        Ok(())
    }
    async fn after(&self, ctx: &mut ConnectionContext) -> Result<()> {
        println!("<= [after] inbound={} dst={:?}:{:?}", ctx.inbound_type, ctx.dst_host, ctx.dst_port);
        Ok(())
    }
}
