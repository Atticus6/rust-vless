use crate::registry::UserRegistry;
use std::fmt::Display;
use std::net::{Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use uuid::Uuid;

/// 本服务实现的 VLESS 协议版本。
pub const VLESS_VERSION: u8 = 0;

/// WebSocket 单条二进制消息的最大长度。
pub const MAX_WS_MESSAGE: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRequest {
    pub user_id: Uuid,
    pub target: String,
    pub command: Command,
    pub payload: Vec<u8>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("data too short: {0} bytes")]
    TooShort(usize),
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u8),
    #[error("invalid uuid: {0}")]
    InvalidUuid(String),
    #[error("unknown user")]
    Unauthorized,
    #[error("unsupported command: {0}")]
    UnsupportedCommand(u8),
    #[error("unsupported address type: {0}")]
    UnsupportedAddressType(u8),
    #[error("invalid address: {0}")]
    InvalidAddress(String),
}

/// 解析一个 VLESS 请求：
///
/// 字节布局：
/// `| version(1) | uuid(16) | addon_len(1) | addon | command(1) | port(2BE) |`
/// `atyp(1) | address | payload |`
///
/// UUID 必须存在于 `users`；目标地址以 `host:port` 返回（IPv6 会加方括号）。
pub fn parse_request(data: &[u8], users: &UserRegistry) -> Result<ParsedRequest, ParseError> {
    if data.len() < 22 {
        return Err(ParseError::TooShort(data.len()));
    }
    if data[0] != VLESS_VERSION {
        return Err(ParseError::UnsupportedVersion(data[0]));
    }

    let user_id =
        Uuid::from_slice(&data[1..17]).map_err(|e| ParseError::InvalidUuid(e.to_string()))?;
    if !users.valid(user_id) {
        return Err(ParseError::Unauthorized);
    }

    let addon_len = data[17] as usize;
    let mut offset = 18usize
        .checked_add(addon_len)
        .ok_or_else(|| ParseError::InvalidAddress("addon length overflow".into()))?;
    if data.len() < offset + 4 {
        return Err(ParseError::TooShort(data.len()));
    }

    let command = match data[offset] {
        1 => Command::Tcp,
        2 => Command::Udp,
        other => return Err(ParseError::UnsupportedCommand(other)),
    };
    offset += 1;

    let port = u16::from_be_bytes([data[offset], data[offset + 1]]);
    offset += 2;

    let atyp = data[offset];
    offset += 1;

    let host = match atyp {
        1 => {
            if data.len() < offset + 4 {
                return Err(ParseError::TooShort(data.len()));
            }
            let ip = Ipv4Addr::from([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;
            ip.to_string()
        }
        2 => {
            if data.len() < offset + 1 {
                return Err(ParseError::TooShort(data.len()));
            }
            let len = data[offset] as usize;
            offset += 1;
            if data.len() < offset + len {
                return Err(ParseError::TooShort(data.len()));
            }
            let host = std::str::from_utf8(&data[offset..offset + len])
                .map_err(|e| ParseError::InvalidAddress(e.to_string()))?;
            if host.is_empty() {
                return Err(ParseError::InvalidAddress("empty domain".into()));
            }
            offset += len;
            host.to_string()
        }
        3 => {
            if data.len() < offset + 16 {
                return Err(ParseError::TooShort(data.len()));
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&data[offset..offset + 16]);
            offset += 16;
            Ipv6Addr::from(octets).to_string()
        }
        other => return Err(ParseError::UnsupportedAddressType(other)),
    };

    Ok(ParsedRequest {
        user_id,
        target: join_host_port(host, port),
        command,
        payload: data[offset..].to_vec(),
    })
}

fn join_host_port(host: impl Display, port: u16) -> String {
    let host = host.to_string();
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER: Uuid = Uuid::from_u128(0x11111111111111111111111111111111);

    fn registry() -> UserRegistry {
        let reg = UserRegistry::new();
        reg.replace_all([USER]);
        reg
    }

    fn header(cmd: u8, port: u16, atyp: u8, addr: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(VLESS_VERSION);
        out.extend_from_slice(USER.as_bytes());
        out.push(0); // addon 长度
        out.push(cmd);
        out.extend_from_slice(&port.to_be_bytes());
        out.push(atyp);
        out.extend_from_slice(addr);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn parses_ipv4_tcp() {
        let data = header(1, 443, 1, &[127, 0, 0, 1], b"ping");
        let req = parse_request(&data, &registry()).unwrap();
        assert_eq!(req.target, "127.0.0.1:443");
        assert_eq!(req.command, Command::Tcp);
        assert_eq!(req.payload, b"ping");
    }

    #[test]
    fn parses_domain_udp() {
        let domain = b"example.com";
        let mut addr = vec![domain.len() as u8];
        addr.extend_from_slice(domain);
        let data = header(2, 53, 2, &addr, b"");
        let req = parse_request(&data, &registry()).unwrap();
        assert_eq!(req.target, "example.com:53");
        assert_eq!(req.command, Command::Udp);
        assert!(req.payload.is_empty());
    }

    #[test]
    fn parses_ipv6() {
        let addr = Ipv6Addr::LOCALHOST.octets();
        let data = header(1, 8080, 3, &addr, b"");
        let req = parse_request(&data, &registry()).unwrap();
        assert_eq!(req.target, "[::1]:8080");
    }

    #[test]
    fn rejects_unknown_user() {
        let mut data = header(1, 80, 1, &[1, 2, 3, 4], b"");
        data[1] ^= 0xff;
        assert_eq!(
            parse_request(&data, &registry()),
            Err(ParseError::Unauthorized)
        );
    }

    #[test]
    fn rejects_bad_version_and_command() {
        let mut data = header(1, 80, 1, &[1, 2, 3, 4], b"");
        data[0] = 1;
        assert_eq!(
            parse_request(&data, &registry()),
            Err(ParseError::UnsupportedVersion(1))
        );

        let data = header(9, 80, 1, &[1, 2, 3, 4], b"");
        assert_eq!(
            parse_request(&data, &registry()),
            Err(ParseError::UnsupportedCommand(9))
        );
    }

    #[test]
    fn rejects_truncated_header() {
        let data = header(1, 80, 1, &[1, 2, 3], b"");
        assert!(matches!(
            parse_request(&data, &registry()),
            Err(ParseError::TooShort(_))
        ));
    }
}
