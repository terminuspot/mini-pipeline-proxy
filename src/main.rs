mod proxy;

use anyhow::Result;
use proxy::server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    // Start server on 127.0.0.1:1080
    let s = Server::new("127.0.0.1:1080".to_string());
    s.run().await
}
