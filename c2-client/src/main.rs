mod config;
mod client;

use crate::client::C2Client;
use crate::config::ClientConfig;
use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::Duration;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    init_logging();

    info!("C2 Client v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = load_config()?;

    // Decode PSK from hex
    let psk = hex::decode(&config.security.psk)
        .map_err(|_| anyhow::anyhow!("Invalid PSK: must be hex-encoded"))?;

    // Connect to relay
    let relay_addr = format!("{}:{}", config.client.relay_addr, config.client.relay_port);
    info!("Connecting to relay at {}...", relay_addr);

    let mut stream = match tokio::time::timeout(
        Duration::from_secs(config.timeouts.connect_timeout_secs),
        TcpStream::connect(&relay_addr),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(e)) => {
            error!("Failed to connect: {}", e);
            return Err(e.into());
        }
        Err(_) => {
            error!("Connection timeout");
            return Err(anyhow::anyhow!("Connection timeout"));
        }
    };

    info!("Connected to {}", relay_addr);

    // Create client and authenticate
    let mut client = C2Client::new(
        config.client.client_id.clone(),
        psk,
        config.timeouts.read_timeout_secs,
        config.timeouts.write_timeout_secs,
    );

    if let Err(e) = client.authenticate(&mut stream).await {
        error!("Authentication failed: {}", e);
        return Err(e.into());
    }

    info!("Authentication successful!");
    println!("\nC2 Client connected and authenticated.");
    println!("Available commands: PING, ECHO <text>, TIME, STATUS, quit");
    println!("Type a command and press Enter:\n");

    // Interactive command loop
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut line = String::new();

    loop {
        print!("> ");
        // Manually flush stdout since print! doesn't auto-flush
        use std::io::Write;
        std::io::stdout().flush()?;

        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                error!("Failed to read input: {}", e);
                break;
            }
        }

        let input = line.trim();

        if input.is_empty() {
            continue;
        }

        if input == "quit" || input == "exit" {
            info!("Exiting...");
            break;
        }

        // Parse command
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0].to_uppercase();
        let args_str = if parts.len() > 1 { parts[1] } else { "" };

        // Build args map
        let mut args = HashMap::new();

        if cmd == "ECHO" {
            if args_str.is_empty() {
                println!("Error: ECHO requires text argument");
                continue;
            }
            args.insert("text".to_string(), serde_json::json!(args_str));
        }

        // Send command
        match client.send_command(&mut stream, &cmd, args).await {
            Ok(result) => {
                println!("[OK] Response: {}", serde_json::to_string_pretty(&result)?);
            }
            Err(e) => {
                warn!("Command failed: {}", e);
                println!("[ERROR] {}", e);
            }
        }
    }

    Ok(())
}

fn load_config() -> anyhow::Result<ClientConfig> {
    // Try configs/client.toml (development)
    if let Ok(config) = ClientConfig::from_file("configs/client.toml") {
        info!("Loaded config from configs/client.toml");
        return Ok(config);
    }

    // Try ./client.toml (current directory)
    if let Ok(config) = ClientConfig::from_file("client.toml") {
        info!("Loaded config from client.toml");
        return Ok(config);
    }

    // Use default config as last resort
    warn!("No config file found, using default configuration");
    warn!("IMPORTANT: Change the PSK in production!");
    Ok(ClientConfig::default_config())
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
}
