use serde::{Serialize, Deserialize};
use std::net::SocketAddr;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionContext {
    pub src_addr: SocketAddr,
    pub dst_host: Option<String>,
    pub dst_port: Option<u16>,
    pub original_dst: Option<SocketAddr>,
    pub inbound_type: String,
    pub metadata: serde_json::Value,
    // tag chosen by router, e.g. 'direct' or 'proxy1'
    pub outbound_tag: String,
}
