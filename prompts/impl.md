# C2 Simulator (Lab) – Implementation Spec (v1)

## Purpose
Build a **small-scale command-and-control simulator** for a controlled Linux lab environment to demonstrate:
- authenticated client/server communication
- structured messaging + replay-safe request handling
- centralized logging and fault-aware behavior
- service-style operation (systemd-friendly)
- use of **GateRelay** as an ingress relay layer (behind the scenes)

This is a **simulation for secure service operations** and reliability testing (not offensive tooling).

## High-Level Architecture

### Nodes (VMs)
1. **relay-vm (DMZ / ingress)**  
   - Runs: **GateRelay** as non-root systemd service  
   - Listens on: `RELAY_LISTEN_ADDR:RELAY_LISTEN_PORT` (ex: `0.0.0.0:4000`)  
   - Forwards to: `c2-vm:C2_PORT` (ex: `10.0.0.30:5000`)  
   - Firewall: allow only relay port inbound (+ restricted SSH)

2. **c2-vm (internal)**  
   - Runs: **c2-server** as non-root systemd service  
   - Listens on private interface: `10.0.0.30:5000` (not exposed to public)  
   - Firewall: allow inbound only from `relay-vm`

3. **client-vm (simulated client)**  
   - Runs: **c2-client** CLI for testing
   - Connects to: `relay-vm:RELAY_LISTEN_PORT`

### Data Flow
`client -> GateRelay -> c2-server`
GateRelay is a transparent TCP relay. Authentication + protocol enforcement happen at **c2-server**.

## Scope (v1)

### Functional Requirements
- Client connects to relay endpoint and communicates with c2-server (through GateRelay).
- Protocol supports:
  - handshake/authentication
  - command request/response messages
  - heartbeats (optional but recommended)
- Commands are **benign simulation tasks**, e.g.:
  - `PING` -> `PONG`
  - `ECHO <text>` -> `ECHO <text>`
  - `TIME` -> server timestamp
  - `STATUS` -> server version/uptime summary
- Server logs:
  - connection accepted/closed
  - auth success/failure
  - commands executed + result status
  - errors/timeouts

### Non-Functional Requirements
- **Security**: authenticated messages, replay resistance, input validation, bounded resource usage
- **Reliability**: timeouts, graceful shutdown, predictable failure behavior
- **Operability**: structured logs, simple config files, systemd unit compatibility

## Threat Model (Lab-Appropriate)
Defend against:
- unauthenticated clients
- replayed requests
- malformed/oversized messages
- connection floods (basic caps)
- partial reads/writes, dropped connections, timeouts

Out of scope:
- stealth, evasion, persistence, exploitation, covert comms

## Protocol Design (v1)

### Transport
- TCP stream.
- Messages are **length-prefixed** frames to avoid delimiter ambiguity.

### Frame Format
- `u32_be length` + `payload_bytes[length]`
- Payload is **JSON** (human-readable) OR **MessagePack** (more compact). Choose JSON for v1.

### Message Schema (JSON)
All messages include:
- `type`: string (e.g. `"hello"`, `"auth"`, `"cmd"`, `"resp"`, `"err"`, `"ping"`)
- `ts`: unix epoch seconds (int)
- `nonce`: random string (client-generated per request)
- `session_id`: string (after auth)
- `seq`: integer monotonic per session

#### Authentication
Use **HMAC-SHA256** with a pre-shared key (PSK) stored in config on both sides.
- Client sends `hello` with `client_id`
- Server responds with `challenge` (random `server_nonce`)
- Client sends `auth` containing:
  - `client_id`
  - `server_nonce`
  - `client_nonce`
  - `sig = HMAC(PSK, client_id | server_nonce | client_nonce)`
- Server verifies and returns `auth_ok` with `session_id`

Replay resistance:
- server stores recent `(client_id, server_nonce, client_nonce)` for a short TTL and rejects reuse
- require `ts` within skew window (e.g., ±120s)

#### Command Request
`cmd` message includes:
- `cmd`: string enum
- `args`: object
- `seq`: incrementing integer
- `sig = HMAC(session_key, session_id | seq | nonce | cmd | canonical_json(args))`

Where `session_key = HMAC(PSK, session_id | server_nonce | client_nonce)`.

Server checks:
- valid session_id
- seq monotonic (reject old seq)
- args size limits
- cmd allowlist
- signature valid

Server response:
- `resp` with matching `seq` and status/result

## Config

### c2-server config (TOML)
- listen_addr, listen_port
- psk (or path to secret)
- allowed_client_ids (optional)
- limits:
  - max_conns
  - max_frame_bytes
  - read_timeout_secs
  - write_timeout_secs
  - auth_timeout_secs
- logging:
  - log_level
  - json_logs true/false

### c2-client config
- relay_addr, relay_port
- client_id
- psk
- timeouts

## Implementation Guidance

### Language
Prefer **Go** (fits your skills list and is strong for networking), but Rust is acceptable.

### Concurrency Model (Go suggestion)
- accept loop spawns goroutine per connection
- each connection uses:
  - framed reader/writer
  - auth phase with timeout
  - session state (seq, session_id)
- enforce limits:
  - `SetReadDeadline/SetWriteDeadline`
  - frame length cap (hard stop)
  - connection cap via semaphore

### Logging
Structured logs (JSON preferred) with fields:
- conn_id
- remote_addr
- client_id
- session_id
- msg_type
- cmd
- seq
- status/error

### Error Handling
- never panic on untrusted input
- return explicit error responses for protocol errors
- close connection on auth failure or repeated invalid frames

## Service Operation (systemd-friendly)

### c2-server systemd expectations
- runs as dedicated non-root user
- reads config from `/etc/c2/config.toml`
- logs to journald (stdout/stderr)
- `Restart=on-failure`
- graceful shutdown on SIGTERM within timeout

### GateRelay integration
- GateRelay listens on DMZ VM and forwards to c2-server private IP/port
- C2 server does NOT know about GateRelay; it just sees TCP peers (source will be relay)
- Log both:
  - GateRelay connection events
  - c2-server auth and command events
- Document firewall rules:
  - DMZ: allow inbound relay port
  - internal: allow inbound C2 port only from DMZ VM

## Acceptance Tests (must pass)

### Protocol Tests
1. Valid auth + PING returns PONG
2. Invalid signature rejected
3. Replay same auth nonces rejected
4. Oversized frame rejected
5. Old seq rejected

### Ops / Reliability Tests
1. Kill -9 server -> systemd restarts -> client reconnects
2. Stop target service -> GateRelay remains up; connection attempts fail cleanly; logs show failure
3. Simulate packet loss/timeout (using `tc netem`) -> timeouts occur and are logged

## Deliverables
- `c2-server` source + README
- `c2-client` source + README
- Protocol document (this file evolves into it)
- Example configs
- systemd unit files (server) + deployment steps
- Test plan + evidence logs (snippets)

## Questions to Resolve (make best assumptions if unsure)
- Choose JSON vs MessagePack (default JSON)
- Choose command set (default: PING/ECHO/TIME/STATUS)
- Choose PSK distribution (default: config file for lab)
- Decide whether to add TLS (out of scope for v1; can be v2)

## What I Want From Claude
1. Propose the simplest secure protocol implementation matching the above (Go preferred).
2. Generate a minimal project structure (`cmd/c2-server`, `cmd/c2-client`, `internal/proto`, etc.).
3. Provide runnable code for:
   - framed transport
   - handshake/auth
   - command execution (allowlist)
   - structured logging
4. Provide a small test suite (unit tests) for framing + auth + replay + seq checks.
5. Provide a deployment checklist for the two-VM topology using GateRelay.
