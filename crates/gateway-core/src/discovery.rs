//! Read-only host discovery.

use crate::command::CommandRunner;
use gateway_model::{
    AppConfig, Conflict, ConflictSeverity, NFT_TABLE, OperatingMode, Ownership, PreflightReport,
    ProbeStatus, ResourceObservation, ResourceOwner, ResourceType,
};

mod probes;
use probes::{
    dataplane_binaries, iptables_legacy_probe, legacy_router_probe, listen_probe, probe, ufw_probe,
    unit_active, unit_probe,
};

pub fn discover_host(config: &AppConfig, runner: &impl CommandRunner) -> PreflightReport {
    let mut report = PreflightReport::default();
    probe(&mut report, "ip", runner, "ip", &["-j", "link"], |out| {
        if out.status == Some(0) {
            ProbeStatus::Detected
        } else {
            ProbeStatus::Failed
        }
    });
    if let Some(ip) = report.probes.iter().find(|p| p.name == "ip") {
        report.interfaces = parse_interface_names(&ip.detail);
    }
    probe(
        &mut report,
        "nft",
        runner,
        "nft",
        &["list", "tables"],
        |out| {
            if out.status == Some(0) {
                ProbeStatus::Detected
            } else {
                ProbeStatus::Failed
            }
        },
    );
    unit_probe(&mut report, runner, "NetworkManager");
    unit_probe(&mut report, runner, "docker");
    ufw_probe(&mut report, runner);
    unit_probe(&mut report, runner, "firewalld");
    listen_probe(&mut report, runner);
    iptables_legacy_probe(&mut report, runner);

    if let Some(nft) = report.probes.iter().find(|p| p.name == "nft")
        && nft.status == ProbeStatus::Detected
        && nft.detail.contains(NFT_TABLE)
    {
        report.observations.push(ResourceObservation {
            resource_id: format!("nft:{NFT_TABLE}"),
            resource_type: ResourceType::NftTable,
            ownership: Ownership::Managed,
            owner: ResourceOwner::Gateway,
            summary: "gateway_kit nft table present".into(),
        });
    }

    if unit_active(&report, "ufw") {
        report.conflicts.push(Conflict {
            id: "ufw-active".into(),
            severity: ConflictSeverity::Blocker,
            resource_id: "service:ufw".into(),
            title: "UFW is active".into(),
            detail: "UFW owns host firewall policy".into(),
            recommendation: "Disable UFW before gateway apply; do not let Gateway-Kit rewrite UFW."
                .into(),
        });
    }
    if unit_active(&report, "firewalld") {
        report.conflicts.push(Conflict {
            id: "firewalld-active".into(),
            severity: ConflictSeverity::Blocker,
            resource_id: "service:firewalld".into(),
            title: "firewalld is active".into(),
            detail: "firewalld owns host firewall policy".into(),
            recommendation: "Disable firewalld before gateway apply.".into(),
        });
    }
    if unit_active(&report, "docker") {
        report.observations.push(ResourceObservation {
            resource_id: "service:docker".into(),
            resource_type: ResourceType::Service,
            ownership: Ownership::External,
            owner: ResourceOwner::Docker,
            summary: "Docker present; docker0 will not be modified".into(),
        });
        report.conflicts.push(Conflict {
            id: "docker-present".into(),
            severity: ConflictSeverity::Warning,
            resource_id: "service:docker".into(),
            title: "Docker detected".into(),
            detail: "Container bridges stay external".into(),
            recommendation: "Keep DHCP off of any port Docker already binds.".into(),
        });
    }
    if config.mode == OperatingMode::Gateway {
        for (name, iface) in [
            ("wan", &config.wan.interface),
            ("lan", &config.lan.interface),
        ] {
            if iface.is_empty() {
                report.conflicts.push(Conflict {
                    id: format!("missing-{name}"),
                    severity: ConflictSeverity::Blocker,
                    resource_id: format!("iface:{name}"),
                    title: format!("No {name} interface"),
                    detail: "gateway mode needs named NICs".into(),
                    recommendation: "Set interface names in config or wizard.".into(),
                });
            }
        }
        if config.wan.interface == config.lan.interface && !config.wan.interface.is_empty() {
            report.conflicts.push(Conflict {
                id: "wan-lan-same".into(),
                severity: ConflictSeverity::Blocker,
                resource_id: format!("iface:{}", config.wan.interface),
                title: "WAN and LAN are the same NIC".into(),
                detail: "split routing needs two interfaces".into(),
                recommendation: "Pick distinct WAN and LAN device names.".into(),
            });
        }
    }
    let own_dhcp_active = runner
        .run(
            "systemctl",
            &["is-active", "--quiet", "gateway-kit-dnsmasq"],
        )
        .is_ok_and(|out| out.status == Some(0));
    if config.dhcp.enabled
        && !own_dhcp_active
        && let Some(ss) = report.probes.iter().find(|p| p.name == "ss")
        && ss.detail.contains(":67")
    {
        report.conflicts.push(Conflict {
            id: "dhcp-port".into(),
            severity: ConflictSeverity::Blocker,
            resource_id: "port:67/udp".into(),
            title: "DHCP port in use".into(),
            detail: ss.detail.clone(),
            recommendation: "Disable the other DHCP server or disable Gateway-Kit DHCP.".into(),
        });
    }
    legacy_router_probe(&mut report, runner, config);
    dataplane_binaries(config, &mut report, runner);
    report
}

