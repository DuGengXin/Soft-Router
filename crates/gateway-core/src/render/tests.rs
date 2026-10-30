use super::{render_nftables, render_singbox, render_singbox_with_secrets};
use gateway_model::{AppConfig, Secrets};

#[test]
fn nftables_allows_routed_traffic_between_wan_and_lan() {
    let cfg = AppConfig {
        wan: gateway_model::InterfaceConfig {
            interface: "enp4s0".into(),
            address: Some("192.111.40.33/24".into()),
            gateway: Some("192.111.40.1".into()),
            ..Default::default()
        },
        lan: gateway_model::InterfaceConfig {
            interface: "enx00e04c413cc0".into(),
            address: Some("192.168.50.1/24".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    let nft = render_nftables(&cfg);
    assert!(
        nft.contains("iifname \"enx00e04c413cc0\" oifname \"enp4s0\" accept"),
        "LAN-to-WAN route missing: {nft}"
    );
    assert!(
        nft.contains("iifname \"enp4s0\" oifname \"enx00e04c413cc0\" accept"),
        "WAN-to-LAN route missing: {nft}"
    );
    assert!(
        nft.contains("oifname \"enp4s0\" masquerade"),
        "WAN-side SNAT must remain for Internet access: {nft}"
    );
}

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
        sets.iter().all(|s| s["update_interval"] == "1d"),
        "geo rule-sets must refresh periodically: {raw}"
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
fn singbox_routes_vless_proxy_uri_through_local_socks_without_wireguard() {
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
    assert_eq!(value["dns"]["final"], "direct-dns-0");
    assert_eq!(value["dns"]["strategy"], "ipv4_only");
    assert_eq!(value["dns"]["servers"][0]["tag"], "direct-dns-0");
    assert_eq!(value["dns"]["servers"][0]["type"], "udp");
    assert_eq!(value["dns"]["servers"][1]["tag"], "proxy-dns");
    assert_eq!(value["dns"]["servers"][1]["type"], "https");
    assert_eq!(value["dns"]["servers"][1]["server"], "1.1.1.1");
    assert_eq!(value["dns"]["servers"][1]["server_port"], 443);
    assert_eq!(value["dns"]["servers"][1]["path"], "/dns-query");
    assert_eq!(value["dns"]["servers"][1]["detour"], "proxy-out");
    assert_eq!(value["dns"]["rules"][0]["rule_set"][0], "geosite-cn");
    assert_eq!(value["dns"]["rules"][0]["rule_set"][1], "geosite-private");
    assert_eq!(value["dns"]["rules"][0]["server"], "direct-dns-0");
    assert_eq!(value["dns"]["rules"][1]["server"], "proxy-dns");
    assert_eq!(value["outbounds"][1]["type"], "socks");
    assert_eq!(value["outbounds"][1]["server"], "127.0.0.1");
    assert_eq!(value["outbounds"][1]["server_port"], 10808);
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
