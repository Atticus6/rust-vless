use rand::RngCore;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current Unix time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Cryptographically random lowercase hex token.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
