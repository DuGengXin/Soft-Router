//! Product-owned systemd unit lifecycle.

use crate::{CommandRunner, Paths};
use gateway_model::{AppConfig, Secrets};

pub(super) fn uses_product_systemd(paths: &Paths) -> bool {
    paths.manages_host_units()
}

pub(super) fn start_data_plane_units(
    paths: &Paths,
    config: &AppConfig,
    secrets: &Secrets,
    runner: &impl CommandRunner,
) {
    let transport = config.tunnel_transport(secrets);
    let use_xray = transport == gateway_model::TunnelTransport::Vless;
    if uses_product_systemd(paths) {
        let _ = runner.run("systemctl", &["enable", "gateway-kit-singbox"]);
        if use_xray {
            let _ = runner.run("systemctl", &["enable", "gateway-kit-xray"]);
        } else {
            let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-xray"]);
        }
        if config.dhcp.enabled {
            let _ = runner.run("systemctl", &["enable", "gateway-kit-dnsmasq"]);
        } else {
            let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-dnsmasq"]);
        }
    }
    if use_xray {
        let _ = runner.run("systemctl", &["restart", "gateway-kit-xray"]);
    }
    let _ = runner.run("systemctl", &["restart", "gateway-kit-singbox"]);
    if config.dhcp.enabled {
        let _ = runner.run("systemctl", &["restart", "gateway-kit-dnsmasq"]);
    }
}

pub(super) fn start_forwarding_unit(paths: &Paths, runner: &impl CommandRunner) {
    if !uses_product_systemd(paths) {
        return;
    }
    let _ = runner.run("systemctl", &["enable", "gateway-kit-forwarding"]);
    let _ = runner.run("systemctl", &["restart", "gateway-kit-forwarding"]);
}

pub(super) fn stop_data_plane_units(paths: &Paths, runner: &impl CommandRunner) {
    if uses_product_systemd(paths) {
        let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-forwarding"]);
        let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-singbox"]);
        let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-xray"]);
        let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-dnsmasq"]);
        return;
    }
    let _ = runner.run("systemctl", &["stop", "gateway-kit-singbox"]);
    let _ = runner.run("systemctl", &["stop", "gateway-kit-xray"]);
    let _ = runner.run("systemctl", &["stop", "gateway-kit-dnsmasq"]);
}
