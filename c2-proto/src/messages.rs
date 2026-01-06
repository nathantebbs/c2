use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// All messages include these base fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub ts: i64, // Unix timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    #[serde(flatten)]
    pub payload: MessagePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessagePayload {
    Hello(HelloPayload),
    Challenge(ChallengePayload),
    Auth(AuthPayload),
    AuthOk(AuthOkPayload),
    Cmd(CmdPayload),
    Resp(RespPayload),
    Ping(PingPayload),
    Pong(PongPayload),
    Err(ErrPayload),
}

/// Client sends Hello with client_id
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelloPayload {
    pub client_id: String,
}

/// Server responds with Challenge containing server_nonce
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengePayload {
    pub server_nonce: String,
}

/// Client sends Auth with signature
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthPayload {
    pub client_id: String,
    pub server_nonce: String,
    pub client_nonce: String,
    pub sig: String, // HMAC-SHA256 hex encoded
}

/// Server responds with AuthOk and session_id
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthOkPayload {
    pub message: String,
}

/// Command request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmdPayload {
    pub cmd: String,
    #[serde(default)]
    pub args: HashMap<String, serde_json::Value>,
    pub sig: String, // HMAC-SHA256 hex encoded
}

/// Command response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RespPayload {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Heartbeat ping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingPayload {}

/// Heartbeat pong
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongPayload {}

/// Error message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrPayload {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl Message {
    pub fn new(msg_type: &str, payload: MessagePayload) -> Self {
        Self {
            msg_type: msg_type.to_string(),
            ts: chrono::Utc::now().timestamp(),
            nonce: None,
            session_id: None,
            seq: None,
            payload,
        }
    }

    pub fn with_nonce(mut self, nonce: String) -> Self {
        self.nonce = Some(nonce);
        self
    }

    pub fn with_session(mut self, session_id: String, seq: u64) -> Self {
        self.session_id = Some(session_id);
        self.seq = Some(seq);
        self
    }

    pub fn hello(client_id: String) -> Self {
        Self::new("hello", MessagePayload::Hello(HelloPayload { client_id }))
    }

    pub fn challenge(server_nonce: String) -> Self {
        Self::new(
            "challenge",
            MessagePayload::Challenge(ChallengePayload { server_nonce }),
        )
    }

    pub fn auth(client_id: String, server_nonce: String, client_nonce: String, sig: String) -> Self {
        Self::new(
            "auth",
            MessagePayload::Auth(AuthPayload {
                client_id,
                server_nonce,
                client_nonce,
                sig,
            }),
        )
    }

    pub fn auth_ok(session_id: String) -> Self {
        Self::new(
            "auth_ok",
            MessagePayload::AuthOk(AuthOkPayload {
                message: "Authentication successful".to_string(),
            }),
        )
        .with_session(session_id, 0)
    }

    pub fn cmd(
        session_id: String,
        seq: u64,
        nonce: String,
        cmd: String,
        args: HashMap<String, serde_json::Value>,
        sig: String,
    ) -> Self {
        Self::new("cmd", MessagePayload::Cmd(CmdPayload { cmd, args, sig }))
            .with_session(session_id, seq)
            .with_nonce(nonce)
    }

    pub fn resp(session_id: String, seq: u64, status: String, result: Option<serde_json::Value>) -> Self {
        Self::new(
            "resp",
            MessagePayload::Resp(RespPayload {
                status,
                result,
                error: None,
            }),
        )
        .with_session(session_id, seq)
    }

    pub fn resp_error(session_id: String, seq: u64, error: String) -> Self {
        Self::new(
            "resp",
            MessagePayload::Resp(RespPayload {
                status: "error".to_string(),
                result: None,
                error: Some(error),
            }),
        )
        .with_session(session_id, seq)
    }

    pub fn ping() -> Self {
        Self::new("ping", MessagePayload::Ping(PingPayload {}))
    }

    pub fn pong() -> Self {
        Self::new("pong", MessagePayload::Pong(PongPayload {}))
    }

    pub fn error(error: String, code: Option<String>) -> Self {
        Self::new("err", MessagePayload::Err(ErrPayload { error, code }))
    }
}
