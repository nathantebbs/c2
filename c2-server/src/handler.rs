use crate::commands::{execute_command, CommandContext};
use crate::session::SessionManager;
use c2_proto::{crypto, framing, Message, MessagePayload, ProtocolError};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

pub struct ConnectionHandler {
    session_manager: SessionManager,
    command_context: Arc<CommandContext>,
    max_frame_size: u32,
    read_timeout: Duration,
    write_timeout: Duration,
    auth_timeout: Duration,
}

impl ConnectionHandler {
    pub fn new(
        session_manager: SessionManager,
        command_context: Arc<CommandContext>,
        max_frame_size: u32,
        read_timeout_secs: u64,
        write_timeout_secs: u64,
        auth_timeout_secs: u64,
    ) -> Self {
        Self {
            session_manager,
            command_context,
            max_frame_size,
            read_timeout: Duration::from_secs(read_timeout_secs),
            write_timeout: Duration::from_secs(write_timeout_secs),
            auth_timeout: Duration::from_secs(auth_timeout_secs),
        }
    }

    /// Handles a client connection
    pub async fn handle<S>(&self, mut stream: S, remote_addr: String)
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let conn_id = crypto::generate_nonce();
        info!("Connection {} from {}", conn_id, remote_addr);

        // Perform authentication with timeout
        let session = match timeout(
            self.auth_timeout,
            self.authenticate(&mut stream, &conn_id, &remote_addr),
        )
        .await
        {
            Ok(Ok(session)) => session,
            Ok(Err(e)) => {
                error!("Authentication failed for conn {}: {}", conn_id, e);
                let _ = self
                    .send_error(&mut stream, format!("Authentication failed: {}", e))
                    .await;
                return;
            }
            Err(_) => {
                error!("Authentication timeout for conn {}", conn_id);
                let _ = self
                    .send_error(&mut stream, "Authentication timeout".to_string())
                    .await;
                return;
            }
        };

        info!(
            "Client {} authenticated as session {}",
            session.client_id, session.session_id
        );

        // Handle commands in this session
        if let Err(e) = self.handle_commands(&mut stream, &session.session_id, &conn_id).await {
            warn!("Command handling error for session {}: {}", session.session_id, e);
        }

