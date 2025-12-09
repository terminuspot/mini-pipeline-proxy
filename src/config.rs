use serde::Deserialize;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

// 总配置结构体，对应 config.json 的根结构
#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub shadowsocks: ShadowsocksConfig,
}

// Shadowsocks 具体配置
// 注意：字段名称必须与 json 中的 key 一致
#[derive(Debug, Deserialize, Clone)]
pub struct ShadowsocksConfig {
    pub server_addr: String,
    pub server_port: u16,
    pub password: String,
    pub method: String,
}

impl AppConfig {
    /// 从指定路径加载配置
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // 读取文件内容
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {:?}", path))?;

        // 解析 JSON
        let config: AppConfig = serde_json::from_str(&content)
            .with_context(|| "failed to parse config json")?;

        Ok(config)
    }
}