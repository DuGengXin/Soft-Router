//! Pure renderers for data-plane config files.

use gateway_model::{
    AppConfig, DEFAULT_UI_PORT, DNS_PROXY_PORT, Secrets, TPROXY_MARK, TPROXY_PORT, parse_ipv4_cidr,
    parse_mac, wan_direct_cidr,
};

mod dns;
mod forwarding;
mod proxy;
mod uri_helpers;
mod xray;
pub(super) use proxy::render_xray_with_secrets;
pub use proxy::{render_singbox, render_singbox_with_secrets, validate_proxy_uri};

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
    # Broadcast and multicast stay on the LAN; proxying them breaks discovery services.
    ip daddr {{ 224.0.0.0/4, 255.255.255.255/32 }} return
    meta l4proto {{ tcp, udp }} tproxy ip to 127.0.0.1:{tproxy} meta mark set {mark}
  }}
  chain output {{
    type route hook output priority mangle; policy accept;
    tcp sport {{ {ssh}, {ui} }} return
    tcp dport {{ {ssh}, {ui} }} return
  }}
  chain input {{
    type filter hook input priority filter; policy accept;
    iifname \"lo\" tcp dport {ui} accept
    iifname \"{lan}\" tcp dport {ui} accept
    iifname \"{wan}\" tcp dport {ui} accept
    tcp dport {ui} drop
  }}
  chain forward {{
    type filter hook forward priority 0; policy accept;
    iifname \"{lan}\" oifname \"{wan}\" accept
    iifname \"{wan}\" oifname \"{lan}\" accept
{wg_forward}  }}
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

pub use forwarding::{render_forwarding_env, render_sysctl};

#[cfg(test)]
#[path = "render/tests.rs"]
mod tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "render/linux_nft_tests.rs"]
mod linux_nft;
