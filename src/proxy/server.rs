use anyhow::Result;
use tokio::net::TcpListener;
use std::sync::Arc;
use crate::proxy::protocol::InboundHandler;

pub struct Server {
    bind_addr: String,
}

impl Server {
    pub fn new(bind_addr: String) -> Self { Self { bind_addr } }

    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        println!("listening on {}", &self.bind_addr);

        // build pipeline default
        let pipeline = Arc::new(crate::proxy::pipeline::Pipeline::new_default());

        let inbound = crate::proxy::socks5::Socks5Inbound::new(pipeline.clone());

        loop {
            let (stream, addr) = listener.accept().await?;
            let inbound = inbound.clone();
            tokio::spawn(async move {
                if let Err(e) = inbound.handle_inbound(stream).await {
                    eprintln!("connection error: {:?}", e);
                }
            });
        }
    }
}
