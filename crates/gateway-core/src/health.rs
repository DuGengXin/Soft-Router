//! Post-apply health. Does not parse secrets.

use crate::command::CommandRunner;
use gateway_model::{
    AppConfig, DNS_PROXY_PORT, HealthSnapshot, HealthStatus, TPROXY_PORT, TunnelTransport,
    XRAY_SOCKS_PORT,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Neutral endpoint that returns 204 and is unreachable without a working overseas path.
///
/// Deliberately not `gstatic.com`: that host is classified as mainland by
/// `geosite-cn`, so sing-box resolves it to a domestic CDN and it proves nothing
/// about the tunnel.
pub const EGRESS_PROBE_URL: &str = "https://www.google.com/generate_204";

/// Tunnelled requests regularly need several seconds; a tighter deadline reports
/// false failures.
const EGRESS_PROBE_TIMEOUT_SECS: &str = "15";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EgressStatus {
    Unknown,
    Reachable,
    Unreachable,
}

/// Result of a real request through the configured overseas transport.
///
/// `check_applied_with_xray_artifact` only proves processes and listeners exist;
/// this proves the tunnel actually carries traffic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EgressReport {
    pub status: EgressStatus,
    pub detail: String,
    pub checked_at: i64,
}

impl EgressReport {
    fn unknown(detail: impl Into<String>) -> Self {
        Self {
            status: EgressStatus::Unknown,
            detail: detail.into(),
            checked_at: now_secs(),
        }
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

/// Send one short request through the active transport and report what came back.
pub fn probe_egress(
    runner: &impl CommandRunner,
    transport: TunnelTransport,
    wireguard_interface: &str,
) -> EgressReport {
    let label = match transport {
        TunnelTransport::Vless => format!("VLESS via Xray SOCKS :{XRAY_SOCKS_PORT}"),
        TunnelTransport::Wireguard if !wireguard_interface.is_empty() => {
            format!("WireGuard {wireguard_interface}")
        }
        _ => return EgressReport::unknown("no overseas transport configured"),
    };
    let mut args: Vec<String> = [
        "-sS",
        "-m",
        EGRESS_PROBE_TIMEOUT_SECS,
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
    ]
    .iter()
    .map(|arg| (*arg).to_string())
    .collect();
    match transport {
        TunnelTransport::Vless => {
            args.push("-x".into());
            args.push(format!("socks5h://127.0.0.1:{XRAY_SOCKS_PORT}"));
        }
        _ => {
            args.push("--interface".into());
            args.push(wireguard_interface.to_string());
        }
    }
    args.push(EGRESS_PROBE_URL.to_string());
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    // A single tunnelled request is occasionally slow enough to look like an
    // outage, so confirm a failure once before reporting it.
    let mut last = probe_once(runner, &argv);
    if last.0 != Some(true) {
        last = probe_once(runner, &argv);
    }
    let (ok, detail) = last;
    EgressReport {
        status: if ok == Some(true) {
            EgressStatus::Reachable
        } else {
            EgressStatus::Unreachable
        },
        detail: format!("{label}: {detail}"),
        checked_at: now_secs(),
    }
}

/// Returns `(reached, detail)`; `None` means the probe itself could not run.
fn probe_once(runner: &impl CommandRunner, argv: &[&str]) -> (Option<bool>, String) {
    match runner.run("curl", argv) {
        Ok(out) if out.status == Some(0) && out.stdout.trim() == "204" => {
            (Some(true), "HTTP 204".into())
        }
        Ok(out) => {
            let code = out.stdout.trim();
            let detail = if code.is_empty() || code == "000" {
                first_line(&out.stderr).unwrap_or_else(|| "no response".into())
            } else {
                format!("HTTP {code}")
            };
            (Some(false), detail)
        }
        Err(error) => (None, error.to_string()),
    }
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

pub fn check_applied(
    config: &AppConfig,
    runner: &impl CommandRunner,
    require_tproxy: bool,
) -> HealthSnapshot {
    check_applied_with_artifacts(config, runner, require_tproxy, None, None)
}

/// Check applied dataplane state and, when requested, the GeoFile cache.
pub fn check_applied_with_cache(
    config: &AppConfig,
    runner: &impl CommandRunner,
    require_tproxy: bool,
    geo_cache: Option<&Path>,
) -> HealthSnapshot {
    check_applied_with_artifacts(config, runner, require_tproxy, None, geo_cache)
}

/// Check applied dataplane state and validate the generated sing-box artifact.
pub fn check_applied_with_artifacts(
    config: &AppConfig,
    runner: &impl CommandRunner,
    require_tproxy: bool,
    singbox_config: Option<&Path>,
    geo_cache: Option<&Path>,
) -> HealthSnapshot {
    check_applied_with_xray_artifact(
        config,
        runner,
        require_tproxy,
        singbox_config,
        geo_cache,
        None,
    )
}

/// Check generated Xray configuration, its service, and local SOCKS listener when configured.
pub fn check_applied_with_xray_artifact(
    config: &AppConfig,
    runner: &impl CommandRunner,
    require_tproxy: bool,
    singbox_config: Option<&Path>,
    geo_cache: Option<&Path>,
    xray_config: Option<&Path>,
) -> HealthSnapshot {
    let mut failed = Vec::new();
    let mut notes = Vec::new();
    let transport = if config.wireguard.enabled {
        TunnelTransport::Wireguard
    } else if xray_config.is_some_and(|path| path.is_file()) {
        TunnelTransport::Vless
    } else {
        TunnelTransport::None
    };
    let (wan_uplink, tunnel_uplink, mut link_notes) = probe_uplinks(config, runner, transport);
    notes.append(&mut link_notes);
    if require_tproxy && wan_uplink == "down" {
        failed.push("WAN gateway is unreachable".into());
    }
    let table_id = config.routing.policy_table_id.to_string();
    match runner.run(
        "nft",
        &["list", "table", "inet", &config.firewall.table_name],
    ) {
        Ok(out) if out.status == Some(0) && out.stdout.contains(&config.firewall.table_name) => {}
        Ok(out) if out.status == Some(0) => {
            failed.push(format!("nft table missing {}", config.firewall.table_name));
        }
        Ok(out) => failed.push(format!("nft table: {}", out.stderr)),
        Err(error) => failed.push(error.to_string()),
    }
    match runner.run("ip", &["rule", "show"]) {
        Ok(out) if out.status == Some(0) && out.stdout.contains(&table_id) => {}
        Ok(out) if out.status == Some(0) => {
            failed.push(format!("ip rule missing lookup {table_id}"));
        }
        Ok(out) => failed.push(format!("ip rule: {}", out.stderr)),
        Err(error) => failed.push(error.to_string()),
    }
    if require_tproxy {
        match runner.run("ip", &["route", "show", "table", &table_id]) {
            Ok(out)
                if out.status == Some(0)
                    && (out.stdout.contains("local 0.0.0.0/0")
                        || out.stdout.contains("local default")) => {}
            Ok(out) if out.status == Some(0) => {
                failed.push(format!(
                    "policy table {table_id} missing local default route"
                ));
            }
            Ok(out) => failed.push(format!("policy route: {}", out.stderr)),
            Err(error) => failed.push(error.to_string()),
        }
    }
    if config.wireguard.enabled && !config.wireguard.interface.is_empty() {
        match runner.run(
            "ip",
            &["-o", "link", "show", "dev", &config.wireguard.interface],
        ) {
            Ok(out)
                if out.status == Some(0) && out.stdout.contains(&config.wireguard.interface) => {}
            Ok(out) if out.status == Some(0) => {
                failed.push(format!(
                    "wireguard iface missing: {}",
                    config.wireguard.interface
                ));
            }
            Ok(out) => failed.push(format!("wg link: {}", out.stderr)),
            Err(error) => failed.push(error.to_string()),
        }
    }
    if require_tproxy {
        if let Some(path) = singbox_config {
            match runner.run(
                "sing-box",
                &["check", "-c", path.to_str().unwrap_or("sing-box.json")],
            ) {
                Ok(out) if out.status == Some(0) => {}
                Ok(out) => failed.push(format!("sing-box config invalid: {}", out.stderr)),
                Err(error) => failed.push(format!("sing-box config check: {error}")),
            }
        }
        let needle = format!(":{TPROXY_PORT}");
        let dns_needle = format!(":{DNS_PROXY_PORT}");
        match runner.run("ss", &["-lntu"]) {
            Ok(out)
                if out.status == Some(0)
                    && out.stdout.contains(&needle)
                    && (!config.dhcp.enabled || out.stdout.contains(&dns_needle)) => {}
            Ok(out) if out.status == Some(0) => {
                if !out.stdout.contains(&needle) {
                    failed.push(format!("tproxy {TPROXY_PORT} not listening"));
                }
                if config.dhcp.enabled && !out.stdout.contains(&dns_needle) {
                    failed.push(format!("sing-box DNS {DNS_PROXY_PORT} not listening"));
                }
            }
            Ok(out) => failed.push(format!("ss: {}", out.stderr)),
            Err(error) => failed.push(error.to_string()),
        }
        check_unit(runner, "gateway-kit-singbox", &mut failed);
        if let Some(path) = xray_config.filter(|path| path.is_file()) {
            match runner.run(
                "xray",
                &["run", "-test", "-c", path.to_str().unwrap_or("xray.json")],
            ) {
                Ok(out) if out.status == Some(0) => {}
                Ok(out) => failed.push(format!("Xray config invalid: {}", out.stderr)),
                Err(error) => failed.push(format!("Xray config check: {error}")),
            }
            check_unit(runner, "gateway-kit-xray", &mut failed);
            match runner.run("ss", &["-lntu"]) {
                Ok(out)
                    if out.status == Some(0)
                        && out
                            .stdout
                            .contains(&format!(":{}", gateway_model::XRAY_SOCKS_PORT)) => {}
                Ok(out) if out.status == Some(0) => failed.push(format!(
                    "Xray SOCKS {} not listening",
                    gateway_model::XRAY_SOCKS_PORT
                )),
                Ok(out) => failed.push(format!("Xray SOCKS listener check: {}", out.stderr)),
                Err(error) => failed.push(format!("Xray SOCKS listener check: {error}")),
            }
        }
        check_unit(runner, "gateway-kit-forwarding", &mut failed);
        if config.dhcp.enabled {
            check_unit(runner, "gateway-kit-dnsmasq", &mut failed);
        }
        if config.routing.china_direct {
            match geo_cache {
                Some(path) if path.is_file() && path.metadata().is_ok_and(|m| m.len() > 0) => {}
                Some(path) => failed.push(format!(
                    "GeoFile cache missing or empty: {}",
                    path.display()
                )),
                None => notes.push("GeoFile cache path was not supplied".into()),
            }
        }
    }
    if failed.is_empty() {
        HealthSnapshot {
            status: HealthStatus::Healthy,
            message: "gateway_kit table and policy rules reachable".into(),
            failed_checks: failed,
            notes,
            wan_uplink,
            tunnel_uplink,
        }
    } else {
        HealthSnapshot {
            status: HealthStatus::Unhealthy,
            message: "post-apply health failed".into(),
            failed_checks: failed,
            notes,
            wan_uplink,
            tunnel_uplink,
        }
    }
}

fn check_unit(runner: &impl CommandRunner, unit: &str, failed: &mut Vec<String>) {
    match runner.run("systemctl", &["is-active", "--quiet", unit]) {
        Ok(out) if out.status == Some(0) => {}
        Ok(out) => failed.push(format!("unit {unit} inactive: {}", out.stderr)),
        Err(error) => failed.push(format!("unit {unit}: {error}")),
    }
}

/// WAN gateway ping and tunnel reachability. Safe in observe mode.
pub fn probe_uplinks(
    config: &AppConfig,
    runner: &impl CommandRunner,
    transport: TunnelTransport,
) -> (String, String, Vec<String>) {
    let mut notes = Vec::new();
    let wan_uplink = if let Some(gw) = config
        .wan
        .gateway
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match runner.run("ping", &["-c", "1", "-W", "1", gw]) {
            Ok(out) if out.status == Some(0) => "up".into(),
            _ => {
                notes.push(format!("上级网关 {gw} ping 无应答"));
                "down".into()
            }
        }
    } else {
        "unknown".into()
    };
    let tunnel_uplink = match transport {
        TunnelTransport::Wireguard if !config.wireguard.interface.is_empty() => {
            match runner.run(
                "wg",
                &["show", &config.wireguard.interface, "latest-handshakes"],
            ) {
                Ok(out) if out.status == Some(0) && handshake_never(&out.stdout) => {
                    notes.push("wireguard has no handshake yet (VPS/endpoint may be down)".into());
                    "down".into()
                }
                Ok(out) if out.status == Some(0) => "up".into(),
                _ => "unknown".into(),
            }
        }
        TunnelTransport::Wireguard => "idle".into(),
        TunnelTransport::Vless => {
            // Xray owns the VLESS handshake; the post-apply check covers its service and SOCKS listener.
            "unknown".into()
        }
        TunnelTransport::None => "idle".into(),
    };
    (wan_uplink, tunnel_uplink, notes)
}

fn handshake_never(stdout: &str) -> bool {
    let body = stdout.trim();
    if body.is_empty() {
        return true;
    }
    body.lines().all(|line| {
        line.split_whitespace()
            .last()
            .is_none_or(|ts| ts == "0" || ts == "(none)")
    })
}

#[cfg(test)]
mod tests {
    use super::{EgressStatus, check_applied, check_applied_with_cache, probe_egress};
    use crate::command::ScriptedRunner;
    use gateway_model::{AppConfig, HealthStatus, TunnelTransport};

    #[test]
    fn egress_probe_reports_reachable_on_204() {
        let runner = ScriptedRunner::default();
        runner.push_ok("curl", "204");
        let report = probe_egress(&runner, TunnelTransport::Vless, "wg0");
        assert_eq!(report.status, EgressStatus::Reachable);
        assert!(report.detail.contains("VLESS"));
    }

    #[test]
    fn egress_probe_reports_unreachable_without_status() {
        let runner = ScriptedRunner::default();
        runner.push_ok("curl", "000");
        let report = probe_egress(&runner, TunnelTransport::Vless, "wg0");
        assert_eq!(report.status, EgressStatus::Unreachable);
    }

    #[test]
    fn egress_probe_is_unknown_without_a_transport() {
        let runner = ScriptedRunner::default();
        let report = probe_egress(&runner, TunnelTransport::None, "wg0");
        assert_eq!(report.status, EgressStatus::Unknown);
    }

    #[test]
    fn nonempty_ip_rule_must_mention_policy_table() {
        let runner = ScriptedRunner::succeeding();
        runner.push_ok("nft", "table inet gateway_kit");
        runner.push_ok("ip", "0: from all lookup main");
        let health = check_applied(&AppConfig::default(), &runner, false);
        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert!(health.failed_checks.iter().any(|c| c.contains("51820")));
    }

    #[test]
    fn production_health_requires_tproxy_listen() {
        let runner = ScriptedRunner::succeeding();
        runner.push_ok("nft", "table inet gateway_kit");
        runner.push_ok("ip", "fwmark 1 lookup 51820");
        runner.push_ok("ip", "local default dev lo scope host");
        runner.push_ok("ss", "tcp LISTEN 127.0.0.1:22");
        let health = check_applied(&AppConfig::default(), &runner, true);
        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert!(health.failed_checks.iter().any(|c| c.contains("tproxy")));
    }

    #[test]
    fn production_health_rejects_unreachable_wan_gateway() {
        let mut cfg = AppConfig::default();
        cfg.wan.gateway = Some("192.168.40.1".into());
        let runner = ScriptedRunner::succeeding();
        runner.push_unavailable("ping");
        let health = check_applied(&cfg, &runner, true);
        assert_eq!(health.status, HealthStatus::Unhealthy);
        assert!(
            health
                .failed_checks
                .iter()
                .any(|check| check.contains("WAN gateway"))
        );
    }

    #[test]
    fn production_health_requires_nonempty_geofile_cache() {
        let path = std::env::temp_dir().join(format!("gk-geo-cache-{}", std::process::id()));
        let runner = ScriptedRunner::succeeding();
        let missing = check_applied_with_cache(&AppConfig::default(), &runner, true, Some(&path));
        assert_eq!(missing.status, HealthStatus::Unhealthy);
        assert!(
            missing
                .failed_checks
                .iter()
                .any(|check| check.contains("GeoFile cache"))
        );

        std::fs::write(&path, "cached-rule-set").unwrap();
        let present = check_applied_with_cache(&AppConfig::default(), &runner, true, Some(&path));
        assert_eq!(present.status, HealthStatus::Healthy);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn wg_zero_handshake_is_note_not_failure() {
        let mut cfg = AppConfig::default();
        cfg.wireguard.enabled = true;
        cfg.wireguard.interface = "wg0".into();
        let runner = ScriptedRunner::succeeding();
        runner.push_ok("wg", "wg0\tabc=\t0");
        runner.push_ok("nft", "table inet gateway_kit");
        runner.push_ok("ip", "fwmark 1 lookup 51820");
        runner.push_ok("ip", "wg0: <POINTOPOINT,UP>");
        let health = check_applied(&cfg, &runner, false);
        assert_eq!(health.status, HealthStatus::Healthy);
        assert!(health.notes.iter().any(|n| n.contains("handshake")));
    }
}
