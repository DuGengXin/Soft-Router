use super::*;

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
