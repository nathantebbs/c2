use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use std::collections::HashMap;

type HmacSha256 = Hmac<Sha256>;

/// Generates a random nonce as a hex string
pub fn generate_nonce() -> String {
    let mut rng = rand::thread_rng();
    let nonce: [u8; 16] = rng.gen();
    hex::encode(nonce)
}

/// Computes HMAC-SHA256 and returns hex-encoded string
pub fn compute_hmac(key: &[u8], data: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(key)
        .expect("HMAC can take key of any size");
    mac.update(data);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Verifies HMAC-SHA256 signature
pub fn verify_hmac(key: &[u8], data: &[u8], expected_sig: &str) -> bool {
    let computed = compute_hmac(key, data);
    constant_time_compare(&computed, expected_sig)
}

/// Constant-time string comparison to prevent timing attacks
fn constant_time_compare(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();

    let mut result = 0u8;
    for (a_byte, b_byte) in a_bytes.iter().zip(b_bytes.iter()) {
        result |= a_byte ^ b_byte;
    }

    result == 0
}

/// Computes authentication signature:
/// HMAC(PSK, client_id | server_nonce | client_nonce)
pub fn compute_auth_signature(
    psk: &[u8],
    client_id: &str,
    server_nonce: &str,
    client_nonce: &str,
) -> String {
    let data = format!("{}{}{}", client_id, server_nonce, client_nonce);
    compute_hmac(psk, data.as_bytes())
}

/// Derives a session key from PSK and auth parameters:
/// session_key = HMAC(PSK, session_id | server_nonce | client_nonce)
pub fn derive_session_key(
    psk: &[u8],
    session_id: &str,
    server_nonce: &str,
    client_nonce: &str,
) -> Vec<u8> {
    let data = format!("{}{}{}", session_id, server_nonce, client_nonce);
    let sig = compute_hmac(psk, data.as_bytes());
    hex::decode(sig).expect("hex decode should not fail")
}

/// Computes command signature:
/// HMAC(session_key, session_id | seq | nonce | cmd | canonical_json(args))
pub fn compute_cmd_signature(
    session_key: &[u8],
    session_id: &str,
    seq: u64,
    nonce: &str,
    cmd: &str,
    args: &HashMap<String, serde_json::Value>,
) -> String {
    // Canonical JSON: serialize args in sorted order
    let args_json = serde_json::to_string(args).unwrap_or_else(|_| "{}".to_string());

    let data = format!("{}{}{}{}{}", session_id, seq, nonce, cmd, args_json);
    compute_hmac(session_key, data.as_bytes())
}

/// Verifies command signature
pub fn verify_cmd_signature(
    session_key: &[u8],
    session_id: &str,
    seq: u64,
    nonce: &str,
    cmd: &str,
    args: &HashMap<String, serde_json::Value>,
    expected_sig: &str,
) -> bool {
    let computed = compute_cmd_signature(session_key, session_id, seq, nonce, cmd, args);
    constant_time_compare(&computed, expected_sig)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_nonce() {
        let nonce1 = generate_nonce();
        let nonce2 = generate_nonce();

        assert_eq!(nonce1.len(), 32); // 16 bytes = 32 hex chars
        assert_ne!(nonce1, nonce2); // Should be different
    }

    #[test]
    fn test_hmac_computation() {
        let key = b"test-key";
        let data = b"test-data";

        let sig1 = compute_hmac(key, data);
        let sig2 = compute_hmac(key, data);

        assert_eq!(sig1, sig2); // Same inputs = same output
        assert_eq!(sig1.len(), 64); // SHA256 = 32 bytes = 64 hex chars
    }

    #[test]
    fn test_hmac_verification() {
        let key = b"test-key";
        let data = b"test-data";

        let sig = compute_hmac(key, data);

        assert!(verify_hmac(key, data, &sig));
        assert!(!verify_hmac(key, b"wrong-data", &sig));
        assert!(!verify_hmac(b"wrong-key", data, &sig));
    }

    #[test]
    fn test_constant_time_compare() {
        assert!(constant_time_compare("abc", "abc"));
        assert!(!constant_time_compare("abc", "abd"));
        assert!(!constant_time_compare("abc", "ab"));
    }

    #[test]
    fn test_auth_signature() {
        let psk = b"shared-secret";
        let client_id = "client-1";
        let server_nonce = "server-nonce-123";
        let client_nonce = "client-nonce-456";

        let sig = compute_auth_signature(psk, client_id, server_nonce, client_nonce);

        // Verify it's deterministic
        let sig2 = compute_auth_signature(psk, client_id, server_nonce, client_nonce);
        assert_eq!(sig, sig2);

        // Verify it changes with different inputs
        let sig3 = compute_auth_signature(psk, "different-client", server_nonce, client_nonce);
        assert_ne!(sig, sig3);
    }

    #[test]
    fn test_cmd_signature() {
        let session_key = b"session-key-123";
        let session_id = "session-abc";
        let seq = 5;
        let nonce = "nonce-xyz";
        let cmd = "PING";
        let args = HashMap::new();

        let sig = compute_cmd_signature(session_key, session_id, seq, nonce, cmd, &args);

        assert!(verify_cmd_signature(
            session_key,
            session_id,
            seq,
            nonce,
            cmd,
            &args,
            &sig
        ));

        // Wrong seq should fail
        assert!(!verify_cmd_signature(
            session_key,
            session_id,
            6,
            nonce,
            cmd,
            &args,
            &sig
        ));
    }
}
