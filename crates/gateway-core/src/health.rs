//! Post-apply health. Does not parse secrets.

use crate::command::CommandRunner;
use gateway_model::{AppConfig, DNS_PROXY_PORT, HealthSnapshot, HealthStatus, TPROXY_PORT};
use std::path::Path;

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
    let mut failed = Vec::new();
    let mut notes = Vec::new();
    let (wan_uplink, tunnel_uplink, mut link_notes) = probe_uplinks(config, runner);
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

/// WAN gateway ping and WireGuard handshake. Safe in observe mode.
pub fn probe_uplinks(
    config: &AppConfig,
    runner: &impl CommandRunner,
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
    let tunnel_uplink = if config.wireguard.enabled && !config.wireguard.interface.is_empty() {
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
    } else {
        "idle".into()
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
    use super::{check_applied, check_applied_with_cache};
    use crate::command::ScriptedRunner;
    use gateway_model::{AppConfig, HealthStatus};

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
