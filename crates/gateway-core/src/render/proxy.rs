//! VLESS URI parsing and sing-box/Xray proxy configuration rendering.

use crate::CoreError;
use gateway_model::{
    AppConfig, DNS_PROXY_PORT, Secrets, TPROXY_PORT, XRAY_SOCKS_PORT, wan_direct_cidr,
};
use serde_json::json;

use super::{dns::overseas_dns_server, uri_helpers::percent_decode};

#[derive(Debug, Clone)]
pub(super) struct ProxyEndpoint {
    pub(super) server: String,
    pub(super) port: u16,
    pub(super) uuid: String,
    pub(super) server_name: String,
    pub(super) public_key: Option<String>,
    pub(super) short_id: Option<String>,
    pub(super) fingerprint: String,
    pub(super) flow: Option<String>,
    pub(super) spider_x: Option<String>,
}

fn parse_proxy_uri(raw: &str) -> Result<ProxyEndpoint, CoreError> {
    let (scheme, remainder) = raw
        .split_once("://")
        .ok_or_else(|| CoreError::ConfigParse("proxy_uri must use vless://".into()))?;
    if !scheme.eq_ignore_ascii_case("vless") {
        return Err(CoreError::ConfigParse("proxy_uri must use vless://".into()));
    }
    let authority = remainder.split(['?', '#']).next().unwrap_or_default();
    let (uuid, host_port) = authority
        .rsplit_once('@')
        .ok_or_else(|| CoreError::ConfigParse("proxy_uri has no UUID or server".into()))?;
    if uuid.is_empty() {
        return Err(CoreError::ConfigParse("proxy_uri has no UUID".into()));
    }
    let (server, port) = parse_host_port(host_port)?;
    let query = remainder
        .split_once('?')
        .map(|(_, query)| query.split('#').next().unwrap_or_default())
        .unwrap_or_default()
        .split('&')
        .filter(|item| !item.is_empty())
        .map(|item| {
            let (key, value) = item.split_once('=').unwrap_or((item, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect::<std::collections::HashMap<_, _>>();
    let network = query
        .get("type")
        .or_else(|| query.get("network"))
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "tcp".into());
    if network != "tcp" {
        return Err(CoreError::ConfigParse(format!(
            "proxy_uri transport type '{network}' is not supported yet; only TCP links can be applied"
        )));
    }
    for parameter in ["path", "host", "serviceName", "mode", "headerType"] {
        if query.get(parameter).is_some_and(|value| !value.is_empty()) {
            return Err(CoreError::ConfigParse(format!(
                "proxy_uri parameter '{parameter}' requires a transport that is not supported yet"
            )));
        }
    }
    let server_name = query
        .get("sni")
        .or_else(|| query.get("serverName"))
        .map(String::as_str)
        .unwrap_or(&server)
        .to_string();
    let public_key = query.get("pbk").cloned();
    let short_id = query.get("sid").cloned();
    if public_key.is_some() != short_id.is_some() {
        return Err(CoreError::ConfigParse(
            "proxy_uri reality parameters require both pbk and sid".into(),
        ));
    }
    if query
        .get("security")
        .is_some_and(|security| !matches!(security.as_str(), "reality" | "tls"))
    {
        return Err(CoreError::ConfigParse(
            "proxy_uri security must be tls or reality".into(),
        ));
    }
    if query
        .get("security")
        .is_some_and(|security| security == "reality")
        && public_key.is_none()
    {
        return Err(CoreError::ConfigParse(
            "proxy_uri security=reality requires pbk and sid".into(),
        ));
    }
    Ok(ProxyEndpoint {
        server,
        port,
        uuid: percent_decode(uuid),
        server_name,
        public_key,
        short_id,
        fingerprint: query.get("fp").cloned().unwrap_or_else(|| "chrome".into()),
        flow: query.get("flow").cloned().filter(|value| !value.is_empty()),
        spider_x: query.get("spx").cloned().filter(|value| !value.is_empty()),
    })
}

fn parse_host_port(raw: &str) -> Result<(String, u16), CoreError> {
    if let Some(rest) = raw.strip_prefix('[') {
        let (host, port) = rest
            .split_once(']')
            .ok_or_else(|| CoreError::ConfigParse("proxy_uri has invalid IPv6 server".into()))?;
        let port = port
            .strip_prefix(':')
            .filter(|value| !value.is_empty())
            .map(|value| value.parse::<u16>())
            .transpose()
            .map_err(|_| CoreError::ConfigParse("proxy_uri has invalid port".into()))?
            .unwrap_or(443);
        if host.is_empty() {
            return Err(CoreError::ConfigParse("proxy_uri has no server".into()));
        }
        return Ok((host.to_string(), port));
    }
    let (server, port) = raw.rsplit_once(':').unwrap_or((raw, "443"));
    if server.is_empty() {
        return Err(CoreError::ConfigParse("proxy_uri has no server".into()));
    }
    let port = port
        .parse::<u16>()
        .map_err(|_| CoreError::ConfigParse("proxy_uri has invalid port".into()))?;
    Ok((server.to_string(), port))
}

pub fn render_singbox(config: &AppConfig) -> String {
    render_singbox_transport(config, None)
}

/// Validate the user-supplied VLESS link without exposing its parsed secrets.
pub fn validate_proxy_uri(raw: &str) -> Result<(), CoreError> {
    parse_proxy_uri(raw).map(|_| ())
}

pub fn render_singbox_with_secrets(
    config: &AppConfig,
    secrets: &Secrets,
) -> Result<String, CoreError> {
    let proxy = secrets
        .proxy_uri
        .as_deref()
        .filter(|uri| !uri.trim().is_empty())
        .map(parse_proxy_uri)
        .transpose()?;
    Ok(render_singbox_transport(config, proxy.as_ref()))
}

pub(crate) fn render_xray_with_secrets(
    config: &AppConfig,
    secrets: &Secrets,
) -> Result<String, CoreError> {
    let proxy = secrets
        .proxy_uri
        .as_deref()
        .filter(|uri| !uri.trim().is_empty())
        .map(parse_proxy_uri)
        .transpose()?;
    let proxy = proxy.ok_or_else(|| {
        CoreError::ConfigParse("Xray VLESS bridge requires proxy_uri in secrets.toml".into())
    })?;
    Ok(super::xray::render(config, &proxy))
}

fn render_singbox_transport(config: &AppConfig, proxy: Option<&ProxyEndpoint>) -> String {
    let cidrs: Vec<String> = {
        let mut cidrs: Vec<String> = config
            .routing
            .extra_direct_cidrs
            .iter()
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty())
            .collect();
        if let Some(wan) = wan_direct_cidr(config)
            && !cidrs.iter().any(|c| c == &wan)
        {
            cidrs.insert(0, wan);
        }
        cidrs
    };
    let cidr_rule = if cidrs.is_empty() {
        String::new()
    } else {
        let joined = cidrs
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!(",\n      {{ \"ip_cidr\": [{joined}], \"outbound\": \"direct\" }}")
    };
    let wan = &config.wan.interface;
    let direct_bind = if wan.is_empty() {
        String::new()
    } else {
        format!(",\n      \"bind_interface\": \"{wan}\"")
    };
    let china = if config.routing.china_direct {
        r#",
      {
        "rule_set": ["geoip-cn", "geosite-cn", "geosite-private"],
        "outbound": "direct"
      }"#
    } else {
        ""
    };
    let rule_set = if config.routing.china_direct {
        r#"[
      {
        "tag": "geoip-cn",
        "type": "remote",
        "format": "binary",
        "url": "https://cdn.jsdelivr.net/gh/SagerNet/sing-geoip@rule-set/geoip-cn.srs",
        "download_detour": "direct",
        "update_interval": "1d"
      },
      {
        "tag": "geosite-cn",
        "type": "remote",
        "format": "binary",
        "url": "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs",
        "download_detour": "direct",
        "update_interval": "1d"
      },
      {
        "tag": "geosite-private",
        "type": "remote",
        "format": "binary",
        "url": "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-private.srs",
        "download_detour": "direct",
        "update_interval": "1d"
      }
    ]"#
    } else {
        "[]"
    };
    let (overseas_tag, overseas_outbound) = if proxy.is_some() {
        let outbound = json!({
            "type": "socks",
            "tag": "proxy-out",
            "server": "127.0.0.1",
            "server_port": XRAY_SOCKS_PORT,
            "version": "5"
        });
        (
            "proxy-out",
            serde_json::to_string(&outbound).unwrap_or_else(|_| "{}".into()),
        )
    } else {
        (
            "wg-out",
            format!(
                "{{\"type\":\"direct\",\"tag\":\"wg-out\",\"bind_interface\": {:?}}}",
                config.wireguard.interface
            ),
        )
    };
    let dns = if config.lan.dns.is_empty() {
        &config.wan.dns
    } else {
        &config.lan.dns
    };
    let dns_server = dns
        .iter()
        .find(|server| !server.trim().is_empty())
        .map(|server| server.trim().to_string())
        .unwrap_or_default();
    let overseas_dns = overseas_dns_server(
        &config.dns.overseas,
        &config.dns.overseas_server_name,
        overseas_tag,
    );
    let dns_servers = json!([
        {
            "type": "udp",
            "tag": "direct-dns-0",
            "server": dns_server,
            "server_port": 53,
            "detour": "direct"
        },
        overseas_dns
    ]);
    let dns_json = serde_json::to_string(&dns_servers).unwrap_or_else(|_| "[]".into());
    // Overseas lookups cross the tunnel, so each uncached one costs a full round
    // trip; a longer TTL keeps repeat lookups local instead of re-crossing it.
    let dns_rules = if config.routing.china_direct {
        r#"[
      { "rule_set": ["geosite-cn", "geosite-private"], "action": "route", "server": "direct-dns-0", "rewrite_ttl": 600 },
      { "inbound": ["dns-in"], "action": "route", "server": "proxy-dns", "rewrite_ttl": 600 }
    ]"#
    } else {
        r#"[
      { "inbound": ["dns-in"], "action": "route", "server": "proxy-dns", "rewrite_ttl": 600 }
    ]"#
    };
    let auto_detect = if wan.is_empty() { "true" } else { "false" };
    format!(
        r#"{{
  "log": {{ "level": "info" }},
  "experimental": {{
    "cache_file": {{
      "enabled": true,
      "path": "sing-box-cache.db"
    }}
  }},
  "dns": {{
    "servers": {dns_json},
    "rules": {dns_rules},
    "final": "direct-dns-0",
    "strategy": "ipv4_only"
  }},
  "inbounds": [
    {{
      "type": "tproxy",
      "tag": "tproxy-in",
      "listen": "127.0.0.1",
      "listen_port": {port}
    }},
    {{
      "type": "direct",
      "tag": "dns-in",
      "listen": "127.0.0.1",
      "listen_port": {dns_port}
    }}
  ],
  "outbounds": [
    {{
      "type": "direct",
      "tag": "direct"{direct_bind}
    }},
    {overseas_outbound}
  ],
    "route": {{
    "rules": [
      {{ "inbound": ["dns-in"], "action": "hijack-dns" }},
      {{ "inbound": ["tproxy-in"], "action": "sniff" }},
      {{ "ip_is_private": true, "outbound": "direct" }}{cidr_rule}{china}
    ],
    "final": "{overseas_tag}",
    "default_domain_resolver": "direct-dns-0",
    "auto_detect_interface": {auto_detect},
    "rule_set": {rule_set}
  }}
}}
"#,
        port = TPROXY_PORT,
        dns_port = DNS_PROXY_PORT,
        dns_json = dns_json,
        overseas_tag = overseas_tag,
        overseas_outbound = overseas_outbound,
    )
}
