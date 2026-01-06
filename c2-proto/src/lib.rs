pub mod messages;
pub mod framing;
pub mod crypto;

pub use messages::*;
pub use framing::*;
pub use crypto::*;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Frame too large: {0} bytes (max: {1})")]
    FrameTooLarge(u32, u32),

    #[error("Invalid message type: {0}")]
    InvalidMessageType(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Replay attack detected")]
    ReplayDetected,

    #[error("Sequence number violation: expected > {0}, got {1}")]
    SequenceViolation(u64, u64),

    #[error("Timestamp out of bounds: {0}")]
    TimestampOutOfBounds(i64),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Session not found: {0}")]
    SessionNotFound(String),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;
