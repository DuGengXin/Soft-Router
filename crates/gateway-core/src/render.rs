//! Pure renderers for data-plane config files.

use crate::CoreError;
use gateway_model::{
    AppConfig, DEFAULT_UI_PORT, DNS_PROXY_PORT, Secrets, TPROXY_MARK, TPROXY_PORT, parse_ipv4_cidr,
    parse_mac, wan_direct_cidr,
};
use serde_json::json;

#[derive(Debug, Clone)]
struct ProxyEndpoint {
    server: String,
    port: u16,
    uuid: String,
    server_name: String,
    public_key: Option<String>,
    short_id: Option<String>,
    fingerprint: String,
    flow: Option<String>,
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
    Ok(ProxyEndpoint {
        server,
        port,
        uuid: percent_decode(uuid),
        server_name,
        public_key,
        short_id,
        fingerprint: query.get("fp").cloned().unwrap_or_else(|| "chrome".into()),
        flow: query.get("flow").cloned().filter(|value| !value.is_empty()),
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

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        {
            output.push(high * 16 + low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn lan_host_ip(config: &AppConfig) -> String {
    config
        .lan
        .address
        .as_deref()
        .and_then(|addr| parse_ipv4_cidr(addr).ok())
        .map(|(ip, _)| ip.to_string())
        .unwrap_or_else(|| "192.168.50.1".into())
}

pub fn render_nftables(config: &AppConfig) -> String {
    let wan = &config.wan.interface;
    let lan = &config.lan.interface;
    let lan_ip = lan_host_ip(config);
    let ssh = config.firewall.ssh_port;
    let ui = if config.ui.port == 0 {
        DEFAULT_UI_PORT
    } else {
        config.ui.port
    };
    let table = &config.firewall.table_name;
    let wg = &config.wireguard.interface;
    let wg_forward = if config.wireguard.enabled {
        format!("    iifname \"{lan}\" oifname \"{wg}\" accept\n")
    } else {
        String::new()
    };
    let wg_masq = if config.wireguard.enabled {
        format!("    oifname \"{wg}\" masquerade\n")
    } else {
        String::new()
    };
    let wan_cidr = wan_direct_cidr(config).unwrap_or_else(|| "192.168.0.0/16".into());
    let dnat_rules: String = config
        .port_forwards
        .iter()
        .filter(|fw| fw.enabled && fw.wan_port > 0 && fw.lan_port > 0 && !fw.lan_ip.is_empty())
        .map(|fw| {
            let proto = fw.protocol.to_ascii_lowercase();
            format!(
                "    iifname \"{wan}\" ip daddr 0.0.0.0/0 {proto} dport {wp} dnat ip to {lip}:{lp}\n",
                wp = fw.wan_port,
                lip = fw.lan_ip,
                lp = fw.lan_port,
            )
        })
        .collect();
    let nat_prerouting = format!(
        "  chain prerouting_nat {{
    type nat hook prerouting priority dstnat; policy accept;
    iifname \"{lan}\" udp dport 53 dnat ip to {lan_ip}:53
    iifname \"{lan}\" tcp dport 53 dnat ip to {lan_ip}:53
{dnat_rules}  }}
"
    );
    format!(
        "#!/usr/sbin/nft -f
# Coexistence: policy accept. Do not take over host input filtering.
# Host SSH/UI: only this box's LAN/loopback ports bypass tproxy (not every dport 22).
table inet {table} {{
  chain prerouting {{
    type filter hook prerouting priority mangle; policy accept;
    iifname != \"{lan}\" return
    meta nfproto ipv6 drop
    udp dport {{ 67, 68 }} return
    udp dport 53 return
    tcp dport 53 return
    ip daddr {{ {lan_ip}, 127.0.0.1 }} tcp dport 53 return
    ip daddr {{ {lan_ip}, 127.0.0.1 }} udp dport 53 return
    ip daddr {{ {lan_ip}, 127.0.0.1 }} tcp dport {{ {ssh}, {ui} }} return
    ip daddr {wan_cidr} return
    ip daddr {{ 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, 127.0.0.0/8 }} return
    meta l4proto {{ tcp, udp }} tproxy ip to 127.0.0.1:{tproxy} meta mark set {mark}
  }}
  chain output {{
    type route hook output priority mangle; policy accept;
    tcp sport {{ {ssh}, {ui} }} return
    tcp dport {{ {ssh}, {ui} }} return
  }}
  chain forward {{
    type filter hook forward priority 0; policy accept;
    iifname \"{lan}\" oifname \"{wan}\" accept
{wg_forward}    iifname \"{wan}\" oifname \"{lan}\" accept
  }}
  chain postrouting {{
    type nat hook postrouting priority srcnat; policy accept;
    oifname \"{wan}\" masquerade
{wg_masq}
  }}
{nat_prerouting}}}
",
        tproxy = TPROXY_PORT,
        mark = TPROXY_MARK,
        wg_forward = wg_forward,
        wg_masq = wg_masq,
    )
}

pub fn render_wg_quick(config: &AppConfig, secrets: &Secrets) -> String {
    let psk = secrets
        .wireguard_preshared_key
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|k| format!("PresharedKey = {k}\n"))
        .unwrap_or_default();
    format!(
        "[Interface]
Address = {address}
ListenPort = {listen}
PrivateKey = {privk}
Table = off

[Peer]
PublicKey = {pubk}
{psk}AllowedIPs = {allowed}
Endpoint = {endpoint}
PersistentKeepalive = 25
",
        address = config.wireguard.address,
        listen = config.wireguard.listen_port,
        privk = secrets.wireguard_private_key.as_deref().unwrap_or(""),
        pubk = secrets.wireguard_peer_public_key.as_deref().unwrap_or(""),
        allowed = config.wireguard.peer_allowed_ips,
        endpoint = config.wireguard.peer_endpoint,
    )
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
        "rule_set": ["geoip-cn", "geosite-cn"],
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
        "download_detour": "direct"
      },
      {
        "tag": "geosite-cn",
        "type": "remote",
        "format": "binary",
        "url": "https://cdn.jsdelivr.net/gh/SagerNet/sing-geosite@rule-set/geosite-cn.srs",
        "download_detour": "direct"
      }
    ]"#
    } else {
        "[]"
    };
    let (overseas_tag, overseas_outbound) = if let Some(proxy) = proxy {
        let mut tls = json!({
            "enabled": true,
            "server_name": proxy.server_name,
            "utls": {"enabled": true, "fingerprint": proxy.fingerprint}
        });
        if let (Some(public_key), Some(short_id)) = (&proxy.public_key, &proxy.short_id) {
            tls["reality"] = json!({
                "enabled": true,
                "public_key": public_key,
                "short_id": short_id
            });
        }
        let mut outbound = json!({
            "type": "vless",
            "tag": "proxy-out",
            "server": proxy.server,
            "server_port": proxy.port,
            "uuid": proxy.uuid,
            "domain_resolver": "direct-dns-0",
            "tls": tls
        });
        if let Some(flow) = &proxy.flow {
            outbound["flow"] = json!(flow);
        }
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
    let dns_servers = json!([
        {
            "type": "udp",
            "tag": "direct-dns-0",
            "server": dns_server.clone(),
            "server_port": 53,
            "detour": "direct"
        },
        {
            "type": "udp",
            "tag": "proxy-dns",
            "server": dns_server,
            "server_port": 53,
            "detour": overseas_tag
        }
    ]);
    let dns_json = serde_json::to_string(&dns_servers).unwrap_or_else(|_| "[]".into());
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
    "rules": [
      {{ "inbound": ["dns-in"], "action": "route", "server": "proxy-dns" }}
    ],
    "final": "proxy-dns",
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

pub fn render_dnsmasq(config: &AppConfig) -> String {
    let lan_ip = lan_host_ip(config);
    let mut hosts = String::new();
    for res in &config.dhcp.reservations {
        if res.mac.trim().is_empty() || res.ip.trim().is_empty() {
            continue;
        }
        let Ok(mac) = parse_mac(&res.mac) else {
            continue;
        };
        hosts.push_str(&format!("dhcp-host={mac},{}\n", res.ip.trim()));
    }
    format!(
        "interface={lan}
listen-address={lan_ip}
bind-interfaces
dhcp-authoritative
dhcp-option=3,{lan_ip}
dhcp-range={start},{end},{lease}
no-resolv
no-hosts
server=127.0.0.1#{dns_port}
dhcp-option=6,{lan_ip}
{hosts}except-interface={wan}
",
        lan = config.lan.interface,
        wan = config.wan.interface,
        start = config.dhcp.range_start,
        end = config.dhcp.range_end,
        lease = config.dhcp.lease_time,
        dns_port = DNS_PROXY_PORT,
    )
}

/// Render the interface contract consumed by the Docker FORWARD compatibility unit.
pub fn render_forwarding_env(config: &AppConfig) -> String {
    format!(
        "LAN_IF={}\nWAN_IF={}\n",
        config.lan.interface, config.wan.interface
    )
}

pub fn render_sysctl() -> &'static str {
    "net.ipv4.ip_forward=1
net.ipv4.conf.all.route_localnet=1
"
}

