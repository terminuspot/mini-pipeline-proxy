mod proxy;
mod config;

use anyhow::Result;
use tokio::fs;
use proxy::server::Server;
use crate::config::AppConfig;
use crate::proxy::outbound::shadowsocks::ShadowsocksOutbound;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 加载配置
    println!("Loading config from config.json...");
    let config = AppConfig::load("config.json")?;

    println!("Config loaded. Server: {}:{}",
             config.shadowsocks.server_addr,
             config.shadowsocks.server_port
    );

    // 2. 初始化 Shadowsocks Outbound
    let ss_outbound = ShadowsocksOutbound::new(config.shadowsocks.clone());
    // 3. 初始化 Pipeline


    // Start server on 127.0.0.1:1080
    let s = Server::new("127.0.0.1:1080".to_string());
    s.run().await
}
