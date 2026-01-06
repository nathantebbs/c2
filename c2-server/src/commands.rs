use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;
use tracing::debug;

/// Server state that commands can access
pub struct CommandContext {
    pub server_start_time: Instant,
    pub version: String,
}

/// Executes a command and returns the result
pub fn execute_command(
    cmd: &str,
    args: &HashMap<String, Value>,
    ctx: &CommandContext,
) -> Result<Value, String> {
    debug!("Executing command: {} with args: {:?}", cmd, args);

    match cmd {
        "PING" => cmd_ping(),
        "ECHO" => cmd_echo(args),
        "TIME" => cmd_time(),
        "STATUS" => cmd_status(ctx),
        _ => Err(format!("Unknown command: {}", cmd)),
    }
}

/// PING command - returns PONG
fn cmd_ping() -> Result<Value, String> {
    Ok(json!({
        "message": "PONG"
    }))
}

/// ECHO command - echoes back the provided text
fn cmd_echo(args: &HashMap<String, Value>) -> Result<Value, String> {
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "ECHO requires 'text' argument".to_string())?;

    Ok(json!({
        "echo": text
    }))
}

/// TIME command - returns server timestamp
fn cmd_time() -> Result<Value, String> {
    let now = chrono::Utc::now();

    Ok(json!({
        "timestamp": now.timestamp(),
        "iso8601": now.to_rfc3339(),
    }))
}

/// STATUS command - returns server status and uptime
fn cmd_status(ctx: &CommandContext) -> Result<Value, String> {
    let uptime = ctx.server_start_time.elapsed();

    Ok(json!({
        "version": ctx.version,
        "uptime_secs": uptime.as_secs(),
        "status": "running",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ping() {
        let result = cmd_ping().unwrap();
        assert_eq!(result["message"], "PONG");
    }

    #[test]
    fn test_echo() {
        let mut args = HashMap::new();
        args.insert("text".to_string(), json!("hello world"));

        let result = cmd_echo(&args).unwrap();
        assert_eq!(result["echo"], "hello world");
    }

    #[test]
    fn test_echo_missing_arg() {
        let args = HashMap::new();
        let result = cmd_echo(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_time() {
        let result = cmd_time().unwrap();
        assert!(result.get("timestamp").is_some());
        assert!(result.get("iso8601").is_some());
    }

    #[test]
    fn test_status() {
        let ctx = CommandContext {
            server_start_time: Instant::now(),
            version: "0.1.0".to_string(),
        };

        let result = cmd_status(&ctx).unwrap();
        assert_eq!(result["version"], "0.1.0");
        assert_eq!(result["status"], "running");
        assert!(result.get("uptime_secs").is_some());
    }
}
