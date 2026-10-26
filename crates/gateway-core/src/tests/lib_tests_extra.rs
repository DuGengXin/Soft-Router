#![allow(clippy::field_reassign_with_default)]

use super::*;
use crate::command::ScriptedRunner;
use gateway_model::{OperatingMode, PlanStatus};

#[test]
fn gateway_plan_renders_split_routing() {
    let mut cfg = AppConfig::default();
    cfg.mode = OperatingMode::Gateway;
    cfg.wan.dns = vec!["192.0.2.53".into()];
    cfg.wan.interface = "eth0".into();
    cfg.lan.interface = "eth1".into();
    cfg.wireguard.enabled = true;
    cfg.wireguard.address = "10.66.0.2/32".into();
    cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("PRIV".into()),
        wireguard_peer_public_key: Some("PUB".into()),
        ui_lan_token: Some("token".into()),
        ..Default::default()
    };
    let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
    assert_eq!(plan.status, PlanStatus::Ready);
    let nft = plan
        .files
        .iter()
        .find(|f| f.relative_path.ends_with("nftables.conf"))
        .unwrap();
    assert!(nft.contents.contains("table inet gateway_kit"));
    assert!(nft.contents.contains("meta nfproto ipv6 drop"));
    assert!(nft.contents.contains("oifname \"eth0\" masquerade"));
    assert!(nft.contents.contains("tproxy ip to 127.0.0.1:7895"));
    assert!(!nft.contents.contains("policy drop"));
    assert!(nft.contents.contains("chain output"));
    assert!(nft.contents.contains("    tcp dport 7676 drop\n  }"));
    assert!(
        nft.contents
            .contains("iifname \"eth1\" tcp dport 7676 accept")
    );
    assert!(
        nft.contents
            .contains("iifname \"lo\" tcp dport 7676 accept")
    );
    assert!(
        nft.contents
            .contains("iifname \"eth0\" tcp dport 7676 drop")
    );
    assert!(
        nft.contents
            .contains("ip daddr { 192.168.50.1, 127.0.0.1 }")
    );
    assert!(!nft.contents.contains("tcp dport 22 return\n    tcp dport"));
    let redacted = plan.redacted();
    let wg_pub = redacted
        .files
        .iter()
        .find(|f| f.relative_path.ends_with("wg0.conf"))
        .unwrap();
    assert!(!wg_pub.contents.contains("PRIV"));
    let sb = plan
        .files
        .iter()
        .find(|f| f.relative_path.ends_with("sing-box.json"))
        .unwrap();
    assert!(sb.contents.contains("192.168.40.0/24"));
    assert!(sb.contents.contains("geoip-cn"));
    assert!(nft.contents.contains("ip daddr 192.168.40.0/24 return"));
    assert!(
        nft.contents
            .contains("iifname \"eth0\" oifname \"eth1\" accept")
    );
    assert!(sb.contents.contains("jsdelivr.net"));
    assert!(!sb.contents.contains("githubusercontent"));
    assert!(sb.contents.contains("wg-out"));
    assert!(sb.contents.contains("\"bind_interface\": \"eth0\""));
    assert!(sb.contents.contains("\"bind_interface\": \"wg0\""));
    assert!(sb.contents.contains("\"auto_detect_interface\": false"));
    let wg = plan
        .files
        .iter()
        .find(|f| f.relative_path.ends_with("wg0.conf"))
        .unwrap();
    assert!(wg.contents.contains("PRIV"));
    assert!(!plan.explanation.contains("PRIV"));
    let dhcp = plan
        .files
        .iter()
        .find(|f| f.relative_path.contains("dnsmasq"))
        .unwrap();
    assert!(dhcp.contents.contains("dhcp-option=3,192.168.50.1"));
    assert!(dhcp.contents.contains("dhcp-option=6,192.168.50.1"));
    assert!(dhcp.contents.contains("no-resolv"));
    assert!(dhcp.contents.contains("server=127.0.0.1#5353"));
    assert!(dhcp.contents.contains("listen-address=192.168.50.1"));
    assert!(nft.contents.contains("dport 53 return"));
    assert!(nft.contents.contains("chain prerouting_nat"));
    assert!(
        nft.contents
            .contains("iifname \"eth1\" udp dport 53 dnat ip to 192.168.50.1:53")
    );
    assert!(sb.contents.contains("\"dns-in\""));
    assert!(sb.contents.contains("\"proxy-dns\""));
    assert!(sb.contents.contains("\"strategy\": \"ipv4_only\""));
}

