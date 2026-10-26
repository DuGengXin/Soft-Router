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
        ui_lan_token: Some("token".into()),
        ..Secrets::default()
    };
    (cfg, secrets)
}

#[test]
fn default_is_observe_and_private() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.mode, OperatingMode::Observe);
    assert_eq!(cfg.firewall.table_name, NFT_TABLE);
    assert_eq!(cfg.ui.bind, "0.0.0.0");
    assert!(cfg.wan.dns.is_empty());
    assert!(cfg.validate(&Secrets::default()).is_err());
    let secrets = Secrets {
        ui_lan_token: Some("token".into()),
        ..Secrets::default()
    };
    assert!(cfg.validate(&secrets).is_ok());
}

#[test]
fn public_wan_warns() {
    let mut cfg = AppConfig::default();
    cfg.wan.address = Some("192.111.40.2/24".into());
    let secrets = Secrets {
        ui_lan_token: Some("token".into()),
        ..Secrets::default()
    };
    let issues = cfg.validate(&secrets).unwrap();
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
        ui_lan_token: Some("token".into()),
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
            contents: r#"{"outbounds":[{"uuid":"secret","tls":{"reality":{"public_key":"key"}}}]}"#
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
