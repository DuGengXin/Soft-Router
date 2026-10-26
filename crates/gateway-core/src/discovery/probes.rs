use crate::command::{CommandError, CommandRunner};
use gateway_model::{
    AppConfig, Conflict, ConflictSeverity, OperatingMode, PreflightReport, ProbeResult, ProbeStatus,
};

pub(super) fn legacy_router_probe(
    report: &mut PreflightReport,
    runner: &impl CommandRunner,
    config: &AppConfig,
) {
    probe(
        report,
        "legacy-router-project",
        runner,
        "test",
        &["-f", "/root/work/soft-router/gateway.py"],
        |out| {
            if out.status == Some(0) {
                ProbeStatus::Detected
            } else {
                ProbeStatus::NotPresent
            }
        },
    );
    probe(
        report,
        "legacy-router-service",
        runner,
        "systemctl",
        &["is-enabled", "gateway-firewall.service"],
        |out| {
            if out.status == Some(0) && out.stdout.trim() == "enabled" {
                ProbeStatus::Detected
            } else {
                ProbeStatus::NotPresent
            }
        },
    );
    probe(
        report,
        "legacy-router-nft",
        runner,
        "nft",
        &["list", "table", "inet", "router"],
        |out| {
            if out.status == Some(0) {
                ProbeStatus::Detected
            } else {
                ProbeStatus::NotPresent
            }
        },
    );
    let detected = [
        "legacy-router-project",
        "legacy-router-service",
        "legacy-router-nft",
    ]
    .iter()
    .any(|name| unit_active(report, name));
    if detected {
        let severity = if config.mode == OperatingMode::Gateway {
            ConflictSeverity::Blocker
        } else {
            ConflictSeverity::Warning
        };
        report.conflicts.push(Conflict {
            id: "legacy-router-installed".into(),
            severity,
            resource_id: "legacy:soft-router".into(),
            title: "Legacy Python Router installation detected".into(),
            detail: "The old router project or its gateway-firewall service still owns shared LAN/proxy resources.".into(),
            recommendation: "Back up and explicitly migrate/disable the legacy installation before Gateway-Kit apply; do not flush Docker or global nftables rules.".into(),
        });
    }
}

pub(super) fn dataplane_binaries(
    config: &AppConfig,
    report: &mut PreflightReport,
    runner: &impl CommandRunner,
) {
    let severity = if config.mode == OperatingMode::Gateway {
        ConflictSeverity::Blocker
    } else {
        ConflictSeverity::Warning
    };
    probe_bin(report, runner, "sing-box", "sing-box", &["version"]);
    probe_bin(
        report,
        runner,
        "sing-box-local",
        "/usr/local/bin/sing-box",
        &["version"],
    );
    if !bin_detected(report, "sing-box") && !bin_detected(report, "sing-box-local") {
        report.conflicts.push(Conflict {
            id: "missing-sing-box".into(),
            severity,
            resource_id: "bin:sing-box".into(),
            title: "sing-box is not installed".into(),
            detail: "gateway apply starts tproxy via sing-box; without it LAN overseas traffic blackholes".into(),
            recommendation:
                "Install the official sing-box binary into PATH or /usr/local/bin before confirm apply."
                    .into(),
        });
    }
    if config.wireguard.enabled {
        probe_bin(report, runner, "wg", "wg", &["--version"]);
        probe_bin(report, runner, "wg-quick", "wg-quick", &["--help"]);
        if !bin_detected(report, "wg") && !bin_detected(report, "wg-quick") {
            report.conflicts.push(Conflict {
                id: "missing-wg".into(),
                severity,
                resource_id: "bin:wg".into(),
                title: "wireguard-tools is not installed".into(),
                detail: "gateway apply needs wg / wg-quick for the VPS tunnel".into(),
                recommendation: "apt-get install wireguard-tools".into(),
            });
        }
    }
    if config.dhcp.enabled {
        probe_bin(report, runner, "dnsmasq", "dnsmasq", &["--version"]);
        if !bin_detected(report, "dnsmasq") {
            report.conflicts.push(Conflict {
                id: "missing-dnsmasq".into(),
                severity,
                resource_id: "bin:dnsmasq".into(),
                title: "dnsmasq is not installed".into(),
                detail: "DHCP is enabled but dnsmasq is missing".into(),
                recommendation: "apt-get install dnsmasq, or disable DHCP in the wizard.".into(),
            });
        }
    }
}

