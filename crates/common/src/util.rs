use rand::RngCore;
use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 Unix 时间，单位毫秒。
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 生成指定字节数的密码学安全随机十六进制串（小写）。
///
/// 用于会话、节点、连线和订阅 token；字节数即熵的上限，调用方不要缩短。
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}
