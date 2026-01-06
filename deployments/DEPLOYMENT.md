# C2 Simulator - Deployment Guide

This guide covers deploying the C2 simulator in a controlled lab environment with the recommended 3-VM topology.

## Architecture Overview

```
+-------------+         +--------------+         +--------------+
| client-vm   |-------->|  relay-vm    |-------->|   c2-vm      |
|             |  public |  (GateRelay) | private |  (c2-server) |
| c2-client   |   :4000 |  DMZ         |  :5000  |  internal    |
+-------------+         +--------------+         +--------------+
```

### VM Roles

1. **relay-vm (DMZ)**: Runs GateRelay as a TCP relay/ingress layer
2. **c2-vm (internal)**: Runs c2-server on private network
3. **client-vm**: Runs c2-client for testing

## Prerequisites

- Rust toolchain (1.70+)
- systemd-based Linux distribution
- Network connectivity between VMs
- GateRelay binary (separate project)

## Building

### On Development Machine

```bash
# Clone and build the project
cd /path/to/c2
cargo build --release

# Binaries will be in target/release/
ls -lh target/release/c2-server
ls -lh target/release/c2-client
```

## Deployment Steps

### 1. Deploy c2-server (c2-vm)

#### Create dedicated user

```bash
sudo useradd -r -s /bin/false c2server
```

#### Install binary and config

```bash
# Create directories
sudo mkdir -p /opt/c2/bin
sudo mkdir -p /etc/c2
sudo mkdir -p /var/log/c2

# Copy binary
sudo cp target/release/c2-server /opt/c2/bin/
sudo chmod +x /opt/c2/bin/c2-server

# Copy config
sudo cp configs/server.toml /etc/c2/config.toml

# Edit config and change PSK!
sudo nano /etc/c2/config.toml
```

#### Generate a strong PSK

```bash
# Generate 32-byte random PSK (64 hex characters)
openssl rand -hex 32
# Example output: 0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
```

Update `/etc/c2/config.toml`:

```toml
[server]
listen_addr = "10.0.0.30"  # Private IP of c2-vm
listen_port = 5000

[security]
psk = "YOUR_GENERATED_PSK_HERE"  # Use the value from openssl rand -hex 32
allowed_client_ids = []  # Optional: restrict clients

[limits]
max_conns = 100
max_frame_bytes = 1048576
read_timeout_secs = 30
write_timeout_secs = 30
auth_timeout_secs = 60

[logging]
log_level = "info"
json_logs = true  # Recommended for production
```

#### Set permissions

```bash
sudo chown -R c2server:c2server /opt/c2
sudo chown -R c2server:c2server /var/log/c2
sudo chmod 600 /etc/c2/config.toml  # Protect PSK
```

#### Install systemd service

```bash
sudo cp deployments/c2-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable c2-server
sudo systemctl start c2-server
```

#### Verify service is running

```bash
sudo systemctl status c2-server
sudo journalctl -u c2-server -f
```

#### Configure firewall (c2-vm)

```bash
# Only allow connections from relay-vm (example: relay-vm is 10.0.0.20)
sudo ufw allow from 10.0.0.20 to any port 5000 proto tcp
sudo ufw enable
```

### 2. Deploy GateRelay (relay-vm)

Assuming GateRelay is a separate project:

```bash
# Install GateRelay binary
sudo cp /path/to/gaterelay /opt/gaterelay/bin/

# Create GateRelay config
sudo mkdir -p /etc/gaterelay

# Example GateRelay config (adjust based on actual GateRelay)
cat <<EOF | sudo tee /etc/gaterelay/config.toml
listen_addr = "0.0.0.0"
listen_port = 4000
forward_addr = "10.0.0.30"  # c2-vm private IP
forward_port = 5000
EOF

# Create systemd service for GateRelay
# (similar to c2-server.service)

sudo systemctl enable gaterelay
sudo systemctl start gaterelay
```

#### Configure firewall (relay-vm)

```bash
# Allow inbound on relay port from anywhere (or restrict to known IPs)
sudo ufw allow 4000/tcp

# Allow forwarding to c2-vm
sudo ufw allow out to 10.0.0.30 port 5000 proto tcp

sudo ufw enable
```

### 3. Deploy c2-client (client-vm)

#### Install binary

```bash
mkdir -p ~/c2-client
cp target/release/c2-client ~/c2-client/
cp configs/client.toml ~/c2-client/
```

#### Configure client

Edit `~/c2-client/client.toml`:

```toml
[client]
client_id = "client-1"
relay_addr = "RELAY_VM_PUBLIC_IP"  # Public IP of relay-vm
relay_port = 4000

[security]
psk = "SAME_PSK_AS_SERVER"  # Must match server's PSK

[timeouts]
connect_timeout_secs = 10
read_timeout_secs = 30
write_timeout_secs = 30
```

