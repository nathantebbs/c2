# C2 Simulator (Lab)

A secure command-and-control simulator for controlled Linux lab environments, demonstrating authenticated client/server communication, structured messaging, replay-safe request handling, and service-style operation.

**Purpose**: Educational and reliability testing for secure service operations (not offensive tooling).

## Features

- HMAC-SHA256 authentication with pre-shared keys
- Replay attack protection (nonce tracking)
- Sequence number validation
- Length-prefixed JSON framing protocol
- Structured logging with tracing
- systemd-friendly service operation
- Connection limits and timeouts
- Graceful shutdown handling

## Architecture

```
+-------------+         +--------------+         +--------------+
| client-vm   |-------->|  relay-vm    |-------->|   c2-vm      |
| c2-client   |  public |  GateRelay   | private |  c2-server   |
+-------------+   :4000 +--------------+   :5000 +--------------+
```

## Project Structure

```
c2/
|-- c2-proto/          # Shared protocol library (framing, crypto, messages)
|-- c2-server/         # C2 server implementation
|-- c2-client/         # C2 client CLI
|-- configs/           # Example configuration files
|-- deployments/       # systemd units and deployment docs
+-- prompts/           # Implementation specifications
```

## Quick Start

### Build

```bash
cargo build --release
```

Binaries will be in `target/release/c2-server` and `target/release/c2-client`.

### Run Server (Development)

```bash
# Uses configs/server.toml by default
./target/release/c2-server
```

### Run Client (Development)

```bash
# Uses configs/client.toml by default
./target/release/c2-client
```

### Test Commands

Once connected:

```
> PING             # Returns PONG
> TIME             # Returns server timestamp
> STATUS           # Returns server version and uptime
> ECHO hello world # Echoes back text
> quit             # Disconnects
```

## Configuration

### Server (configs/server.toml)

```toml
[server]
listen_addr = "127.0.0.1"
listen_port = 5000

[security]
psk = "0123456789abcdef..."  # CHANGE IN PRODUCTION
allowed_client_ids = []       # Optional allowlist

[limits]
max_conns = 100
max_frame_bytes = 1048576
read_timeout_secs = 30
write_timeout_secs = 30
auth_timeout_secs = 60

[logging]
log_level = "info"
json_logs = false
```

### Client (configs/client.toml)

```toml
[client]
client_id = "client-1"
relay_addr = "127.0.0.1"
relay_port = 4000

[security]
psk = "0123456789abcdef..."  # Must match server

[timeouts]
connect_timeout_secs = 10
read_timeout_secs = 30
write_timeout_secs = 30
```

## Protocol Overview

### Transport

- TCP with length-prefixed JSON frames
- Frame format: `u32_be length` + `JSON payload`

### Authentication Flow

1. Client -> Server: `HELLO` (client_id)
2. Server -> Client: `CHALLENGE` (server_nonce)
3. Client -> Server: `AUTH` (client_nonce, HMAC signature)
4. Server -> Client: `AUTH_OK` (session_id)

### Command Execution

After authentication, client sends `CMD` messages with:
- Monotonically increasing sequence numbers
- Per-request nonces
- HMAC signatures over (session_id, seq, nonce, cmd, args)

Server validates:
- Sequence number is greater than last seen
- Nonce hasn't been used (replay protection)
- Signature is valid
- Command is in allowlist

## Security Features

### Replay Protection

- Server tracks used (client_id, server_nonce, client_nonce) tuples
- Nonces expire after configurable TTL (default 5 minutes)
- Replayed authentication attempts are rejected

### Sequence Validation

- Each session maintains monotonic sequence counter
- Commands with old or duplicate sequence numbers are rejected
- Prevents replay of command messages

### Timestamp Validation

- All messages include Unix timestamp
- Server rejects messages outside of skew window (default ±2 minutes)
- Protects against extremely old replayed messages

### Input Validation

- Frame size limits prevent memory exhaustion
- Command allowlist prevents arbitrary execution
- Argument size limits prevent resource abuse
- Connection limits prevent DoS

## Testing

```bash
# Run all tests
cargo test --workspace

# Run specific test suite
cargo test --package c2-proto
cargo test --package c2-server
```

## Deployment

For production deployment with the 3-VM topology (relay, c2-server, client), see:

**[deployments/DEPLOYMENT.md](deployments/DEPLOYMENT.md)**

This includes:
- systemd service setup
- Firewall configuration
- Security hardening
- Monitoring and troubleshooting

## Development

### Protocol Library (c2-proto)

Core protocol implementation:
- `messages.rs` - Message types and serialization
- `framing.rs` - Length-prefixed transport codec
- `crypto.rs` - HMAC-SHA256 utilities

### Server (c2-server)

- `session.rs` - Session management and replay protection
- `commands.rs` - Command handlers (PING, ECHO, TIME, STATUS)
- `handler.rs` - Connection handler and authentication
- `config.rs` - Configuration loading
- `main.rs` - Server entrypoint

### Client (c2-client)

- `client.rs` - Protocol client implementation
- `config.rs` - Configuration loading
- `main.rs` - Interactive CLI

## Logging

Server logs include:
- Connection events (accept, close)
- Authentication success/failure
- Command execution and results
- Errors and timeouts
- Security events (replay attempts, invalid signatures)

Enable JSON logs for structured output:

```toml
[logging]
json_logs = true
```

View logs with journald:

```bash
sudo journalctl -u c2-server -f
```

## License

MIT

## Threat Model

This simulator defends against:
- Unauthenticated clients
- Replayed requests
- Malformed/oversized messages
- Connection floods (basic caps)
- Partial reads/writes, dropped connections, timeouts

Out of scope:
- Stealth, evasion, persistence
- Exploitation, covert communications
- Advanced DoS attacks
