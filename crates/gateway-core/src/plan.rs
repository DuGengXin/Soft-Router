//! Build a ChangePlan without executing it.

use crate::CoreError;
use crate::render::{
    render_dnsmasq, render_forwarding_env, render_nftables, render_singbox_with_secrets,
    render_sysctl, render_wg_quick,
};
use gateway_model::{
    ActionKind, AppConfig, ChangePlan, ConflictPolicy, OperatingMode, PlanAction, PlanStatus,
    PreflightReport, RenderedFile, Secrets,
};

pub fn build_plan(
    config: &AppConfig,
    secrets: &Secrets,
    report: &PreflightReport,
) -> Result<ChangePlan, CoreError> {
    let issues = config.validate(secrets)?;
    if config.mode == OperatingMode::Observe {
        return Ok(ChangePlan {
            status: PlanStatus::ObserveOnly,
            explanation: "observe mode: no nft, routing, NAT, or tunnel changes".into(),
            issues,
            actions: Vec::new(),
            files: Vec::new(),
        });
    }
    if report.has_blockers() && config.environment.conflict_policy == ConflictPolicy::Block {
        let titles: Vec<_> = report
            .conflicts
            .iter()
            .filter(|c| c.severity == gateway_model::ConflictSeverity::Blocker)
            .map(|c| c.title.clone())
            .collect();
        return Ok(ChangePlan {
            status: PlanStatus::Blocked,
            explanation: format!("blocked by: {}", titles.join(", ")),
            issues,
            actions: Vec::new(),
            files: Vec::new(),
        });
    }

    let mut files = vec![
        RenderedFile {
            relative_path: "nftables.conf".into(),
            contents: render_nftables(config),
        },
        RenderedFile {
            relative_path: "sysctl.d/99-gateway-kit.conf".into(),
            contents: render_sysctl().into(),
        },
        RenderedFile {
            relative_path: "sing-box.json".into(),
            contents: render_singbox_with_secrets(config, secrets)?,
        },
        RenderedFile {
            relative_path: "forwarding.env".into(),
            contents: render_forwarding_env(config),
        },
    ];
    if config.wireguard.enabled {
        files.push(RenderedFile {
            relative_path: format!("{}.conf", config.wireguard.interface),
            contents: render_wg_quick(config, secrets),
        });
    }
    if config.dhcp.enabled {
        files.push(RenderedFile {
            relative_path: "dnsmasq.gateway-kit.conf".into(),
            contents: render_dnsmasq(config),
        });
    }

    let transport_action = if config.wireguard.enabled {
        PlanAction {
            id: "wg-sync".into(),
            kind: ActionKind::WgSync,
            summary: "sync WireGuard interface without replacing the main routing table".into(),
        }
    } else {
        PlanAction {
            id: "proxy-uri".into(),
            kind: ActionKind::Systemctl,
            summary: "start sing-box with the VLESS proxy_uri outbound".into(),
        }
    };
    let transport = if config.wireguard.enabled {
        "WireGuard wg-out"
    } else {
        "VLESS proxy-out"
    };
    let actions = vec![
        PlanAction {
            id: "write-generated".into(),
            kind: ActionKind::WriteFile,
            summary: format!("write {} generated file(s)", files.len()),
        },
        PlanAction {
            id: "iface-addr".into(),
            kind: ActionKind::IpRule,
            summary: "ip addr replace on named LAN/WAN (does not add a main-table default route)"
                .into(),
        },
        PlanAction {
            id: "sysctl-forward".into(),
            kind: ActionKind::Sysctl,
            summary: "install /etc/sysctl.d/99-gateway-kit.conf and apply forwarding".into(),
        },
        PlanAction {
            id: "nft-apply".into(),
            kind: ActionKind::NftApply,
            summary: "load table inet gateway_kit (does not rewrite host default route)".into(),
        },
        PlanAction {
            id: "docker-forwarding".into(),
            kind: ActionKind::Systemctl,
            summary: "install tagged LAN/WAN forwarding compatibility rules without flushing Docker chains".into(),
        },
        PlanAction {
            id: "policy-route".into(),
            kind: ActionKind::IpRule,
            summary: format!(
                "ensure ip rule to table {} for marked tunnel traffic",
                config.routing.policy_table_id
            ),
        },
        transport_action,
        PlanAction {
            id: "sing-box".into(),
            kind: ActionKind::Systemctl,
            summary: format!(
                "start sing-box with generated config (china direct / else {transport})"
            ),
        },
    ];

    Ok(ChangePlan {
        status: PlanStatus::Ready,
        explanation: format!(
            "LAN DNS is hijacked to sing-box; private/China traffic is direct via WAN, other traffic uses {transport}; host SSH/UI bypass tproxy"
        ),
        issues,
        actions,
        files,
    })
}
