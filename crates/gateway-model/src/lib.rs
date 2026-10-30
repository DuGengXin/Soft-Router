//! Gateway-Kit domain types. No I/O.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use thiserror::Error;

mod config;
mod state;
mod wg_import;
pub use config::*;
pub use state::*;

pub use wg_import::{WgImport, parse_wireguard_blob};

pub const NFT_TABLE: &str = "gateway_kit";
pub const POLICY_TABLE_ID: u32 = 51820;
pub const DEFAULT_UI_PORT: u16 = 7676;
pub const TPROXY_PORT: u16 = 7895;
pub const TPROXY_MARK: u32 = 1;
/// Loopback port used to hand LAN DNS from dnsmasq to sing-box.
pub const DNS_PROXY_PORT: u16 = 5353;
/// Loopback SOCKS5 bridge from sing-box routing to the Xray VLESS client.
pub const XRAY_SOCKS_PORT: u16 = 10808;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ValidateError {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub system: SystemConfig,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default)]
    pub mode: OperatingMode,
    #[serde(default)]
    pub wan: InterfaceConfig,
    #[serde(default)]
    pub lan: InterfaceConfig,
    #[serde(default)]
    pub dhcp: DhcpConfig,
    #[serde(default)]
    pub firewall: FirewallConfig,
    #[serde(default)]
    pub wireguard: WireGuardConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
    #[serde(default)]
    pub dns: DnsConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
}

/// The single overseas transport in use. WireGuard and VLESS are mutually exclusive.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TunnelTransport {
    None,
    Wireguard,
    Vless,
}

