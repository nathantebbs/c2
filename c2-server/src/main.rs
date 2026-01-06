mod config;
mod session;
mod commands;
mod handler;

use crate::config::ServerConfig;
use crate::commands::CommandContext;
use crate::handler::ConnectionHandler;
use crate::session::SessionManager;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load configuration
    let config = load_config()?;

    // Initialize logging
    init_logging(&config);

    info!("C2 Server v{} starting...", env!("CARGO_PKG_VERSION"));

    // Decode PSK from hex
    let psk = hex::decode(&config.security.psk)
        .map_err(|_| anyhow::anyhow!("Invalid PSK: must be hex-encoded"))?;

    if psk.len() < 32 {
        warn!("PSK is shorter than recommended (32 bytes). Using length: {} bytes", psk.len());
    }

    // Create session manager
    let session_manager = SessionManager::new(
        psk,
        config.security.allowed_client_ids.clone(),
        config.security.timestamp_skew_secs,
        config.security.nonce_ttl_secs,
    );

    // Create command context
    let command_context = Arc::new(CommandContext {
        server_start_time: Instant::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    // Create connection handler
    let handler = Arc::new(ConnectionHandler::new(
        session_manager,
        command_context,
        config.limits.max_frame_bytes,
        config.limits.read_timeout_secs,
        config.limits.write_timeout_secs,
        config.limits.auth_timeout_secs,
    ));

    // Create connection limit semaphore
    let connection_semaphore = Arc::new(Semaphore::new(config.limits.max_conns));

    // Bind to listen address
    let listen_addr = format!("{}:{}", config.server.listen_addr, config.server.listen_port);
    let listener = TcpListener::bind(&listen_addr).await?;

    info!("Listening on {}", listen_addr);
    info!("Maximum concurrent connections: {}", config.limits.max_conns);

    // Accept connections
    loop {
        // Acquire connection slot
        let permit = connection_semaphore.clone().acquire_owned().await?;

        // Accept connection
        let (stream, remote_addr) = match listener.accept().await {
            Ok(conn) => conn,
            Err(e) => {
                error!("Failed to accept connection: {}", e);
                continue;
            }
        };

        let remote_addr_str = remote_addr.to_string();
        let handler = handler.clone();

        // Spawn connection handler
        tokio::spawn(async move {
            handler.handle(stream, remote_addr_str).await;
            drop(permit); // Release connection slot
        });
    }
}

fn load_config() -> anyhow::Result<ServerConfig> {
    // Try to load from /etc/c2/config.toml first (production)
    if let Ok(config) = ServerConfig::from_file("/etc/c2/config.toml") {
        info!("Loaded config from /etc/c2/config.toml");
        return Ok(config);
    }

    // Try configs/server.toml (development)
    if let Ok(config) = ServerConfig::from_file("configs/server.toml") {
        info!("Loaded config from configs/server.toml");
        return Ok(config);
    }

    // Try ./server.toml (current directory)
    if let Ok(config) = ServerConfig::from_file("server.toml") {
        info!("Loaded config from server.toml");
        return Ok(config);
    }

    // Use default config as last resort
    warn!("No config file found, using default configuration");
    warn!("IMPORTANT: Change the PSK in production!");
    Ok(ServerConfig::default_config())
}

fn init_logging(config: &ServerConfig) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.logging.log_level));

    if config.logging.json_logs {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .init();
    }
}