#[test]
fn china_direct_off_does_not_pull_geoip() {
    let mut cfg = AppConfig::default();
    cfg.mode = OperatingMode::Gateway;
    cfg.wan.dns = vec!["192.0.2.53".into()];
    cfg.wan.interface = "eth0".into();
    cfg.lan.interface = "eth1".into();
    cfg.routing.china_direct = false;
    cfg.wireguard.enabled = true;
    cfg.wireguard.address = "10.66.0.2/32".into();
    cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("PRIV".into()),
        wireguard_peer_public_key: Some("PUB".into()),
        ui_lan_token: Some("token".into()),
        ..Default::default()
    };
    let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
    let sb = plan
        .files
        .iter()
        .find(|f| f.relative_path.ends_with("sing-box.json"))
        .unwrap();
    assert!(!sb.contents.contains("geoip-cn"));
    assert!(!sb.contents.contains("geosite-cn"));
    assert!(!sb.contents.contains("geosite-private"));
    assert!(sb.contents.contains("\"final\": \"wg-out\""));
}

#[test]
fn blockers_stop_apply() {
    let mut cfg = AppConfig::default();
    cfg.mode = OperatingMode::Gateway;
    cfg.wan.dns = vec!["192.0.2.53".into()];
    cfg.wan.interface = "eth0".into();
    cfg.lan.interface = "eth1".into();
    cfg.wireguard.enabled = true;
    cfg.wireguard.address = "10.66.0.2/32".into();
    cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("PRIV".into()),
        wireguard_peer_public_key: Some("PUB".into()),
        ui_lan_token: Some("token".into()),
        ..Default::default()
    };
    let mut report = PreflightReport::default();
    report.conflicts.push(gateway_model::Conflict {
        id: "ufw".into(),
        severity: gateway_model::ConflictSeverity::Blocker,
        resource_id: "ufw".into(),
        title: "UFW".into(),
        detail: "on".into(),
        recommendation: "off".into(),
    });
    let plan = build_plan(&cfg, &secrets, &report).unwrap();
    assert_eq!(plan.status, PlanStatus::Blocked);
    let err = apply_plan(
        &Paths::workspace(),
        &cfg,
        &gateway_model::Secrets::default(),
        &plan,
        gateway_model::ApplyReason::UserConfirm,
        &ScriptedRunner::default(),
        false,
    )
    .unwrap_err();
    assert!(matches!(err, CoreError::Blocked(_)));
}

