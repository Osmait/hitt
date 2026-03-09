use std::sync::Arc;
use tokio::sync::Mutex;

use super::capture::{CaptureStore, CapturedRequest};

/// Proxy server state
#[derive(Debug)]
pub struct ProxyServer {
    pub port: u16,
    pub active: bool,
    pub captures: Arc<Mutex<CaptureStore>>,
}

impl ProxyServer {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            active: false,
            captures: Arc::new(Mutex::new(CaptureStore::new())),
        }
    }

    /// Start the proxy server
    /// NOTE: Full MITM proxy implementation requires the hudsucker crate.
    /// This is a placeholder that provides the interface.
    pub async fn start(&mut self) -> anyhow::Result<()> {
        self.active = true;
        tracing::info!("Proxy server started on port {}", self.port);
        // In a full implementation, this would start a hudsucker MITM proxy
        // that captures requests/responses and stores them in self.captures
        Ok(())
    }

    pub async fn stop(&mut self) {
        self.active = false;
        tracing::info!("Proxy server stopped");
    }

    pub fn is_active(&self) -> bool {
        self.active
    }
}
