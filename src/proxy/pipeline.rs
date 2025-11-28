use crate::proxy::context::ConnectionContext;
use crate::proxy::middleware::logging::LoggingMiddleware;
use crate::proxy::outbound::SimpleOutboundManager;
use crate::proxy::protocol::{Connection, Middleware, OutboundManager, Router};
use crate::proxy::router::simple::SimpleRouter;
use crate::proxy::transport::tcp::TcpTransport;
use anyhow::Result;
use std::sync::Arc;
use tokio::io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use crate::proxy::outbound::tcp_impl::TcpConnection;

pub struct Pipeline {
    router: Arc<dyn Router>,
    outbound_mgr: Arc<dyn OutboundManager>,
    middlewares: Vec<Arc<dyn Middleware>>,
}

impl Pipeline {
    pub fn new(router: Arc<dyn Router>, outbound_mgr: Arc<dyn OutboundManager>, middlewares: Vec<Arc<dyn Middleware>>) -> Self {
        Self { router, outbound_mgr, middlewares }
    }

    pub fn new_default() -> Self {
        let router = Arc::new(SimpleRouter::new("direct".to_string(), "proxy".to_string()));
        let transport = Arc::new(TcpTransport::new());
        let outbound = Arc::new(SimpleOutboundManager::new(transport));
        let middlewares: Vec<Arc<dyn Middleware>> = vec![
            Arc::new(LoggingMiddleware),
        ];
        Self::new(router, outbound, middlewares)
    }

    pub async fn process(&self, mut ctx: ConnectionContext, mut inbound: TcpStream) -> Result<()> {
        // middleware before
        for m in &self.middlewares {
            m.before(&mut ctx).await?;
        }

        // route selection
        let selected = self.router.select_outbound(&ctx).await?;
        ctx.outbound_tag = selected;

        // connect outbound
        let mut outbound = self.outbound_mgr.connect_box(&ctx).await?;

        // Try downcast to TcpConnection to use copy_bidirectional for performance.
        // If downcast fails, fallback to manual relay using Connection trait methods.
        let mut used_direct = false;
        if let Some(tcp_conn) = outbound.as_any().downcast_mut::<TcpConnection>() {
            used_direct = true;
            // perform bidirectional copy between inbound and tcp_conn.stream
            // let (mut ri, mut wi) = inbound.split();
            let mut os = &mut tcp_conn.stream;
            // copy_bidirectional requires AsyncRead+AsyncWrite; both are available
            let _ = copy_bidirectional(&mut inbound, &mut os).await?;
        } else {
            // fallback generic relay using Connection trait
            let (mut ri, mut wi) = inbound.split();
            let outbound_ref = Arc::new(tokio::sync::Mutex::new(outbound));
            // client -> outbound
            let c2o = {
                let outbound_ref = outbound_ref.clone();
                async move {
                    let mut buf = [0u8; 4096];
                    loop {
                        let n = ri.read(&mut buf).await?;
                        if n == 0 { break; }
                        let mut out = outbound_ref.lock().await;
                        out.send(&buf[..n]).await?;
                    }
                    Ok::<(), anyhow::Error>(())
                }
            };
            // outbound -> client
            let o2c = {
                let outbound_ref = outbound_ref.clone();
                async move {
                    let mut buf = [0u8; 4096];
                    loop {
                        let mut out = outbound_ref.lock().await;
                        let n = out.recv(&mut buf).await?;
                        drop(out);
                        if n == 0 { break; }
                        wi.write_all(&buf[..n]).await?;
                    }
                    Ok::<(), anyhow::Error>(())
                }
            };
            tokio::try_join!(c2o, o2c)?;
        }

        // middleware after
        for m in &self.middlewares {
            m.after(&mut ctx).await?;
        }

        Ok(())
    }
}
