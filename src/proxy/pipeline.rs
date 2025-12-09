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
use crate::proxy::outbound::direct::DirectConnection;

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
        if let Some(tcp_conn) = outbound.as_any().downcast_mut::<DirectConnection>() {
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

    /// 专门为 UDP 或需要手动控制数据流的场景提供的方法
    /// 执行：Middleware(Before) -> Router -> Outbound Connect
    pub async fn create_outbound_connection(&self, ctx: &mut ConnectionContext) -> Result<Box<dyn Connection>> {
        // 1. Run "Before" Middlewares
        for m in &self.middlewares {
            m.before(ctx).await?;
        }

        // 2. Route selection
        let selected_tag = self.router.select_outbound(ctx).await?;
        ctx.outbound_tag = selected_tag;

        // 3. Connect outbound
        // 注意：OutboundManager 内部应该根据 ctx (比如 metadata 或 protocol) 决定建立 TCP 还是 UDP 连接
        // 或者 Router 选出的 tag 对应的 outbound 本身支持 UDP
        let conn = self.outbound_mgr.connect_box(ctx).await?;

        // 4. "After" middlewares are typically for logging results after the session ends.
        // For manually managed connections, the caller might need to handle logging,
        // or we simplify and just return the connection here.

        Ok(conn)
    }
}
