use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientConfig {
    pub client: ClientSettings,
    pub security: SecuritySettings,
    pub timeouts: TimeoutSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientSettings {
    pub client_id: String,
    pub relay_addr: String,
    pub relay_port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecuritySettings {
    /// Pre-shared key for HMAC authentication (hex-encoded)
    pub psk: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimeoutSettings {
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_read_timeout")]
    pub read_timeout_secs: u64,
    #[serde(default = "default_write_timeout")]
    pub write_timeout_secs: u64,
}

fn default_connect_timeout() -> u64 {
    10
}

fn default_read_timeout() -> u64 {
    30
}

fn default_write_timeout() -> u64 {
    30
}

impl ClientConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: ClientConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Self {
            client: ClientSettings {
                client_id: "client-1".to_string(),
                relay_addr: "127.0.0.1".to_string(),
                relay_port: 4000,
            },
            security: SecuritySettings {
                psk: "change-me-in-production".to_string(),
            },
            timeouts: TimeoutSettings {
                connect_timeout_secs: default_connect_timeout(),
                read_timeout_secs: default_read_timeout(),
                write_timeout_secs: default_write_timeout(),
            },
        }
    }
}
