use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// OBS WebSocket v5 authentication secret.
///
/// `base64(sha256(base64(sha256(password + salt)) + challenge))`
pub fn authentication_string(password: &str, salt: &str, challenge: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    let secret = STANDARD.encode(hasher.finalize_reset());
    hasher.update(secret.as_bytes());
    hasher.update(challenge.as_bytes());
    STANDARD.encode(hasher.finalize())
}