#[cfg(test)]
mod tests {
    use super::{render_singbox, render_singbox_with_secrets};
    use gateway_model::{AppConfig, Secrets};

    #[test]
    fn singbox_json_parses() {
        let cfg = AppConfig {
            wan: gateway_model::InterfaceConfig {
                interface: "eth0".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let raw = render_singbox(&cfg);
        let value: serde_json::Value = serde_json::from_str(&raw).expect(&raw);
        assert_eq!(value["route"]["final"], "wg-out");
        assert_eq!(value["outbounds"][0]["bind_interface"], "eth0");
        assert_eq!(value["outbounds"][1]["bind_interface"], "wg0");
        assert_eq!(value["route"]["auto_detect_interface"], false);
        assert_eq!(
            value["experimental"]["cache_file"]["path"],
            "sing-box-cache.db"
        );
        let sets = value["route"]["rule_set"].as_array().expect("rule_set");
        assert!(
            sets.iter()
                .all(|s| s["url"].as_str().unwrap_or("").contains("jsdelivr.net")),
            "{raw}"
        );
        assert!(
            sets.iter().all(|s| !s["url"]
                .as_str()
                .unwrap_or("")
                .contains("githubusercontent")),
            "{raw}"
        );
        assert!(
            value["route"]["rules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r.get("ip_cidr").is_some())
        );
    }

    #[test]
    fn singbox_keeps_wan_prefix_when_extra_direct_empty() {
        let cfg = AppConfig {
            routing: gateway_model::RoutingConfig {
                china_direct: false,
                extra_direct_cidrs: vec![],
                ..Default::default()
            },
            ..Default::default()
        };
        let raw = render_singbox(&cfg);
        let value: serde_json::Value = serde_json::from_str(&raw).expect(&raw);
        let rules = value["route"]["rules"].as_array().expect("rules");
        let has_wan = rules.iter().any(|r| {
            r.get("ip_cidr")
                .and_then(|v| v.as_array())
                .is_some_and(|arr| arr.iter().any(|c| c.as_str() == Some("192.168.40.0/24")))
        });
        assert!(has_wan, "{raw}");
        assert_eq!(value["route"]["final"], "wg-out");
    }

    #[test]
    fn singbox_renders_vless_reality_proxy_uri_without_wireguard() {
        let cfg = AppConfig::default();
        let secrets = Secrets {
            proxy_uri: Some("vless://11111111-1111-1111-1111-111111111111@example.com:443?sni=example.org&security=reality&pbk=public-key&sid=short-id&fp=chrome&flow=xtls-rprx-vision".into()),
            ..Secrets::default()
        };
        let raw = render_singbox_with_secrets(&cfg, &secrets).expect("valid proxy URI");
        let value: serde_json::Value = serde_json::from_str(&raw).expect(&raw);
        assert_eq!(value["route"]["final"], "proxy-out");
        assert!(value["inbounds"][0].get("sniff").is_none());
        assert_eq!(value["route"]["rules"][0]["action"], "hijack-dns");
        assert_eq!(value["route"]["rules"][1]["action"], "sniff");
        assert_eq!(value["dns"]["final"], "proxy-dns");
        assert_eq!(value["dns"]["strategy"], "ipv4_only");
        assert_eq!(value["outbounds"][1]["type"], "vless");
        assert_eq!(value["outbounds"][1]["server"], "example.com");
        assert_eq!(
            value["outbounds"][1]["uuid"],
            "11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(
            value["outbounds"][1]["tls"]["reality"]["public_key"],
            "public-key"
        );
        assert_eq!(value["outbounds"][1]["flow"], "xtls-rprx-vision");
        assert!(value["outbounds"][1].get("bind_interface").is_none());
    }

    #[test]
    fn singbox_rejects_malformed_proxy_uri() {
        let cfg = AppConfig::default();
        let secrets = Secrets {
            proxy_uri: Some("https://example.com/not-vless".into()),
            ..Secrets::default()
        };
        assert!(render_singbox_with_secrets(&cfg, &secrets).is_err());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_nft {
    use super::render_nftables;
    use gateway_model::AppConfig;
    use std::process::Command;

    #[test]
    fn nft_check_accepts_gateway_table() {
        let mut cfg = AppConfig {
            wan: gateway_model::InterfaceConfig {
                interface: "eth0".into(),
                ..Default::default()
            },
            lan: gateway_model::InterfaceConfig {
                interface: "eth1".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.port_forwards.push(gateway_model::PortForward {
            enabled: true,
            protocol: "tcp".into(),
            wan_port: 8080,
            lan_ip: "192.168.50.10".into(),
            lan_port: 80,
        });
        let body = render_nftables(&cfg);
        let path = std::env::temp_dir().join("gateway-kit-nft-check.conf");
        std::fs::write(&path, &body).expect("write nft fixture");
        let output = Command::new("nft")
            .args(["-c", "-f", path.to_str().expect("utf8 path")])
            .output();
        let output = match output {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                panic!("nft not installed; CI must apt-get install nftables");
            }
            Err(error) => panic!("nft: {error}"),
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Operation not permitted") || stderr.contains("Permission denied") {
            eprintln!("skipping nft runtime check without required kernel permissions");
            return;
        }
        assert!(output.status.success(), "nft -c failed: {}", stderr);
    }
}
