//! 订阅生成。
//!
//! 把账号名下的代理用户转换成客户端可直接导入的两种格式：
//! - V2Ray：每行一个 `vless://` URI，整体再做一次 base64；
//! - Clash：最小可用的 YAML 配置（proxies + 选择组 + 兜底规则）。
//!
//! 这里只做纯字符串转换，不访问数据库，便于针对协议细节写单元测试。

use base64::Engine;
use uuid::Uuid;

/// 订阅里的单个代理用户。
#[derive(Debug, Clone)]
pub struct SubscriptionUser {
    pub id: Uuid,
    pub name: String,
}

/// 客户端连接入口所需的地址信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    /// 是否通过 HTTPS/WSS 访问；决定 `security` 字段。
    pub tls: bool,
    /// WebSocket 路径，固定为 `/ws`。
    pub path: String,
}

impl Endpoint {
    fn security(&self) -> &'static str {
        if self.tls {
            "tls"
        } else {
            "none"
        }
    }
}

/// 从公网 URL 或请求头推导连接入口。
///
/// `public_url` 优先（反向代理后 Host 头可能不可信）；为空时使用
/// `Host` 头，`X-Forwarded-Proto: https` 表示 TLS。缺省端口分两种：
/// - `public_url` 按 URL 规范取 http=80、https=443；
/// - `Host` 回退按主节点默认监听取 http=8080、https=443。
pub fn resolve_endpoint(
    public_url: &str,
    host_header: Option<&str>,
    forwarded_proto: Option<&str>,
) -> Endpoint {
    if let Some(parsed) = parse_http_url(public_url) {
        return Endpoint {
            path: "/ws".into(),
            ..parsed
        };
    }

    let tls = forwarded_proto.map(|value| value.eq_ignore_ascii_case("https")) == Some(true);
    let raw = host_header.unwrap_or("127.0.0.1");
    let (host, port) = split_authority(raw, if tls { 443 } else { 8080 });
    Endpoint {
        host,
        port,
        tls,
        path: "/ws".into(),
    }
}

/// 拆分 `host[:port]`，支持 `[IPv6]:port`。
///
/// 返回的主机保留方括号，便于直接拼进 `vless://` URI；裸 IPv6（多个冒号且
/// 没有方括号）整体视为主机并使用默认端口。
fn split_authority(raw: &str, default_port: u16) -> (String, u16) {
    if let Some(rest) = raw.strip_prefix('[') {
        if let Some((host, tail)) = rest.split_once(']') {
            let port = tail
                .strip_prefix(':')
                .and_then(|port| port.parse::<u16>().ok())
                .unwrap_or(default_port);
            return (format!("[{host}]"), port);
        }
    }
    match raw.rsplit_once(':') {
        Some((host, port)) if port.parse::<u16>().is_ok() && !host.contains(':') => {
            (host.to_string(), port.parse::<u16>().unwrap())
        }
        _ => (raw.to_string(), default_port),
    }
}

fn parse_http_url(raw: &str) -> Option<Endpoint> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (tls, rest) = if let Some(rest) = raw.strip_prefix("https://") {
        (true, rest)
    } else {
        (false, raw.strip_prefix("http://")?)
    };
    let authority = rest.split('/').next()?;
    let (host, port) = split_authority(authority, if tls { 443 } else { 80 });
    if host.is_empty() {
        return None;
    }
    Some(Endpoint {
        host,
        port,
        tls,
        path: "/ws".into(),
    })
}

/// 生成单个 `vless://` URI。
///
/// - `endpoint.host` 原样写入（IPv6 需带方括号），`path` 与用户名称会做
///   百分号编码；
/// - 返回可直接放进 V2Ray 订阅的一行文本，不会失败。
pub fn vless_uri(endpoint: &Endpoint, user: &SubscriptionUser) -> String {
    format!(
        "vless://{}@{}:{}?encryption=none&security={}&type=ws&host={}&path={}#{}",
        user.id,
        endpoint.host,
        endpoint.port,
        endpoint.security(),
        endpoint.host,
        percent_encode(&endpoint.path),
        percent_encode(&user.name),
    )
}

/// 生成 V2Ray 订阅内容。
///
/// 返回标准字母表、带填充的 base64；解码后是每行一个 [`vless_uri`] 的文本。
/// 没有用户时返回空字符串的 base64（即空字符串），客户端会显示空订阅。
pub fn v2ray_subscription(endpoint: &Endpoint, users: &[SubscriptionUser]) -> String {
    let joined = users
        .iter()
        .map(|user| vless_uri(endpoint, user))
        .collect::<Vec<_>>()
        .join("\n");
    base64::engine::general_purpose::STANDARD.encode(joined)
}