fn probe_bin(
    report: &mut PreflightReport,
    runner: &impl CommandRunner,
    name: &str,
    program: &str,
    args: &[&str],
) {
    probe(report, name, runner, program, args, |out| {
        if out.status == Some(0) {
            ProbeStatus::Detected
        } else {
            ProbeStatus::Failed
        }
    });
}

fn bin_detected(report: &PreflightReport, name: &str) -> bool {
    report
        .probes
        .iter()
        .any(|p| p.name == name && p.status == ProbeStatus::Detected)
}

pub(super) fn probe(
    report: &mut PreflightReport,
    name: &str,
    runner: &impl CommandRunner,
    program: &str,
    args: &[&str],
    classify: impl Fn(&crate::command::CommandOutput) -> ProbeStatus,
) {
    match runner.run(program, args) {
        Ok(out) => {
            let status = classify(&out);
            report.probes.push(ProbeResult {
                name: name.into(),
                status,
                detail: truncate(&out.stdout),
            });
        }
        Err(CommandError::Unavailable(p)) => report.probes.push(ProbeResult {
            name: name.into(),
            status: ProbeStatus::Unavailable,
            detail: format!("{p} not found"),
        }),
        Err(error) => report.probes.push(ProbeResult {
            name: name.into(),
            status: ProbeStatus::Failed,
            detail: error.to_string(),
        }),
    }
}

pub(super) fn unit_probe(report: &mut PreflightReport, runner: &impl CommandRunner, unit: &str) {
    probe(
        report,
        unit,
        runner,
        "systemctl",
        &["is-active", unit],
        |out| {
            if out.stdout.trim() == "active" {
                ProbeStatus::Detected
            } else {
                ProbeStatus::NotPresent
            }
        },
    );
}

pub(super) fn ufw_probe(report: &mut PreflightReport, runner: &impl CommandRunner) {
    probe(report, "ufw", runner, "ufw", &["status"], |out| {
        if out.status == Some(0)
            && out
                .stdout
                .lines()
                .any(|line| line.trim().eq_ignore_ascii_case("status: active"))
        {
            ProbeStatus::Detected
        } else {
            ProbeStatus::NotPresent
        }
    });
}

pub(super) fn listen_probe(report: &mut PreflightReport, runner: &impl CommandRunner) {
    probe(report, "ss", runner, "ss", &["-lntu"], |out| {
        if out.status == Some(0) {
            ProbeStatus::Detected
        } else {
            ProbeStatus::Failed
        }
    });
}

pub(super) fn iptables_legacy_probe(report: &mut PreflightReport, runner: &impl CommandRunner) {
    probe(
        report,
        "iptables",
        runner,
        "iptables",
        &["--version"],
        |out| {
            if out.status == Some(0) {
                ProbeStatus::Detected
            } else {
                ProbeStatus::Failed
            }
        },
    );
    if let Some(ipt) = report.probes.iter().find(|p| p.name == "iptables")
        && ipt.status == ProbeStatus::Detected
        && ipt.detail.to_ascii_lowercase().contains("legacy")
        && !ipt.detail.to_ascii_lowercase().contains("nf_tables")
    {
        report.conflicts.push(Conflict {
            id: "iptables-legacy".into(),
            severity: ConflictSeverity::Blocker,
            resource_id: "iptables-legacy".into(),
            title: "iptables-legacy is active".into(),
            detail: ipt.detail.clone(),
            recommendation:
                "Use nftables (iptables-nft). Gateway-Kit will not rewrite iptables-legacy.".into(),
        });
    }
}

pub(super) fn unit_active(report: &PreflightReport, name: &str) -> bool {
    report
        .probes
        .iter()
        .any(|p| p.name == name && p.status == ProbeStatus::Detected)
}

fn truncate(s: &str) -> String {
    const MAX: usize = 4000;
    if s.len() <= MAX {
        s.to_string()
    } else {
        s[..MAX].to_string()
    }
}
