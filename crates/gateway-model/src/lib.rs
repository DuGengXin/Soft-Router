//! Gateway-Kit domain types. No I/O.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use thiserror::Error;

mod wg_import;

pub use wg_import::{WgImport, parse_wireguard_blob};

pub const NFT_TABLE: &str = "gateway_kit";
pub const POLICY_TABLE_ID: u32 = 51820;
pub const DEFAULT_UI_PORT: u16 = 7676;
pub const TPROXY_PORT: u16 = 7895;
pub const TPROXY_MARK: u32 = 1;
/// Loopback port used to hand LAN DNS from dnsmasq to sing-box.
pub const DNS_PROXY_PORT: u16 = 5353;

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
    pub ui: UiConfig,
    #[serde(default)]
    pub port_forwards: Vec<PortForward>,
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
        if !is_loopback_bind(&self.ui.bind)
            && secrets.ui_lan_token.as_ref().is_none_or(|t| t.is_empty())
        {
            return Err(ValidateError::Message(
                "non-loopback UI bind requires secrets.ui_lan_token".into(),
            ));
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemConfig {
    pub hostname: String,
    pub timezone: String,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            hostname: "gateway".into(),
            timezone: "Asia/Shanghai".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentConfig {
    pub conflict_policy: ConflictPolicy,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            conflict_policy: ConflictPolicy::Block,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Block,
    Acknowledge,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum OperatingMode {
    #[default]
    Observe,
    Gateway,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InterfaceConfig {
    #[serde(default)]
    pub interface: String,
    pub mac: Option<String>,
    pub address: Option<String>,
    pub gateway: Option<String>,
    #[serde(default)]
    pub dns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DhcpConfig {
    pub enabled: bool,
    pub range_start: String,
    pub range_end: String,
    pub lease_time: String,
    #[serde(default)]
    pub reservations: Vec<DhcpReservation>,
}

impl Default for DhcpConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            range_start: "192.168.50.100".into(),
            range_end: "192.168.50.200".into(),
            lease_time: "12h".into(),
            reservations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DhcpReservation {
    pub mac: String,
    pub ip: String,
    #[serde(default)]
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortForward {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_tcp")]
    pub protocol: String,
    pub wan_port: u16,
    pub lan_ip: String,
    pub lan_port: u16,
}

fn default_true() -> bool {
    true
}

fn default_tcp() -> String {
    "tcp".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FirewallConfig {
    pub enabled: bool,
    pub table_name: String,
    pub ssh_port: u16,
}

impl Default for FirewallConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            table_name: NFT_TABLE.into(),
            ssh_port: 22,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WireGuardConfig {
    pub enabled: bool,
    pub interface: String,
    pub address: String,
    pub listen_port: u16,
    pub peer_endpoint: String,
    pub peer_allowed_ips: String,
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interface: "wg0".into(),
            address: String::new(),
            listen_port: 51820,
            peer_endpoint: String::new(),
            peer_allowed_ips: "0.0.0.0/0".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutingConfig {
    pub china_direct: bool,
    pub extra_direct_cidrs: Vec<String>,
    pub policy_table_id: u32,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            china_direct: true,
            extra_direct_cidrs: vec![
                "10.0.0.0/8".into(),
                "172.16.0.0/12".into(),
                "192.168.0.0/16".into(),
            ],
            policy_table_id: POLICY_TABLE_ID,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiConfig {
    pub bind: String,
    pub port: u16,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".into(),
            port: DEFAULT_UI_PORT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Secrets {
    pub wireguard_private_key: Option<String>,
    pub wireguard_peer_public_key: Option<String>,
    pub wireguard_preshared_key: Option<String>,
    pub ui_lan_token: Option<String>,
    pub proxy_uri: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct SecretsPatch {
    pub wireguard_private_key: Option<String>,
    pub wireguard_peer_public_key: Option<String>,
    pub wireguard_preshared_key: Option<String>,
    pub ui_lan_token: Option<String>,
    pub proxy_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SecretsStatus {
    pub wireguard_private_key_present: bool,
    pub wireguard_peer_public_key_present: bool,
    pub wireguard_preshared_key_present: bool,
    pub ui_lan_token_present: bool,
    pub proxy_uri_present: bool,
}

impl Secrets {
    pub fn status(&self) -> SecretsStatus {
        SecretsStatus {
            wireguard_private_key_present: present_secret(&self.wireguard_private_key),
            wireguard_peer_public_key_present: present_secret(&self.wireguard_peer_public_key),
            wireguard_preshared_key_present: present_secret(&self.wireguard_preshared_key),
            ui_lan_token_present: present_secret(&self.ui_lan_token),
            proxy_uri_present: present_secret(&self.proxy_uri),
        }
    }

    pub fn apply_patch(&mut self, patch: &SecretsPatch) {
        overlay(
            &mut self.wireguard_private_key,
            &patch.wireguard_private_key,
        );
        overlay(
            &mut self.wireguard_peer_public_key,
            &patch.wireguard_peer_public_key,
        );
        overlay(
            &mut self.wireguard_preshared_key,
            &patch.wireguard_preshared_key,
        );
        overlay(&mut self.ui_lan_token, &patch.ui_lan_token);
        overlay_trimmed(&mut self.proxy_uri, &patch.proxy_uri);
    }
}

fn present_secret(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|s| !s.is_empty())
}

fn overlay(slot: &mut Option<String>, incoming: &Option<String>) {
    let Some(value) = incoming else {
        return;
    };
    if value.is_empty() {
        *slot = None;
    } else {
        *slot = Some(value.clone());
    }
}

fn overlay_trimmed(slot: &mut Option<String>, incoming: &Option<String>) {
    let Some(value) = incoming else {
        return;
    };
    let value = value.trim();
    if value.is_empty() {
        *slot = None;
    } else {
        *slot = Some(value.to_string());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Interface,
    Address,
    Route,
    PolicyRule,
    NftTable,
    FirewallBackend,
    Service,
    Port,
    Sysctl,
    File,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceOwner {
    Gateway,
    NetworkManager,
    Networkd,
    Docker,
    Ufw,
    Firewalld,
    User,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    Managed,
    Observed,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceObservation {
    pub resource_id: String,
    pub resource_type: ResourceType,
    pub ownership: Ownership,
    pub owner: ResourceOwner,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictSeverity {
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStatus {
    Detected,
    NotPresent,
    Unavailable,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeResult {
    pub name: String,
    pub status: ProbeStatus,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Conflict {
    pub id: String,
    pub severity: ConflictSeverity,
    pub resource_id: String,
    pub title: String,
    pub detail: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PreflightReport {
    pub observations: Vec<ResourceObservation>,
    pub conflicts: Vec<Conflict>,
    pub probes: Vec<ProbeResult>,
    #[serde(default)]
    pub interfaces: Vec<String>,
}

impl PreflightReport {
    pub fn has_blockers(&self) -> bool {
        self.conflicts
            .iter()
            .any(|c| c.severity == ConflictSeverity::Blocker)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    ObserveOnly,
    Blocked,
    Ready,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    WriteFile,
    NftApply,
    Sysctl,
    IpRule,
    Systemctl,
    WgSync,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanAction {
    pub id: String,
    pub kind: ActionKind,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderedFile {
    pub relative_path: String,
    pub contents: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangePlan {
    pub status: PlanStatus,
    pub explanation: String,
    pub issues: Vec<ValidationIssue>,
    pub actions: Vec<PlanAction>,
    pub files: Vec<RenderedFile>,
}

impl ChangePlan {
    pub fn redacted(&self) -> Self {
        let mut plan = self.clone();
        for file in &mut plan.files {
            file.contents = file
                .contents
                .lines()
                .map(|line| {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("PrivateKey") || trimmed.starts_with("PresharedKey") {
                        format!(
                            "{} = [redacted]",
                            trimmed.split('=').next().unwrap_or("PrivateKey").trim()
                        )
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if file.relative_path.ends_with("sing-box.json")
                && let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&file.contents)
            {
                Self::redact_json_secrets(&mut value);
                file.contents = serde_json::to_string_pretty(&value)
                    .unwrap_or_else(|_| "{\"redacted\":true}".into());
            }
        }
        plan
    }

    fn redact_json_secrets(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(object) => {
                for (key, value) in object.iter_mut() {
                    if matches!(key.as_str(), "uuid" | "public_key" | "private_key") {
                        *value = serde_json::Value::String("[redacted]".into());
                    } else {
                        Self::redact_json_secrets(value);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    Self::redact_json_secrets(value);
                }
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Observe,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthSnapshot {
    pub status: HealthStatus,
    pub message: String,
    pub failed_checks: Vec<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    #[serde(default)]
    pub wan_uplink: String,
    #[serde(default)]
    pub tunnel_uplink: String,
}

/// Snapshot of the gateway host. Collected via `sysinfo`, not raw /proc parsers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostMetrics {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub uptime_secs: u64,
    pub cpu_percent: f32,
    pub cpu_count: usize,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub mem_total_bytes: u64,
    pub mem_used_bytes: u64,
    pub disks: Vec<DiskMetric>,
    pub nets: Vec<NetMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetric {
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetMetric {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplyReason {
    UserConfirm,
    BootRestore,
    Disable,
    Rollback,
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
mod tests {
    use super::*;

    fn valid_gateway() -> (AppConfig, Secrets) {
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.8:51820".into();
        let secrets = Secrets {
            wireguard_private_key: Some("priv".into()),
            wireguard_peer_public_key: Some("pub".into()),
            ..Secrets::default()
        };
        (cfg, secrets)
    }

    #[test]
    fn default_is_observe_and_private() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.mode, OperatingMode::Observe);
        assert_eq!(cfg.firewall.table_name, NFT_TABLE);
        assert!(cfg.wan.dns.is_empty());
        let issues = cfg.validate(&Secrets::default()).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn public_wan_warns() {
        let mut cfg = AppConfig::default();
        cfg.wan.address = Some("192.111.40.2/24".into());
        let issues = cfg.validate(&Secrets::default()).unwrap();
        assert!(issues.iter().any(|i| i.id == "wan-public-cidr"));
    }

    #[test]
    fn overlapping_cidrs_fail() {
        let mut cfg = AppConfig::default();
        cfg.wan.address = Some("192.168.50.2/24".into());
        assert!(cfg.validate(&Secrets::default()).is_err());
    }

    #[test]
    fn dhcp_outside_lan_fails() {
        let mut cfg = AppConfig::default();
        cfg.dhcp.range_start = "10.0.0.1".into();
        assert!(cfg.validate(&Secrets::default()).is_err());
    }

    #[test]
    fn lan_listen_requires_token() {
        let mut cfg = AppConfig::default();
        cfg.ui.bind = "0.0.0.0".into();
        assert!(cfg.validate(&Secrets::default()).is_err());
        let secrets = Secrets {
            ui_lan_token: Some("token".into()),
            ..Secrets::default()
        };
        assert!(cfg.validate(&secrets).is_ok());
    }

    #[test]
    fn gateway_requires_nics_and_wg_keys() {
        let (cfg, secrets) = valid_gateway();
        assert!(cfg.validate(&secrets).is_ok());
        let mut no_nic = cfg.clone();
        no_nic.lan.interface.clear();
        assert!(no_nic.validate(&secrets).is_err());
        let mut no_wg = cfg.clone();
        no_wg.wireguard.enabled = false;
        assert!(no_wg.validate(&secrets).is_err());
    }

    #[test]
    fn gateway_rejects_empty_dns_configuration() {
        let (mut cfg, secrets) = valid_gateway();
        cfg.wan.dns.clear();
        cfg.lan.dns.clear();
        assert!(cfg.validate(&secrets).is_err());
    }

    #[test]
    fn gateway_accepts_proxy_uri_without_wireguard() {
        let (mut cfg, _) = valid_gateway();
        cfg.wireguard.enabled = false;
        let secrets = Secrets {
            proxy_uri: Some("vless://uuid@example.com:443?sni=example.com".into()),
            ..Secrets::default()
        };
        assert!(cfg.validate(&secrets).is_ok());
    }

    #[test]
    fn gateway_rejects_two_overseas_transports() {
        let (cfg, mut secrets) = valid_gateway();
        secrets.proxy_uri = Some("vless://uuid@example.com:443".into());
        assert!(cfg.validate(&secrets).is_err());
    }

    #[test]
    fn secrets_patch_overlays_without_clearing_omitted_fields() {
        let mut secrets = Secrets {
            wireguard_private_key: Some("keep".into()),
            ui_lan_token: Some("old".into()),
            ..Secrets::default()
        };
        secrets.apply_patch(&SecretsPatch {
            ui_lan_token: Some("new".into()),
            ..SecretsPatch::default()
        });
        assert_eq!(secrets.wireguard_private_key.as_deref(), Some("keep"));
        assert_eq!(secrets.ui_lan_token.as_deref(), Some("new"));
        assert!(secrets.status().wireguard_private_key_present);
        assert!(!secrets.status().wireguard_peer_public_key_present);
        secrets.apply_patch(&SecretsPatch {
            wireguard_private_key: Some(String::new()),
            ..SecretsPatch::default()
        });
        assert!(!secrets.status().wireguard_private_key_present);
        secrets.apply_patch(&SecretsPatch {
            proxy_uri: Some("  vless://uuid@example.com:443  ".into()),
            ..SecretsPatch::default()
        });
        assert_eq!(
            secrets.proxy_uri.as_deref(),
            Some("vless://uuid@example.com:443")
        );
    }

    #[test]
    fn redacted_plan_hides_proxy_credentials() {
        let plan = ChangePlan {
            status: PlanStatus::Ready,
            explanation: String::new(),
            issues: Vec::new(),
            actions: Vec::new(),
            files: vec![RenderedFile {
                relative_path: "sing-box.json".into(),
                contents:
                    r#"{"outbounds":[{"uuid":"secret","tls":{"reality":{"public_key":"key"}}}]}"#
                        .into(),
            }],
        };
        let redacted = plan.redacted();
        assert!(!redacted.files[0].contents.contains("secret"));
        assert!(!redacted.files[0].contents.contains(": \"key\""));
        assert!(redacted.files[0].contents.contains("[redacted]"));
    }

    #[test]
    fn wan_direct_cidr_is_network_prefix() {
        let cfg = AppConfig::default();
        assert_eq!(wan_direct_cidr(&cfg).as_deref(), Some("192.168.40.0/24"));
    }

    #[test]
    fn reservation_outside_lan_fails() {
        let mut cfg = AppConfig::default();
        cfg.dhcp.reservations.push(DhcpReservation {
            mac: "aa:bb:cc:dd:ee:ff".into(),
            ip: "10.1.1.8".into(),
            hostname: String::new(),
        });
        assert!(cfg.validate(&Secrets::default()).is_err());
    }

    #[test]
    fn port_forward_must_target_lan() {
        let mut cfg = AppConfig::default();
        cfg.port_forwards.push(PortForward {
            enabled: true,
            protocol: "tcp".into(),
            wan_port: 8080,
            lan_ip: "10.9.9.9".into(),
            lan_port: 80,
        });
        assert!(cfg.validate(&Secrets::default()).is_err());
    }

    #[test]
    fn blockers_detected() {
        let mut report = PreflightReport::default();
        assert!(!report.has_blockers());
        report.conflicts.push(Conflict {
            id: "x".into(),
            severity: ConflictSeverity::Blocker,
            resource_id: "ufw".into(),
            title: "UFW".into(),
            detail: "active".into(),
            recommendation: "disable ufw".into(),
        });
        assert!(report.has_blockers());
    }
}