/// 生成 Clash 配置。
///
/// 返回 UTF-8 YAML：`proxies`（vless + ws）、一个 `PROXY` 选择组和
/// `MATCH,PROXY` 兜底规则。没有用户时输出 `proxies: []`，仍是合法 YAML；
/// 主机名与名称都按双引号字符串输出，避免 `[IPv6]` 被当成 YAML 序列、
/// 控制字符破坏整份配置。
pub fn clash_config(endpoint: &Endpoint, users: &[SubscriptionUser]) -> String {
    let mut out = String::new();
    if users.is_empty() {
        out.push_str("proxies: []\n");
    } else {
        out.push_str("proxies:\n");
        for user in users {
            out.push_str(&format!(
                "  - name: \"{}\"\n    type: vless\n    server: \"{}\"\n    port: {}\n    uuid: {}\n    udp: true\n    tls: {}\n    network: ws\n    ws-opts:\n      path: \"{}\"\n      headers:\n        Host: \"{}\"\n",
                yaml_escape(&user.name),
                yaml_escape(&endpoint.host),
                endpoint.port,
                user.id,
                endpoint.tls,
                yaml_escape(&endpoint.path),
                yaml_escape(&endpoint.host),
            ));
        }
    }
    out.push_str("proxy-groups:\n  - name: PROXY\n    type: select\n    proxies:\n");
    if users.is_empty() {
        out.push_str("      - DIRECT\n");
    } else {
        for user in users {
            out.push_str(&format!("      - \"{}\"\n", yaml_escape(&user.name)));
        }
    }
    out.push_str("rules:\n  - MATCH,PROXY\n");
    out
}

/// 百分号编码：保留 RFC 3986 unreserved 字符，其余按 UTF-8 字节编码。
fn percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// YAML 双引号字符串转义。
///
/// 除引号/反斜杠/换行外，其余 C0 控制字符与 DEL 也转成 `\xNN`：NUL 等字符
/// 不允许直接出现在 YAML 流中，会让整份配置解析失败。
fn yaml_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02X}", c as u32))
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint() -> Endpoint {
        Endpoint {
            host: "example.com".into(),
            port: 8080,
            tls: false,
            path: "/ws".into(),
        }
    }

    fn user(name: &str) -> SubscriptionUser {
        SubscriptionUser {
            id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
            name: name.into(),
        }
    }

    #[test]
    fn uri_contains_expected_fields_and_encodes_name() {
        let uri = vless_uri(&endpoint(), &user("手机 & 电脑"));
        assert!(uri.starts_with("vless://11111111-1111-1111-1111-111111111111@example.com:8080?"));
        assert!(uri.contains("encryption=none"));
        assert!(uri.contains("security=none"));
        assert!(uri.contains("type=ws"));
        assert!(uri.contains("path=%2Fws"));
        assert!(uri.ends_with("#%E6%89%8B%E6%9C%BA%20%26%20%E7%94%B5%E8%84%91"));
    }

    #[test]
    fn v2ray_subscription_is_base64_of_uri_lines() {
        let users = vec![user("phone"), user("laptop")];
        let encoded = v2ray_subscription(&endpoint(), &users);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let text = String::from_utf8(decoded).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("#phone"));
        assert!(lines[1].contains("#laptop"));
    }

    #[test]
    fn clash_config_lists_proxies_and_escapes_names() {
        let config = clash_config(&endpoint(), &[user("a\"b")]);
        assert!(config.starts_with("proxies:\n"));
        assert!(config.contains("name: \"a\\\"b\""));
        assert!(config.contains("type: vless"));
        assert!(config.contains("network: ws"));
        assert!(config.contains("path: \"/ws\""));
        assert!(config.contains("MATCH,PROXY"));
    }

    #[test]
    fn clash_config_without_users_is_still_valid() {
        let config = clash_config(&endpoint(), &[]);
        assert!(config.starts_with("proxies: []\n"));
        assert!(config.contains("- DIRECT"));
    }

    #[test]
    fn endpoint_parses_bracketed_ipv6_with_port() {
        assert_eq!(
            resolve_endpoint("", Some("[2001:db8::1]:8443"), None),
            Endpoint {
                host: "[2001:db8::1]".into(),
                port: 8443,
                tls: false,
                path: "/ws".into(),
            }
        );
        assert_eq!(
            resolve_endpoint("http://[2001:db8::2]", None, None),
            Endpoint {
                host: "[2001:db8::2]".into(),
                port: 80,
                tls: false,
                path: "/ws".into(),
            }
        );
    }

    #[test]
    fn clash_config_quotes_ipv6_hosts_and_escapes_controls() {
        let endpoint = Endpoint {
            host: "[2001:db8::1]".into(),
            port: 8443,
            tls: true,
            path: "/ws".into(),
        };
        let config = clash_config(&endpoint, &[user("a\u{0}b")]);
        // IPv6 必须作为字符串，否则 YAML 会把方括号解析成序列。
        assert!(config.contains("server: \"[2001:db8::1]\""));
        assert!(config.contains("Host: \"[2001:db8::1]\""));
        // NUL 不能原样出现在 YAML 中。
        assert!(!config.contains('\u{0}'));
        assert!(config.contains("\\x00"));
    }

    #[test]
    fn endpoint_prefers_public_url_and_derives_tls() {
        assert_eq!(
            resolve_endpoint("https://master.example.com:8443", Some("ignored:1"), None),
            Endpoint {
                host: "master.example.com".into(),
                port: 8443,
                tls: true,
                path: "/ws".into(),
            }
        );
        assert_eq!(
            resolve_endpoint("", Some("edge.example.com:9000"), Some("https")),
            Endpoint {
                host: "edge.example.com".into(),
                port: 9000,
                tls: true,
                path: "/ws".into(),
            }
        );
        assert_eq!(
            resolve_endpoint("", Some("edge.example.com"), None),
            Endpoint {
                host: "edge.example.com".into(),
                port: 8080,
                tls: false,
                path: "/ws".into(),
            }
        );
    }
}
