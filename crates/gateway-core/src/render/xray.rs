//! Render an Xray-core client that exposes a loopback SOCKS5 bridge.

use super::proxy::ProxyEndpoint;
use gateway_model::{AppConfig, XRAY_SOCKS_PORT};
use serde_json::{Value, json};

pub(super) fn render(_config: &AppConfig, proxy: &ProxyEndpoint) -> String {
    let inbound = json!({
        "listen": "127.0.0.1",
        "port": XRAY_SOCKS_PORT,
        "protocol": "socks",
        "settings": {
            "auth": "noauth",
            "udp": true,
            "ip": "127.0.0.1"
        },
        "tag": "gateway-kit-socks"
    });
    let mut user = json!({
        "id": proxy.uuid,
        "encryption": "none"
    });
    if let Some(flow) = &proxy.flow {
        user["flow"] = json!(flow);
    }
    let vnext = json!({
        "address": proxy.server,
        "port": proxy.port,
        "users": [user]
    });

    let (security, settings_key, tls_settings) = reality_settings(proxy);
    let outbound = json!({
        "protocol": "vless",
        "settings": { "vnext": [vnext] },
        "streamSettings": {
            "network": "tcp",
            "security": security,
            (settings_key): tls_settings
        },
        "tag": "proxy-out"
    });
    let config = json!({
        "log": { "loglevel": "warning" },
        "inbounds": [inbound],
        "outbounds": [outbound]
    });
    serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".into())
}

fn reality_settings(proxy: &ProxyEndpoint) -> (&'static str, &'static str, Value) {
    match (&proxy.public_key, &proxy.short_id) {
        (Some(public_key), Some(short_id)) => (
            "reality",
            "realitySettings",
            json!({
                "serverName": proxy.server_name,
                "fingerprint": proxy.fingerprint,
                "password": public_key,
                "shortId": short_id,
                "spiderX": proxy.spider_x.as_deref().unwrap_or("")
            }),
        ),
        _ => (
            "tls",
            "tlsSettings",
            json!({
                "serverName": proxy.server_name,
                "fingerprint": proxy.fingerprint
            }),
        ),
    }
}