pub(crate) fn parse_interface_names(raw: &str) -> Vec<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw)
        && let Some(items) = value.as_array()
    {
        return items
            .iter()
            .filter_map(|item| item.get("ifname")?.as_str())
            .filter(|name| *name != "lo" && !name.starts_with("lo:"))
            .map(str::to_string)
            .collect();
    }
    raw.lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.split_once(": ")?.1;
            let name = rest.split(['@', ':']).next()?.trim();
            if name.is_empty() || name == "lo" {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_interface_names;

    #[test]
    fn parse_ip_json_skips_loopback() {
        let names =
            parse_interface_names(r#"[{"ifname":"lo"},{"ifname":"eth0"},{"ifname":"eth1"}]"#);
        assert_eq!(names, vec!["eth0", "eth1"]);
    }

    #[test]
    fn parse_ip_text() {
        let names = parse_interface_names("1: lo: <LOOPBACK>\n2: eth0: <BROADCAST>\n");
        assert_eq!(names, vec!["eth0"]);
    }

    #[test]
    fn iptables_legacy_blocks() {
        use crate::command::ScriptedRunner;
        use gateway_model::AppConfig;
        let runner = ScriptedRunner::succeeding();
        runner.push_ok("iptables", "iptables v1.8.7 (legacy)");
        let report = super::discover_host(&AppConfig::default(), &runner);
        assert!(
            report.conflicts.iter().any(|c| c.id == "iptables-legacy"
                && c.severity == gateway_model::ConflictSeverity::Blocker)
        );
    }

    #[test]
    fn same_wan_lan_blocks_in_gateway() {
        use crate::command::ScriptedRunner;
        use gateway_model::{AppConfig, OperatingMode};
        let cfg = AppConfig {
            mode: OperatingMode::Gateway,
            wan: gateway_model::InterfaceConfig {
                interface: "eth0".into(),
                ..Default::default()
            },
            lan: gateway_model::InterfaceConfig {
                interface: "eth0".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let report = super::discover_host(&cfg, &ScriptedRunner::succeeding());
        assert!(report.conflicts.iter().any(|c| c.id == "wan-lan-same"));
        assert!(report.has_blockers());
    }

    #[test]
    fn gateway_blocks_without_singbox() {
        use crate::command::ScriptedRunner;
        use gateway_model::{AppConfig, OperatingMode};
        let cfg = AppConfig {
            mode: OperatingMode::Gateway,
            wan: gateway_model::InterfaceConfig {
                interface: "eth0".into(),
                ..Default::default()
            },
            lan: gateway_model::InterfaceConfig {
                interface: "eth1".into(),
                ..Default::default()
            },
            dhcp: gateway_model::DhcpConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let runner = ScriptedRunner::succeeding();
        runner.push_unavailable("sing-box");
        runner.push_unavailable("/usr/local/bin/sing-box");
        let report = super::discover_host(&cfg, &runner);
        assert!(report.conflicts.iter().any(|c| c.id == "missing-sing-box"
            && c.severity == gateway_model::ConflictSeverity::Blocker));
        assert!(report.has_blockers());
    }

    #[test]
    fn observe_warns_without_singbox() {
        use crate::command::ScriptedRunner;
        use gateway_model::AppConfig;
        let mut cfg = AppConfig::default();
        cfg.dhcp.enabled = false;
        let runner = ScriptedRunner::succeeding();
        runner.push_unavailable("sing-box");
        runner.push_unavailable("/usr/local/bin/sing-box");
        let report = super::discover_host(&cfg, &runner);
        assert!(report.conflicts.iter().any(|c| c.id == "missing-sing-box"
            && c.severity == gateway_model::ConflictSeverity::Warning));
        assert!(!report.has_blockers());
    }

    #[test]
    fn ufw_inactive_status_does_not_block_gateway() {
        use crate::command::ScriptedRunner;
        use gateway_model::AppConfig;
        let runner = ScriptedRunner::succeeding();
        runner.push_ok("ufw", "Status: inactive");
        let report = super::discover_host(&AppConfig::default(), &runner);
        assert!(!report.conflicts.iter().any(|c| c.id == "ufw-active"));
    }

    #[test]
    fn gateway_blocks_when_legacy_router_project_is_present() {
        use crate::command::ScriptedRunner;
        use gateway_model::{AppConfig, OperatingMode};
        let cfg = AppConfig {
            mode: OperatingMode::Gateway,
            wan: gateway_model::InterfaceConfig {
                interface: "eth0".into(),
                ..Default::default()
            },
            lan: gateway_model::InterfaceConfig {
                interface: "eth1".into(),
                ..Default::default()
            },
            dhcp: gateway_model::DhcpConfig {
                enabled: false,
                ..Default::default()
            },
            ..Default::default()
        };
        let runner = ScriptedRunner::succeeding();
        runner.push_ok("test", "");
        let report = super::discover_host(&cfg, &runner);
        assert!(report.conflicts.iter().any(|c| {
            c.id == "legacy-router-installed"
                && c.severity == gateway_model::ConflictSeverity::Blocker
        }));
    }
}