#### Run client

```bash
cd ~/c2-client
./c2-client
```

## Testing

### Basic Connectivity Test

From client-vm:

```bash
./c2-client
# Once connected, try:
> PING
> TIME
> STATUS
> ECHO hello world
> quit
```

### Protocol Tests

Test the acceptance criteria from the spec:

1. **Valid auth + PING**:
   ```
   > PING
   # Should receive PONG
   ```

2. **Invalid signature** (modify PSK in client config, restart client):
   - Should be rejected with auth error

3. **Replay attack**:
   - The protocol prevents this automatically (nonce tracking)
   - Check server logs for "Replay attack detected" messages

4. **Oversized frame**:
   - Server has max_frame_bytes limit configured
   - Attempts to send frames larger than limit are rejected

5. **Sequence number validation**:
   - Server tracks sequence numbers per session
   - Out-of-order commands are rejected

### Reliability Tests

1. **Server restart**:
   ```bash
   sudo systemctl restart c2-server
   # Client should reconnect and re-authenticate
   ```

2. **Network disruption**:
   ```bash
   # Simulate packet loss on relay-vm
   sudo tc qdisc add dev eth0 root netem loss 20%

   # Cleanup
   sudo tc qdisc del dev eth0 root
   ```

3. **Connection limits**:
   - Configure `max_conns = 5` in server config
   - Start 6+ clients simultaneously
   - 6th client should be rejected

## Monitoring

### View server logs

```bash
# Real-time logs
sudo journalctl -u c2-server -f

# JSON logs (if json_logs = true)
sudo journalctl -u c2-server -o json-pretty

# Filter by error level
sudo journalctl -u c2-server | grep ERROR
```

### Check connections

```bash
# On c2-vm
sudo ss -tnp | grep c2-server

# On relay-vm
sudo ss -tnp | grep gaterelay
```

## Firewall Rules Summary

### relay-vm (DMZ)

```bash
# Inbound: allow relay port
sudo ufw allow 4000/tcp

# Outbound: allow to c2-vm
sudo ufw allow out to 10.0.0.30 port 5000 proto tcp

# SSH (restricted)
sudo ufw allow from ADMIN_IP to any port 22
```

### c2-vm (Internal)

```bash
# Inbound: only from relay-vm
sudo ufw allow from 10.0.0.20 to any port 5000 proto tcp

# SSH (restricted)
sudo ufw allow from ADMIN_IP to any port 22

# Deny all other inbound
sudo ufw default deny incoming
```

## Troubleshooting

### Client can't connect

```bash
# Check relay is listening
telnet RELAY_IP 4000

# Check c2-server is listening
# (from relay-vm)
telnet 10.0.0.30 5000
```

### Authentication fails

- Verify PSK matches exactly on client and server
- Check server logs: `sudo journalctl -u c2-server | grep -i auth`
- Ensure timestamp skew is within tolerance (check system clocks)

### Connection timeouts

- Check firewall rules on both VMs
- Verify network connectivity: `ping`, `traceroute`
- Increase timeout values in configs

## Security Considerations

1. **PSK Management**:
   - Use strong random PSKs (32+ bytes)
   - Store PSK in protected config files (chmod 600)
   - Rotate PSKs periodically

2. **Network Isolation**:
   - Keep c2-server on private network
   - Only expose relay to public network
   - Use firewall rules to restrict access

3. **Logging**:
   - Enable JSON logs for structured monitoring
   - Centralize logs to SIEM if available
   - Monitor for auth failures and replay attempts

4. **Resource Limits**:
   - Configure appropriate max_conns
   - Set frame size limits
   - Use systemd resource controls

## Maintenance

### Update deployment

```bash
# Build new version
cargo build --release

# Stop service
sudo systemctl stop c2-server

# Backup old binary
sudo cp /opt/c2/bin/c2-server /opt/c2/bin/c2-server.old

# Install new binary
sudo cp target/release/c2-server /opt/c2/bin/

# Restart service
sudo systemctl start c2-server
```

### Rotate PSK

```bash
# Generate new PSK
NEW_PSK=$(openssl rand -hex 32)

# Update server config
sudo nano /etc/c2/config.toml  # Update PSK

# Restart server
sudo systemctl restart c2-server

# Update all client configs with new PSK
```

## Uninstall

```bash
# Stop and disable service
sudo systemctl stop c2-server
sudo systemctl disable c2-server

# Remove files
sudo rm /etc/systemd/system/c2-server.service
sudo rm -rf /opt/c2
sudo rm -rf /etc/c2
sudo rm -rf /var/log/c2

# Remove user
sudo userdel c2server

# Reload systemd
sudo systemctl daemon-reload
```
