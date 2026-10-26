#[cfg(all(test, target_os = "linux"))]
mod linux_nft {
    use super::render_nftables;
    use gateway_model::AppConfig;
    use std::process::Command;

    #[test]
    fn nft_check_accepts_gateway_table() {
        let mut cfg = AppConfig {
            wan: gateway_model::InterfaceConfig {
                interface: "eth0".into(),
                ..Default::default()
            },
            lan: gateway_model::InterfaceConfig {
                interface: "eth1".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        cfg.port_forwards.push(gateway_model::PortForward {
            enabled: true,
            protocol: "tcp".into(),
            wan_port: 8080,
            lan_ip: "192.168.50.10".into(),
            lan_port: 80,
        });
        let body = render_nftables(&cfg);
        let path = std::env::temp_dir().join("gateway-kit-nft-check.conf");
        std::fs::write(&path, &body).expect("write nft fixture");
        let output = Command::new("nft")
            .args(["-c", "-f", path.to_str().expect("utf8 path")])
            .output();
        let output = match output {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                panic!("nft not installed; CI must apt-get install nftables");
            }
            Err(error) => panic!("nft: {error}"),
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("Operation not permitted") || stderr.contains("Permission denied") {
            eprintln!("skipping nft runtime check without required kernel permissions");
            return;
        }
        assert!(output.status.success(), "nft -c failed: {}", stderr);
    }
}