#[test]
fn confirm_apply_writes_and_can_disable() {
    let dir = std::env::temp_dir().join(format!("gk-test-{}", unix_now()));
    fs::create_dir_all(&dir).unwrap();
    let mut paths = Paths::workspace();
    paths.generated_dir = dir.join("generated");
    paths.backup_dir = dir.join("backups");
    paths.state_db = dir.join("state.db");
    paths.ensure_dirs().unwrap();
    let mut cfg = AppConfig::default();
    cfg.mode = OperatingMode::Gateway;
    cfg.wan.dns = vec!["192.0.2.53".into()];
    cfg.wan.interface = "eth0".into();
    cfg.lan.interface = "eth1".into();
    cfg.wireguard.enabled = true;
    cfg.wireguard.address = "10.66.0.2/32".into();
    cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("PRIV".into()),
        wireguard_peer_public_key: Some("PUB".into()),
        ui_lan_token: Some("token".into()),
        ..Default::default()
    };
    let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
    let runner = ScriptedRunner::succeeding();
    apply_plan(
        &paths,
        &cfg,
        &secrets,
        &plan,
        gateway_model::ApplyReason::UserConfirm,
        &runner,
        true,
    )
    .unwrap();
    assert!(paths.generated_dir.join("nftables.conf").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(paths.generated_dir.join("sing-box.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
    let calls = runner.calls.lock().unwrap_or_else(|e| e.into_inner());
    assert!(calls.iter().any(|(p, args)| {
        p == "ip"
            && args.first().map(String::as_str) == Some("rule")
            && args.get(1).map(String::as_str) == Some("add")
    }));
    assert!(
        !calls.iter().any(|(p, args)| {
            p == "wg"
                && args.first().map(String::as_str) == Some("syncconf")
                && args.iter().any(|a| a.ends_with("wg0.conf"))
        }),
        "wg syncconf must use stripped .sync.conf, not wg-quick Address/Table file"
    );
    assert!(
        !calls
            .iter()
            .any(|(p, args)| { p == "systemctl" && args.iter().any(|a| a == "enable") }),
        "workspace apply must not enable host systemd units"
    );
    drop(calls);
    disable(&paths, &cfg, &runner, true).unwrap();
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn apply_replaces_iface_addrs_not_main_default() {
    let dir = std::env::temp_dir().join(format!("gk-addr-{}", unix_now()));
    fs::create_dir_all(&dir).unwrap();
    let mut paths = Paths::workspace();
    paths.generated_dir = dir.join("generated");
    paths.backup_dir = dir.join("backups");
    paths.state_db = dir.join("state.db");
    paths.ensure_dirs().unwrap();
    let mut cfg = AppConfig::default();
    cfg.mode = OperatingMode::Gateway;
    cfg.wan.dns = vec!["192.0.2.53".into()];
    cfg.wan.interface = "eth0".into();
    cfg.lan.interface = "eth1".into();
    cfg.wireguard.enabled = true;
    cfg.wireguard.address = "10.66.0.2/32".into();
    cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("PRIV".into()),
        wireguard_peer_public_key: Some("PUB".into()),
        ui_lan_token: Some("token".into()),
        ..Default::default()
    };
    let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
    let runner = ScriptedRunner::succeeding();
    apply_plan(
        &paths,
        &cfg,
        &secrets,
        &plan,
        gateway_model::ApplyReason::UserConfirm,
        &runner,
        true,
    )
    .unwrap();
    let calls = runner.calls.lock().unwrap_or_else(|e| e.into_inner());
    assert!(calls.iter().any(|(p, args)| {
        p == "ip"
            && args.first().map(String::as_str) == Some("addr")
            && args.iter().any(|a| a == "eth1")
    }));
    assert!(calls.iter().any(|(p, args)| {
        p == "ip"
            && args.first().map(String::as_str) == Some("addr")
            && args.iter().any(|a| a == "eth0")
    }));
    assert!(!calls.iter().any(|(p, args)| {
        p == "ip"
            && args
                .windows(2)
                .any(|w| w[0] == "route" && w[1] == "default")
    }));
    assert!(
        calls
            .iter()
            .any(|(p, args)| { p == "ip" && args.iter().any(|a| a.ends_with("/32")) })
    );
    drop(calls);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn apply_installs_non_default_wg_routes_not_main_default() {
    let dir = std::env::temp_dir().join(format!("gk-wg-rt-{}", unix_now()));
    fs::create_dir_all(&dir).unwrap();
    let mut paths = Paths::workspace();
    paths.generated_dir = dir.join("generated");
    paths.backup_dir = dir.join("backups");
    paths.state_db = dir.join("state.db");
    paths.ensure_dirs().unwrap();
    let mut cfg = AppConfig::default();
    cfg.mode = OperatingMode::Gateway;
    cfg.wan.dns = vec!["192.0.2.53".into()];
    cfg.wan.interface = "eth0".into();
    cfg.lan.interface = "eth1".into();
    cfg.wireguard.enabled = true;
    cfg.wireguard.address = "10.66.0.2/32".into();
    cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    cfg.wireguard.peer_allowed_ips = "10.66.0.1/32,0.0.0.0/0".into();
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("PRIV".into()),
        wireguard_peer_public_key: Some("PUB".into()),
        ui_lan_token: Some("token".into()),
        ..Default::default()
    };
    let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
    let runner = ScriptedRunner::succeeding();
    apply_plan(
        &paths,
        &cfg,
        &secrets,
        &plan,
        gateway_model::ApplyReason::UserConfirm,
        &runner,
        true,
    )
    .unwrap();
    let calls = runner.calls.lock().unwrap_or_else(|e| e.into_inner());
    assert!(calls.iter().any(|(p, args)| {
        p == "ip"
            && args.windows(4).any(|w| {
                w[0] == "route" && w[1] == "replace" && w[2] == "10.66.0.1/32" && w[3] == "dev"
            })
    }));
    assert!(!calls.iter().any(|(p, args)| {
        p == "ip"
            && args
                .windows(2)
                .any(|w| w[0] == "route" && w[1] == "default")
    }));
    drop(calls);
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn boot_restore_replays_last_successful_generation() {
    let dir = std::env::temp_dir().join(format!("gk-boot-{}", unix_now()));
    fs::create_dir_all(&dir).unwrap();
    let mut paths = Paths::workspace();
    paths.config = dir.join("config.toml");
    paths.secrets = dir.join("secrets.toml");
    paths.generated_dir = dir.join("generated");
    paths.backup_dir = dir.join("backups");
    paths.state_db = dir.join("state.db");
    paths.ensure_dirs().unwrap();
    let mut cfg = AppConfig::default();
    cfg.mode = OperatingMode::Gateway;
    cfg.wan.dns = vec!["192.0.2.53".into()];
    cfg.wan.interface = "eth0".into();
    cfg.lan.interface = "eth1".into();
    cfg.wireguard.enabled = true;
    cfg.wireguard.address = "10.66.0.2/32".into();
    cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("PRIV".into()),
        wireguard_peer_public_key: Some("PUB".into()),
        ui_lan_token: Some("token".into()),
        ..Default::default()
    };
    crate::save_config(&paths.config, &cfg).unwrap();
    crate::save_secrets(&paths.secrets, &secrets).unwrap();
    let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
    let runner = ScriptedRunner::succeeding();
    apply_plan(
        &paths,
        &cfg,
        &secrets,
        &plan,
        gateway_model::ApplyReason::UserConfirm,
        &runner,
        true,
    )
    .unwrap();
    let restored = restore_on_boot(&paths, &secrets, &runner)
        .unwrap()
        .expect("boot restore");
    assert!(restored.message.contains("BootRestore"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn boot_recovery_bypasses_interrupted_apply_even_if_config_was_changed() {
    let dir = std::env::temp_dir().join(format!("gk-recovery-{}", unix_now()));
    fs::create_dir_all(&dir).unwrap();
    let mut paths = Paths::workspace();
    paths.config = dir.join("config.toml");
    paths.secrets = dir.join("secrets.toml");
    paths.generated_dir = dir.join("generated");
    paths.backup_dir = dir.join("backups");
    paths.state_db = dir.join("state.db");
    paths.ensure_dirs().unwrap();

    let mut interrupted = AppConfig::default();
    interrupted.mode = OperatingMode::Gateway;
    interrupted.wan.dns = vec!["192.0.2.53".into()];
    interrupted.wan.interface = "eth0".into();
    interrupted.lan.interface = "eth1".into();
    interrupted.wireguard.enabled = true;
    interrupted.wireguard.address = "10.66.0.2/32".into();
    interrupted.wireguard.peer_endpoint = "203.0.113.9:51820".into();
    crate::save_config(&paths.config, &AppConfig::default()).unwrap();
    let conn = init_database(&paths.state_db).unwrap();
    begin_apply_journal(&conn, "g-interrupted", &interrupted).unwrap();

    let result = restore_on_boot(
        &paths,
        &gateway_model::Secrets::default(),
        &ScriptedRunner::succeeding(),
    );
    assert!(result.is_err());
    let conn = init_database(&paths.state_db).unwrap();
    assert!(pending_apply_journal(&conn).unwrap().is_none());
    assert!(latest_successful_generation(&conn).unwrap().is_none());
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_secrets_does_not_log_and_unix_mode_is_private() {
    let dir = std::env::temp_dir().join(format!("gk-sec-{}", unix_now()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("secrets.toml");
    let secrets = gateway_model::Secrets {
        wireguard_private_key: Some("SUPERSECRET".into()),
        ..Default::default()
    };
    crate::save_secrets(&path, &secrets).unwrap();
    let raw = fs::read_to_string(&path).unwrap();
    assert!(raw.contains("SUPERSECRET"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn install_script_is_observe_only() {
    let sh = include_str!("../../../../packaging/install.sh");
    let unit = include_str!("../../../../packaging/gateway-kit.service");
    let wait = include_str!("../../../../packaging/gateway-kit-wait-network.sh");
    assert!(sh.contains("observe"));
    assert!(sh.contains("listening on 0.0.0.0:7676"));
    assert!(sh.contains("ui_lan_token"));
    assert!(sh.contains("ip -4 -o addr show"));
    assert!(sh.contains("gateway-kit-wait-network.sh"));
    assert!(unit.contains("EnvironmentFile=-/etc/gateway-kit/generated/forwarding.env"));
    assert!(unit.contains("ExecStartPre=/usr/local/libexec/gateway-kit-wait-network.sh"));
    assert!(unit.contains("Restart=on-failure"));
    assert!(wait.contains("LAN_IF"));
    assert!(wait.contains("WAN_IF"));
    assert!(wait.contains("ip link show dev"));
    assert!(!sh.contains("wg-quick"));
    assert!(!sh.contains("ip route"));
    assert!(!sh.contains("ip addr"));
}
