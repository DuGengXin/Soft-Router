use serde_json::json;

/// Turn an overseas resolver spec into a sing-box DNS server entry.
pub(super) fn overseas_dns_server(
    spec: &str,
    server_name: &str,
    detour: &str,
) -> serde_json::Value {
    let spec = spec.trim();
    let (kind, rest) = if let Some(rest) = spec.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = spec.strip_prefix("tls://") {
        ("tls", rest)
    } else {
        ("udp", spec)
    };
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.parse::<u16>().is_ok() => {
            (host, port.parse::<u16>().ok())
        }
        _ => (authority, None),
    };
    let (host, kind) = if host.is_empty() {
        ("1.1.1.1", "https")
    } else {
        (host, kind)
    };
    let default_port = match kind {
        "https" => 443,
        "tls" => 853,
        _ => 53,
    };
    let mut server = json!({
        "type": kind,
        "tag": "proxy-dns",
        "server": host,
        "server_port": port.unwrap_or(default_port),
        "detour": detour
    });
    if kind == "https" {
        server["path"] = json!(if path.is_empty() { "/dns-query" } else { path });
    }
    let server_name = server_name.trim();
    if !server_name.is_empty() {
        server["tls"] = json!({ "enabled": true, "server_name": server_name });
    }
    server
}
