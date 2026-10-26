use super::{render_singbox, render_singbox_with_secrets};
use gateway_model::{AppConfig, Secrets};

#[test]
fn singbox_json_parses() {
    let cfg = AppConfig {
        wan: gateway_model::InterfaceConfig {
            interface: "eth0".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let raw = render_singbox(&cfg);
    let value: serde_json::Value = serde_json::from_str(&raw).expect(&raw);
    assert_eq!(value["route"]["final"], "wg-out");
    assert_eq!(value["outbounds"][0]["bind_interface"], "eth0");
    assert_eq!(value["outbounds"][1]["bind_interface"], "wg0");
    assert_eq!(value["route"]["auto_detect_interface"], false);
    assert_eq!(
        value["experimental"]["cache_file"]["path"],
        "sing-box-cache.db"
    );
    let sets = value["route"]["rule_set"].as_array().expect("rule_set");
    assert!(
        sets.iter()
            .all(|s| s["url"].as_str().unwrap_or("").contains("jsdelivr.net")),
        "{raw}"
    );
    assert!(
        sets.iter().all(|s| !s["url"]
            .as_str()
            .unwrap_or("")
            .contains("githubusercontent")),
        "{raw}"
    );
    assert!(
        value["route"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.get("ip_cidr").is_some())
    );
}

#[test]
fn singbox_keeps_wan_prefix_when_extra_direct_empty() {
    let cfg = AppConfig {
        routing: gateway_model::RoutingConfig {
            china_direct: false,
            extra_direct_cidrs: vec![],
            ..Default::default()
        },
        ..Default::default()
    };
    let raw = render_singbox(&cfg);
    let value: serde_json::Value = serde_json::from_str(&raw).expect(&raw);
    let rules = value["route"]["rules"].as_array().expect("rules");
    let has_wan = rules.iter().any(|r| {
        r.get("ip_cidr")
            .and_then(|v| v.as_array())
            .is_some_and(|arr| arr.iter().any(|c| c.as_str() == Some("192.168.40.0/24")))
    });
    assert!(has_wan, "{raw}");
    assert_eq!(value["route"]["final"], "wg-out");
}

#[test]
fn singbox_renders_vless_reality_proxy_uri_without_wireguard() {
    let cfg = AppConfig::default();
    let secrets = Secrets {
            proxy_uri: Some("vless://11111111-1111-1111-1111-111111111111@example.com:443?sni=example.org&security=reality&pbk=public-key&sid=short-id&fp=chrome&flow=xtls-rprx-vision".into()),
            ..Secrets::default()
        };
    let raw = render_singbox_with_secrets(&cfg, &secrets).expect("valid proxy URI");
    let value: serde_json::Value = serde_json::from_str(&raw).expect(&raw);
    assert_eq!(value["route"]["final"], "proxy-out");
    assert!(value["inbounds"][0].get("sniff").is_none());
    assert_eq!(value["route"]["rules"][0]["action"], "hijack-dns");
    assert_eq!(value["route"]["rules"][1]["action"], "sniff");
    assert_eq!(value["dns"]["final"], "proxy-dns");
    assert_eq!(value["dns"]["strategy"], "ipv4_only");
    assert_eq!(value["dns"]["rules"][0]["rule_set"][0], "geosite-cn");
    assert_eq!(value["dns"]["rules"][0]["rule_set"][1], "geosite-private");
    assert_eq!(value["dns"]["rules"][0]["server"], "direct-dns-0");
    assert_eq!(value["dns"]["rules"][1]["server"], "proxy-dns");
    assert_eq!(value["outbounds"][1]["type"], "vless");
    assert_eq!(value["outbounds"][1]["server"], "example.com");
    assert_eq!(
        value["outbounds"][1]["uuid"],
        "11111111-1111-1111-1111-111111111111"
    );
    assert_eq!(
        value["outbounds"][1]["tls"]["reality"]["public_key"],
        "public-key"
    );
    assert_eq!(value["outbounds"][1]["flow"], "xtls-rprx-vision");
    assert!(value["outbounds"][1].get("bind_interface").is_none());
}

#[test]
fn singbox_rejects_malformed_proxy_uri() {
    let cfg = AppConfig::default();
    let secrets = Secrets {
        proxy_uri: Some("https://example.com/not-vless".into()),
        ..Secrets::default()
    };
    assert!(render_singbox_with_secrets(&cfg, &secrets).is_err());
}
