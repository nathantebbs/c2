use c2_proto::{crypto, ProtocolError, Result};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Tracks active sessions
#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    nonce_cache: Arc<RwLock<NonceCache>>,
    psk: Vec<u8>,
    allowed_clients: Option<HashSet<String>>,
    timestamp_skew: i64,
}

/// Represents an authenticated session
#[derive(Debug, Clone)]
pub struct Session {
    pub session_id: String,
    pub client_id: String,
    pub session_key: Vec<u8>,
    pub last_seq: u64,
    pub created_at: Instant,
}

/// Cache for tracking used nonces to prevent replay attacks
struct NonceCache {
    nonces: HashMap<String, Instant>,
    ttl: Duration,
}

impl SessionManager {
    pub fn new(
        psk: Vec<u8>,
        allowed_clients: Vec<String>,
        timestamp_skew_secs: i64,
        nonce_ttl_secs: u64,
    ) -> Self {
        let allowed = if allowed_clients.is_empty() {
            None
        } else {
            Some(allowed_clients.into_iter().collect())
        };

        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            nonce_cache: Arc::new(RwLock::new(NonceCache {
                nonces: HashMap::new(),
                ttl: Duration::from_secs(nonce_ttl_secs),
            })),
            psk,
            allowed_clients: allowed,
            timestamp_skew: timestamp_skew_secs,
        }
    }

    /// Validates timestamp is within acceptable skew
    pub fn validate_timestamp(&self, ts: i64) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let diff = (now - ts).abs();

        if diff > self.timestamp_skew {
            warn!("Timestamp out of bounds: {} (now: {}, diff: {})", ts, now, diff);
            return Err(ProtocolError::TimestampOutOfBounds(ts));
        }

        Ok(())
    }

    /// Validates that client_id is allowed (if allowlist is configured)
    pub fn validate_client_id(&self, client_id: &str) -> Result<()> {
        if let Some(ref allowed) = self.allowed_clients {
            if !allowed.contains(client_id) {
                warn!("Client ID not in allowlist: {}", client_id);
                return Err(ProtocolError::AuthFailed("client not allowed".to_string()));
            }
        }
        Ok(())
    }

    /// Validates authentication signature
    pub fn validate_auth(
        &self,
        client_id: &str,
        server_nonce: &str,
        client_nonce: &str,
        sig: &str,
    ) -> Result<()> {
        if !crypto::verify_hmac(&self.psk,
            format!("{}{}{}", client_id, server_nonce, client_nonce).as_bytes(),
            sig
        ) {
            warn!("Invalid authentication signature for client: {}", client_id);
            return Err(ProtocolError::InvalidSignature);
        }

        Ok(())
    }

    /// Checks and records a nonce to prevent replay attacks
    pub async fn check_and_record_nonce(&self, nonce_key: String) -> Result<()> {
        let mut cache = self.nonce_cache.write().await;

        // Clean up expired nonces
        cache.cleanup();

        // Check if nonce was already used
        if cache.nonces.contains_key(&nonce_key) {
            warn!("Replay attack detected: nonce already used: {}", nonce_key);
            return Err(ProtocolError::ReplayDetected);
        }

        // Record this nonce
        cache.nonces.insert(nonce_key, Instant::now());

        Ok(())
    }

    /// Creates a new authenticated session
    pub async fn create_session(
        &self,
        client_id: String,
        server_nonce: &str,
        client_nonce: &str,
    ) -> Session {
        let session_id = crypto::generate_nonce();
        let session_key = crypto::derive_session_key(
            &self.psk,
            &session_id,
            server_nonce,
            client_nonce,
        );

        let session = Session {
            session_id: session_id.clone(),
            client_id: client_id.clone(),
            session_key,
            last_seq: 0,
            created_at: Instant::now(),
        };

        debug!("Created session {} for client {}", session_id, client_id);

        let mut sessions = self.sessions.write().await;
        sessions.insert(session_id.clone(), session.clone());

        session
    }

    /// Retrieves a session by ID
    pub async fn get_session(&self, session_id: &str) -> Result<Session> {
        let sessions = self.sessions.read().await;
        sessions
            .get(session_id)
            .cloned()
            .ok_or_else(|| ProtocolError::SessionNotFound(session_id.to_string()))
    }

    /// Validates and updates sequence number for a session
    pub async fn validate_and_update_seq(&self, session_id: &str, seq: u64) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| ProtocolError::SessionNotFound(session_id.to_string()))?;

        if seq <= session.last_seq {
            warn!(
                "Sequence violation for session {}: expected > {}, got {}",
                session_id, session.last_seq, seq
            );
            return Err(ProtocolError::SequenceViolation(session.last_seq, seq));
        }

        session.last_seq = seq;
        debug!("Updated sequence for session {} to {}", session_id, seq);

        Ok(())
    }

    /// Removes a session
    pub async fn remove_session(&self, session_id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(session_id);
        debug!("Removed session {}", session_id);
    }
}

impl NonceCache {
    fn cleanup(&mut self) {
        let now = Instant::now();
        self.nonces.retain(|_, created| {
            now.duration_since(*created) < self.ttl
        });
    }
}