        // Clean up session
        self.session_manager.remove_session(&session.session_id).await;
        info!("Connection {} closed", conn_id);
    }

    /// Performs the authentication handshake
    async fn authenticate<S>(
        &self,
        stream: &mut S,
        conn_id: &str,
        remote_addr: &str,
    ) -> Result<crate::session::Session, ProtocolError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // Step 1: Receive HELLO from client
        let hello_msg = self.read_message(stream).await?;

        if hello_msg.msg_type != "hello" {
            return Err(ProtocolError::InvalidMessageType(hello_msg.msg_type));
        }

        let client_id = match &hello_msg.payload {
            MessagePayload::Hello(payload) => &payload.client_id,
            _ => return Err(ProtocolError::InvalidMessageType("expected hello".to_string())),
        };

        debug!("Received HELLO from client: {}", client_id);

        // Validate client ID
        self.session_manager.validate_client_id(client_id)?;

        // Step 2: Send CHALLENGE with server_nonce
        let server_nonce = crypto::generate_nonce();
        let challenge_msg = Message::challenge(server_nonce.clone());
        self.write_message(stream, &challenge_msg).await?;

        debug!("Sent CHALLENGE with nonce: {}", server_nonce);

        // Step 3: Receive AUTH from client
        let auth_msg = self.read_message(stream).await?;

        if auth_msg.msg_type != "auth" {
            return Err(ProtocolError::InvalidMessageType(auth_msg.msg_type));
        }

        self.session_manager.validate_timestamp(auth_msg.ts)?;

        let (auth_client_id, auth_server_nonce, client_nonce, sig) = match &auth_msg.payload {
            MessagePayload::Auth(payload) => (
                &payload.client_id,
                &payload.server_nonce,
                &payload.client_nonce,
                &payload.sig,
            ),
            _ => return Err(ProtocolError::InvalidMessageType("expected auth".to_string())),
        };

        // Validate auth parameters match
        if auth_client_id != client_id {
            return Err(ProtocolError::AuthFailed("client_id mismatch".to_string()));
        }

        if auth_server_nonce != &server_nonce {
            return Err(ProtocolError::AuthFailed("server_nonce mismatch".to_string()));
        }

        // Check for replay (nonce reuse)
        let nonce_key = format!("{}:{}:{}", client_id, server_nonce, client_nonce);
        self.session_manager
            .check_and_record_nonce(nonce_key)
            .await?;

        // Validate signature
        self.session_manager
            .validate_auth(client_id, &server_nonce, client_nonce, sig)?;

        // Step 4: Create session and send AUTH_OK
        let session = self
            .session_manager
            .create_session(client_id.clone(), &server_nonce, client_nonce)
            .await;

        let auth_ok_msg = Message::auth_ok(session.session_id.clone());
        self.write_message(stream, &auth_ok_msg).await?;

        info!(
            "Client {} authenticated successfully (conn: {}, addr: {})",
            client_id, conn_id, remote_addr
        );

        Ok(session)
    }

    /// Handles command messages in an authenticated session
    async fn handle_commands<S>(
        &self,
        stream: &mut S,
        session_id: &str,
        _conn_id: &str,
    ) -> Result<(), ProtocolError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        loop {
            let msg = match self.read_message(stream).await {
                Ok(msg) => msg,
                Err(e) => {
                    if matches!(e, ProtocolError::Io(_)) {
                        debug!("Connection closed for session {}", session_id);
                        return Ok(());
                    }
                    return Err(e);
                }
            };

            match msg.msg_type.as_str() {
                "cmd" => {
                    if let Err(e) = self.handle_cmd(stream, session_id, &msg).await {
                        error!("Command error for session {}: {}", session_id, e);
                        let resp = Message::resp_error(
                            session_id.to_string(),
                            msg.seq.unwrap_or(0),
                            e.to_string(),
                        );
                        self.write_message(stream, &resp).await?;
                    }
                }
                "ping" => {
                    debug!("Received PING from session {}", session_id);
                    let pong = Message::pong();
                    self.write_message(stream, &pong).await?;
                }
                _ => {
                    warn!("Unknown message type: {} from session {}", msg.msg_type, session_id);
                }
            }
        }
    }

    /// Handles a CMD message
    async fn handle_cmd<S>(
        &self,
        stream: &mut S,
        session_id: &str,
        msg: &Message,
    ) -> Result<(), ProtocolError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        // Validate message structure
        let (cmd, args, sig) = match &msg.payload {
            MessagePayload::Cmd(payload) => (&payload.cmd, &payload.args, &payload.sig),
            _ => return Err(ProtocolError::InvalidMessageType("expected cmd".to_string())),
        };

        let seq = msg.seq.ok_or_else(|| {
            ProtocolError::InvalidMessageType("cmd missing seq".to_string())
        })?;

        let nonce = msg.nonce.as_ref().ok_or_else(|| {
            ProtocolError::InvalidMessageType("cmd missing nonce".to_string())
        })?;

        // Validate timestamp
        self.session_manager.validate_timestamp(msg.ts)?;

        // Validate and update sequence number
        self.session_manager
            .validate_and_update_seq(session_id, seq)
            .await?;

        // Get session and verify signature
        let session = self.session_manager.get_session(session_id).await?;

        if !crypto::verify_cmd_signature(
            &session.session_key,
            session_id,
            seq,
            nonce,
            cmd,
            args,
            sig,
        ) {
            return Err(ProtocolError::InvalidSignature);
        }

        debug!("Executing command {} for session {}", cmd, session_id);

        // Execute command
        let result = execute_command(cmd, args, &self.command_context);

        let resp = match result {
            Ok(value) => Message::resp(session_id.to_string(), seq, "success".to_string(), Some(value)),
            Err(e) => Message::resp_error(session_id.to_string(), seq, e),
        };

        self.write_message(stream, &resp).await?;

        Ok(())
    }

    async fn read_message<S>(&self, stream: &mut S) -> Result<Message, ProtocolError>
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

    async fn write_message<S>(&self, stream: &mut S, msg: &Message) -> Result<(), ProtocolError>
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

    async fn send_error<S>(&self, stream: &mut S, error: String) -> Result<(), ProtocolError>
    where
        S: AsyncWrite + Unpin,
    {
        let err_msg = Message::error(error, None);
        self.write_message(stream, &err_msg).await
    }
}
