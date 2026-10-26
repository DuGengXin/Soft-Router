use gateway_model::AppConfig;

/// Render the interface contract consumed by the Docker FORWARD compatibility unit.
pub fn render_forwarding_env(config: &AppConfig) -> String {
    format!(
        "LAN_IF={}\nWAN_IF={}\n",
        config.lan.interface, config.wan.interface
    )
}

pub fn render_sysctl() -> &'static str {
    "net.ipv4.ip_forward=1
net.ipv4.conf.all.route_localnet=1
"
}
