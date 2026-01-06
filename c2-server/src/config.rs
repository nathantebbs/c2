use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub server: ServerSettings,
    pub security: SecuritySettings,
    pub limits: LimitsSettings,
    pub logging: LoggingSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerSettings {
    pub listen_addr: String,
    pub listen_port: u16,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecuritySettings {
    /// Pre-shared key for HMAC authentication (hex-encoded)
    pub psk: String,
    /// Optional list of allowed client IDs (empty = allow all)
    #[serde(default)]
    pub allowed_client_ids: Vec<String>,
    /// Timestamp skew tolerance in seconds
    #[serde(default = "default_timestamp_skew")]
    pub timestamp_skew_secs: i64,
    /// How long to remember nonces for replay protection (seconds)
    #[serde(default = "default_nonce_ttl")]
    pub nonce_ttl_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LimitsSettings {
    /// Maximum concurrent connections
    #[serde(default = "default_max_conns")]
    pub max_conns: usize,
    /// Maximum frame size in bytes
    #[serde(default = "default_max_frame")]
    pub max_frame_bytes: u32,
    /// Read timeout in seconds
    #[serde(default = "default_read_timeout")]
    pub read_timeout_secs: u64,
    /// Write timeout in seconds
    #[serde(default = "default_write_timeout")]
    pub write_timeout_secs: u64,
    /// Authentication timeout in seconds
    #[serde(default = "default_auth_timeout")]
    pub auth_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LoggingSettings {
    /// Log level: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub log_level: String,
    /// Output logs as JSON
    #[serde(default)]
    pub json_logs: bool,
}

fn default_timestamp_skew() -> i64 {
    120 // ±2 minutes
}

fn default_nonce_ttl() -> u64 {
    300 // 5 minutes
}

fn default_max_conns() -> usize {
    100
}

fn default_max_frame() -> u32 {
    c2_proto::DEFAULT_MAX_FRAME_SIZE
}

fn default_read_timeout() -> u64 {
    30
}

fn default_write_timeout() -> u64 {
    30
}

fn default_auth_timeout() -> u64 {
    60
}

fn default_log_level() -> String {
    "info".to_string()
}

impl ServerConfig {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: ServerConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Self {
            server: ServerSettings {
                listen_addr: "127.0.0.1".to_string(),
                listen_port: 5000,
            },
            security: SecuritySettings {
                psk: "change-me-in-production".to_string(),
                allowed_client_ids: vec![],
                timestamp_skew_secs: default_timestamp_skew(),
                nonce_ttl_secs: default_nonce_ttl(),
            },
            limits: LimitsSettings {
                max_conns: default_max_conns(),
                max_frame_bytes: default_max_frame(),
                read_timeout_secs: default_read_timeout(),
                write_timeout_secs: default_write_timeout(),
                auth_timeout_secs: default_auth_timeout(),
            },
            logging: LoggingSettings {
                log_level: default_log_level(),
                json_logs: false,
            },
        }
    }
}
