use super::*;

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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

/// Resolver policy for the rendered sing-box DNS block.
///
/// Domestic names keep resolving through the configured interface DNS over
/// the direct outbound. Overseas lookups cross the tunnel, so DoH/DoT can
/// recover from packet loss with TCP retransmission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DnsConfig {
    /// Accepts `ip`, `ip:port`, `tls://host[:port]` or `https://host[:port]/path`.
    #[serde(default = "default_overseas_dns")]
    pub overseas: String,
    /// Optional TLS SNI override for IP-based TLS endpoints.
    #[serde(default)]
    pub overseas_server_name: String,
}

/// Plain UDP is preferred: these queries already cross the tunnel, so DoH adds a
/// TCP+TLS handshake over a high-latency path without protecting anything extra.
pub fn default_overseas_dns() -> String {
    "1.1.1.1".into()
}

impl Default for DnsConfig {
    fn default() -> Self {
        Self {
            overseas: default_overseas_dns(),
            overseas_server_name: String::new(),
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
            bind: "0.0.0.0".into(),
            port: DEFAULT_UI_PORT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct Secrets {
    pub wireguard_private_key: Option<String>,
    pub wireguard_peer_public_key: Option<String>,
    pub wireguard_preshared_key: Option<String>,
    pub proxy_uri: Option<String>,
    pub admin_username: Option<String>,
    pub admin_password_hash: Option<String>,
    #[serde(default)]
    pub admin_password_change_required: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct SecretsPatch {
    pub wireguard_private_key: Option<String>,
    pub wireguard_peer_public_key: Option<String>,
    pub wireguard_preshared_key: Option<String>,
    pub proxy_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SecretsStatus {
    pub wireguard_private_key_present: bool,
    pub wireguard_peer_public_key_present: bool,
    pub wireguard_preshared_key_present: bool,
    pub proxy_uri_present: bool,
    pub admin_account_configured: bool,
}

impl Secrets {
    pub fn status(&self) -> SecretsStatus {
        SecretsStatus {
            wireguard_private_key_present: present_secret(&self.wireguard_private_key),
            wireguard_peer_public_key_present: present_secret(&self.wireguard_peer_public_key),
            wireguard_preshared_key_present: present_secret(&self.wireguard_preshared_key),
            proxy_uri_present: present_secret(&self.proxy_uri),
            admin_account_configured: self
                .admin_username
                .as_deref()
                .is_some_and(|s| !s.is_empty())
                && present_secret(&self.admin_password_hash),
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
