use c2_proto::{crypto, framing, Message, MessagePayload, ProtocolError, Result};
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};

pub struct C2Client {
    client_id: String,
    psk: Vec<u8>,
    session_id: Option<String>,
    session_key: Option<Vec<u8>>,
    seq: u64,
    max_frame_size: u32,
    read_timeout: Duration,
    write_timeout: Duration,
}

impl C2Client {
    pub fn new(
        client_id: String,
        psk: Vec<u8>,
        read_timeout_secs: u64,
        write_timeout_secs: u64,
    ) -> Self {
        Self {
            client_id,
            psk,
            session_id: None,
            session_key: None,
            seq: 0,
            max_frame_size: c2_proto::DEFAULT_MAX_FRAME_SIZE,
            read_timeout: Duration::from_secs(read_timeout_secs),
            write_timeout: Duration::from_secs(write_timeout_secs),
        }
    }

    /// Authenticates with the server
    pub async fn authenticate<S>(&mut self, stream: &mut S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        info!("Starting authentication as client: {}", self.client_id);

        // Step 1: Send HELLO
        let hello = Message::hello(self.client_id.clone());
        self.write_message(stream, &hello).await?;
        debug!("Sent HELLO");

        // Step 2: Receive CHALLENGE
        let challenge_msg = self.read_message(stream).await?;

        if challenge_msg.msg_type != "challenge" {
            return Err(ProtocolError::InvalidMessageType(challenge_msg.msg_type));
        }

        let server_nonce = match &challenge_msg.payload {
            MessagePayload::Challenge(payload) => &payload.server_nonce,
            _ => return Err(ProtocolError::InvalidMessageType("expected challenge".to_string())),
        };

        debug!("Received CHALLENGE with server nonce: {}", server_nonce);

        // Step 3: Send AUTH
        let client_nonce = crypto::generate_nonce();
        let sig = crypto::compute_auth_signature(
            &self.psk,
            &self.client_id,
            server_nonce,
            &client_nonce,
        );

        let auth = Message::auth(
            self.client_id.clone(),
            server_nonce.clone(),
            client_nonce.clone(),
            sig,
        );

        self.write_message(stream, &auth).await?;
        debug!("Sent AUTH");

        // Step 4: Receive AUTH_OK
        let auth_ok_msg = self.read_message(stream).await?;

        if auth_ok_msg.msg_type != "auth_ok" {
            return Err(ProtocolError::InvalidMessageType(auth_ok_msg.msg_type));
        }

        let session_id = auth_ok_msg
            .session_id
            .ok_or_else(|| ProtocolError::AuthFailed("no session_id in auth_ok".to_string()))?;

        // Derive session key
        let session_key = crypto::derive_session_key(
            &self.psk,
            &session_id,
            server_nonce,
            &client_nonce,
        );

        self.session_id = Some(session_id.clone());
        self.session_key = Some(session_key);
        self.seq = 0;

        info!("Authentication successful! Session ID: {}", session_id);

        Ok(())
    }

    /// Sends a command and waits for response
    pub async fn send_command<S>(
        &mut self,
        stream: &mut S,
        cmd: &str,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let session_id = self.session_id.as_ref()
            .ok_or_else(|| ProtocolError::AuthFailed("not authenticated".to_string()))?;

        let session_key = self.session_key.as_ref()
            .ok_or_else(|| ProtocolError::AuthFailed("no session key".to_string()))?;

        // Increment sequence number
        self.seq += 1;
        let seq = self.seq;

        // Generate nonce for this command
        let nonce = crypto::generate_nonce();

        // Compute signature
        let sig = crypto::compute_cmd_signature(
            session_key,
            session_id,
            seq,
            &nonce,
            cmd,
            &args,
        );

        // Build command message
        let cmd_msg = Message::cmd(
            session_id.clone(),
            seq,
            nonce,
            cmd.to_string(),
            args,
            sig,
        );

        debug!("Sending command: {} (seq: {})", cmd, seq);

        // Send command
        self.write_message(stream, &cmd_msg).await?;

        // Receive response
        let resp_msg = self.read_message(stream).await?;

        if resp_msg.msg_type != "resp" {
            return Err(ProtocolError::InvalidMessageType(resp_msg.msg_type));
        }

        let resp_payload = match &resp_msg.payload {
            MessagePayload::Resp(payload) => payload,
            _ => return Err(ProtocolError::InvalidMessageType("expected resp".to_string())),
        };

        // Check if response matches our sequence number
        if resp_msg.seq != Some(seq) {
            warn!("Response seq mismatch: expected {}, got {:?}", seq, resp_msg.seq);
        }

        if resp_payload.status == "error" {
            let error = resp_payload.error.as_ref()
                .map(|s| s.as_str())
                .unwrap_or("unknown error");
            return Err(ProtocolError::AuthFailed(error.to_string()));
        }

        Ok(resp_payload.result.clone().unwrap_or(serde_json::Value::Null))
    }

    /// Sends a ping and waits for pong
    pub async fn ping<S>(&self, stream: &mut S) -> Result<()>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let ping = Message::ping();
        self.write_message(stream, &ping).await?;

        let pong = self.read_message(stream).await?;

        if pong.msg_type != "pong" {
            return Err(ProtocolError::InvalidMessageType(pong.msg_type));
        }

        debug!("PING -> PONG");
        Ok(())
    }

    async fn read_message<S>(&self, stream: &mut S) -> Result<Message>
    where
        S: AsyncRead + Unpin,
    {
        timeout(
            self.read_timeout,
            framing::read_frame(stream, self.max_frame_size),
        )
        .await
        .map_err(|_| {
            ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "read timeout",
            ))
        })?
    }

    async fn write_message<S>(&self, stream: &mut S, msg: &Message) -> Result<()>
    where
        S: AsyncWrite + Unpin,
    {
        timeout(self.write_timeout, framing::write_frame(stream, msg))
            .await
            .map_err(|_| {
                ProtocolError::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "write timeout",
                ))
            })?
    }
}
