use serde::{Deserialize, Serialize};
use std::sync::Arc;
use parking_lot::RwLock;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub client_timeout: Duration,
    pub upstream_timeout: Duration,
    pub max_history_size: usize,
    pub max_body_size: usize,
    pub truncate_body_at: usize,
    pub access_token: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            client_timeout: Duration::from_secs(30),
            upstream_timeout: Duration::from_millis(500),
            max_history_size: 100,
            max_body_size: 1024 * 1024, // 1MB
            truncate_body_at: 1024,     // 1KB
            access_token: uuid::Uuid::new_v4().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SharedConfig {
    inner: Arc<RwLock<ProxyConfig>>,
}

impl SharedConfig {
    pub fn new(config: ProxyConfig) -> Self {
        Self {
            inner: Arc::new(RwLock::new(config)),
        }
    }

    pub fn read(&self) -> parking_lot::RwLockReadGuard<ProxyConfig> {
        self.inner.read()
    }

    #[allow(dead_code)]
    pub fn write(&self) -> parking_lot::RwLockWriteGuard<ProxyConfig> {
        self.inner.write()
    }

    pub fn update<F>(&self, f: F) 
    where
        F: FnOnce(&mut ProxyConfig),
    {
        let mut config = self.inner.write();
        f(&mut config);
    }

    #[allow(dead_code)]
    pub fn get_client_timeout(&self) -> Duration {
        self.inner.read().client_timeout
    }

    #[allow(dead_code)]
    pub fn get_upstream_timeout(&self) -> Duration {
        self.inner.read().upstream_timeout
    }

    pub fn get_access_token(&self) -> String {
        self.inner.read().access_token.clone()
    }
}

impl Default for SharedConfig {
    fn default() -> Self {
        Self::new(ProxyConfig::default())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigUpdate {
    pub client_timeout_ms: Option<u64>,
    pub upstream_timeout_ms: Option<u64>,
    pub max_history_size: Option<usize>,
    pub max_body_size: Option<usize>,
    pub truncate_body_at: Option<usize>,
}

impl ConfigUpdate {
    pub fn apply_to(&self, config: &mut ProxyConfig) {
        if let Some(timeout) = self.client_timeout_ms {
            config.client_timeout = Duration::from_millis(timeout);
        }
        if let Some(timeout) = self.upstream_timeout_ms {
            config.upstream_timeout = Duration::from_millis(timeout);
        }
        if let Some(size) = self.max_history_size {
            config.max_history_size = size;
        }
        if let Some(size) = self.max_body_size {
            config.max_body_size = size;
        }
        if let Some(size) = self.truncate_body_at {
            config.truncate_body_at = size;
        }
    }
}