impl AppConfig {
    /// Resolve the active overseas transport from config and stored secrets.
    pub fn tunnel_transport(&self, secrets: &Secrets) -> TunnelTransport {
        if self.wireguard.enabled {
            TunnelTransport::Wireguard
        } else if secrets
            .proxy_uri
            .as_deref()
            .is_some_and(|uri| !uri.trim().is_empty())
        {
            TunnelTransport::Vless
        } else {
            TunnelTransport::None
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            system: SystemConfig::default(),
            environment: EnvironmentConfig::default(),
            mode: OperatingMode::Observe,
            wan: InterfaceConfig {
                address: Some("192.168.40.2/24".into()),
                gateway: Some("192.168.40.1".into()),
                // DNS is an operator-owned input. Do not silently select a
                // public resolver; gateway mode validation requires an
                // explicit configured value before rendering the data plane.
                dns: Vec::new(),
                ..InterfaceConfig::default()
            },
            lan: InterfaceConfig {
                address: Some("192.168.50.1/24".into()),
                ..InterfaceConfig::default()
            },
            dhcp: DhcpConfig::default(),
            firewall: FirewallConfig::default(),
            wireguard: WireGuardConfig::default(),
            routing: RoutingConfig::default(),
            dns: DnsConfig::default(),
            ui: UiConfig::default(),
            port_forwards: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn validate(&self, secrets: &Secrets) -> Result<Vec<ValidationIssue>, ValidateError> {
        let mut issues = Vec::new();
        if self.firewall.table_name != NFT_TABLE {
            return Err(ValidateError::Message(format!(
                "nft table must be {NFT_TABLE}"
            )));
        }
        if self.routing.policy_table_id != POLICY_TABLE_ID {
            return Err(ValidateError::Message(format!(
                "policy table id must be {POLICY_TABLE_ID}"
            )));
        }
        if let Some(addr) = &self.lan.address {
            let lan = parse_ipv4_cidr(addr)?;
            if !is_rfc1918(lan.0, lan.1) {
                issues.push(ValidationIssue::warning(
                    "lan-public-cidr",
                    format!("{addr} is not RFC1918; routing conflicts are likely"),
                ));
            }
            if self.dhcp.enabled {
                let start = parse_ipv4(&self.dhcp.range_start)?;
                let end = parse_ipv4(&self.dhcp.range_end)?;
                if !ipv4_in_cidr(start, lan.0, lan.1) || !ipv4_in_cidr(end, lan.0, lan.1) {
                    return Err(ValidateError::Message(
                        "DHCP range must sit inside the LAN CIDR".into(),
                    ));
                }
                if u32::from(start) > u32::from(end) {
                    return Err(ValidateError::Message(
                        "DHCP range_start > range_end".into(),
                    ));
                }
            }
            for res in &self.dhcp.reservations {
                if res.mac.trim().is_empty() && res.ip.trim().is_empty() {
                    continue;
                }
                parse_mac(&res.mac)?;
                let ip = parse_ipv4(&res.ip)?;
                if !ipv4_in_cidr(ip, lan.0, lan.1) {
                    return Err(ValidateError::Message(
                        "DHCP reservation IP must sit inside the LAN CIDR".into(),
                    ));
                }
            }
        }
        if let Some(addr) = &self.wan.address {
            let wan = parse_ipv4_cidr(addr)?;
            if !is_rfc1918(wan.0, wan.1) {
                issues.push(ValidationIssue::warning(
                    "wan-public-cidr",
                    format!("{addr} is not RFC1918; confirm this is intentional"),
                ));
            }
            if let Some(lan_addr) = &self.lan.address {
                let lan = parse_ipv4_cidr(lan_addr)?;
                if cidr_overlap(lan.0, lan.1, wan.0, wan.1) {
                    return Err(ValidateError::Message("LAN and WAN CIDRs overlap".into()));
                }
            }
        }
        if let Some(addr) = &self.lan.address {
            let lan = parse_ipv4_cidr(addr)?;
            for fw in &self.port_forwards {
                if !fw.enabled {
                    continue;
                }
                let proto = fw.protocol.to_ascii_lowercase();
                if proto != "tcp" && proto != "udp" {
                    return Err(ValidateError::Message(
                        "port forward protocol must be tcp or udp".into(),
                    ));
                }
                if fw.wan_port == 0 || fw.lan_port == 0 {
                    return Err(ValidateError::Message(
                        "port forward ports must be 1-65535".into(),
                    ));
                }
                let ip = parse_ipv4(&fw.lan_ip)?;
                if !ipv4_in_cidr(ip, lan.0, lan.1) {
                    return Err(ValidateError::Message(
                        "port forward LAN IP must sit inside the LAN CIDR".into(),
                    ));
                }
            }
        }
        if self.mode == OperatingMode::Gateway {
            let dns_configured = self
                .lan
                .dns
                .iter()
                .chain(self.wan.dns.iter())
                .any(|server| !server.trim().is_empty());
            if !dns_configured {
                return Err(ValidateError::Message(
                    "gateway mode requires at least one configured DNS server".into(),
                ));
            }
            if self.wan.interface.trim().is_empty() || self.lan.interface.trim().is_empty() {
                return Err(ValidateError::Message(
                    "gateway mode requires wan.interface and lan.interface".into(),
                ));
            }
            if self.wan.interface == self.lan.interface {
                return Err(ValidateError::Message(
                    "WAN and LAN must be different NICs".into(),
                ));
            }
            let has_proxy_uri = secrets
                .proxy_uri
                .as_deref()
                .is_some_and(|uri| !uri.trim().is_empty());
            if !self.wireguard.enabled && !has_proxy_uri {
                return Err(ValidateError::Message(
                    "gateway mode requires WireGuard or a proxy_uri in secrets.toml".into(),
                ));
            }
            if self.wireguard.enabled && has_proxy_uri {
                return Err(ValidateError::Message(
                    "choose exactly one overseas transport: WireGuard or proxy_uri".into(),
                ));
            }
        }
        if self.wireguard.enabled {
            if self.wireguard.address.trim().is_empty()
                || self.wireguard.peer_endpoint.trim().is_empty()
            {
                return Err(ValidateError::Message(
                    "wireguard.enabled requires address and peer_endpoint".into(),
                ));
            }
            if secrets
                .wireguard_private_key
                .as_ref()
                .is_none_or(|k| k.is_empty())
                || secrets
                    .wireguard_peer_public_key
                    .as_ref()
                    .is_none_or(|k| k.is_empty())
            {
                return Err(ValidateError::Message(
                    "wireguard.enabled requires keys in secrets.toml".into(),
                ));
            }
        }
        Ok(issues)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ValidationIssue {
    pub id: String,
    pub severity: ConflictSeverity,
    pub detail: String,
}

impl ValidationIssue {
    pub fn warning(id: &str, detail: String) -> Self {
        Self {
            id: id.into(),
            severity: ConflictSeverity::Warning,
            detail,
        }
    }
}

pub fn is_loopback_bind(bind: &str) -> bool {
    matches!(bind, "127.0.0.1" | "::1" | "localhost")
        || bind.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

pub fn parse_ipv4(raw: &str) -> Result<Ipv4Addr, ValidateError> {
    raw.parse()
        .map_err(|_| ValidateError::Message(format!("invalid IPv4: {raw}")))
}

pub fn parse_ipv4_cidr(raw: &str) -> Result<(Ipv4Addr, u8), ValidateError> {
    let (addr, prefix) = raw
        .split_once('/')
        .ok_or_else(|| ValidateError::Message(format!("CIDR required: {raw}")))?;
    let ip = parse_ipv4(addr)?;
    let prefix: u8 = prefix
        .parse()
        .map_err(|_| ValidateError::Message(format!("invalid prefix: {raw}")))?;
    if prefix > 32 {
        return Err(ValidateError::Message(format!(
            "prefix out of range: {raw}"
        )));
    }
    Ok((ip, prefix))
}

pub fn ipv4_network(ip: Ipv4Addr, prefix: u8) -> Ipv4Addr {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    Ipv4Addr::from(u32::from(ip) & mask)
}

/// WAN prefix used as always-direct (work PCs reach the upstream LAN via masquerade).
pub fn wan_direct_cidr(config: &AppConfig) -> Option<String> {
    let addr = config.wan.address.as_deref()?;
    let (ip, prefix) = parse_ipv4_cidr(addr).ok()?;
    Some(format!("{}/{}", ipv4_network(ip, prefix), prefix))
}

pub fn parse_mac(raw: &str) -> Result<String, ValidateError> {
    let hex: String = raw
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if hex.len() != 12 {
        return Err(ValidateError::Message(format!("invalid MAC: {raw}")));
    }
    Ok(hex
        .as_bytes()
        .chunks(2)
        .map(|c| std::str::from_utf8(c).unwrap_or("00"))
        .collect::<Vec<_>>()
        .join(":"))
}

pub fn ipv4_in_cidr(ip: Ipv4Addr, net: Ipv4Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    (u32::from(ip) & mask) == (u32::from(net) & mask)
}

pub fn cidr_overlap(a: Ipv4Addr, ap: u8, b: Ipv4Addr, bp: u8) -> bool {
    ipv4_in_cidr(a, b, bp) || ipv4_in_cidr(b, a, ap)
}

pub fn is_rfc1918(ip: Ipv4Addr, prefix: u8) -> bool {
    let o = ip.octets();
    (o[0] == 10 && prefix >= 8)
        || (o[0] == 172 && (16..=31).contains(&o[1]) && prefix >= 12)
        || (o[0] == 192 && o[1] == 168 && prefix >= 16)
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
#[path = "model_tests.rs"]
mod tests;
