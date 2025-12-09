use crate::proxy::context::ConnectionContext;
use crate::proxy::protocol::Connection;
use anyhow::Result;
use std::any::Any;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore, ServerName};
use tokio_rustls::TlsConnector;

pub struct TlsOutbound {
    pub config: Arc<ClientConfig>,
}

impl TlsOutbound {
    pub fn new(config: Arc<ClientConfig>) -> Self {
        // load webpki roots
        let mut roots = RootCertStore::empty();
        roots.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
            OwnedTrustAnchor::from_subject_spki_name_constraints(
                ta.subject,
                ta.spki,
                ta.name_constraints,
            )
        }));
        let cfg = ClientConfig::builder()
            .with_safe_defaults()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Self {
            config: Arc::new(cfg),
        }
    }

    pub async fn connect(&self, ctx: ConnectionContext) -> Result<Box<dyn Connection>> {
        let host = ctx
            .dst_host
            .clone()
            .ok_or_else(|| anyhow::anyhow!("no host"))?;
        let port = ctx.dst_port.unwrap_or(443);
        let addr = format!("{}:{}", host, port);
        let tcp = TcpStream::connect(addr).await?;
        let connector = TlsConnector::from(self.config.clone());
        let dnsname = ServerName::try_from(host.as_str())?;
        let tls_stream = connector.connect(dnsname, tcp).await?;
        Ok(Box::new(TlsConnection::new(tls_stream)))
    }
}

pub struct TlsConnection {
    inner: TlsStream<TcpStream>,
}

impl TlsConnection {
    pub fn new(s: TlsStream<TcpStream>) -> Self {
        Self { inner: s }
    }
}
#[async_trait::async_trait]
impl Connection for TlsConnection {
    fn as_any(&mut self) -> &mut dyn Any {
        self
    }

    async fn send(&mut self, data: &[u8]) -> Result<()> {
        self.inner.write_all(data).await?;
        Ok(())
    }

    async fn recv(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.inner.read(buf).await?;
        Ok(n)
    }
}
