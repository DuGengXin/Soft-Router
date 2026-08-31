//! Apply, rollback, and disable. Commands stay injectable.

use crate::command::CommandRunner;
use crate::{
    CoreError, Paths, begin_apply_journal, clear_active_generation, commit_generation,
    init_database, insert_generation, latest_successful_generation, next_generation_id,
    pending_apply_journal, previous_successful_generation, record_event,
};
use gateway_model::{AppConfig, ApplyReason, ChangePlan, PlanStatus, Secrets};

const SYSCTL_DROPIN: &str = "/etc/sysctl.d/99-gateway-kit.conf";
use std::fs;

#[derive(Debug, Clone)]
pub struct ApplyResult {
    pub generation_id: String,
    pub message: String,
}

struct ApplyGuard<'a, R: CommandRunner> {
    paths: &'a Paths,
    config: &'a AppConfig,
    runner: &'a R,
    backup: Option<std::path::PathBuf>,
    committed: bool,
}

impl<R: CommandRunner> Drop for ApplyGuard<'_, R> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }

        // A failed apply must converge to the safe bypass state. The command
        // runner is intentionally best-effort here: the original apply error
        // remains the returned error, while cleanup attempts every operation.
        let _ = disable(self.paths, self.config, self.runner, true);
        if let Some(backup) = &self.backup {
            let _ = restore_generated(self.paths, backup);
        }
    }
}

pub fn apply_plan(
    paths: &Paths,
    config: &AppConfig,
    _secrets: &Secrets,
    plan: &ChangePlan,
    reason: ApplyReason,
    runner: &impl CommandRunner,
    execute: bool,
) -> Result<ApplyResult, CoreError> {
    match reason {
        ApplyReason::UserConfirm | ApplyReason::BootRestore => {}
        ApplyReason::Disable | ApplyReason::Rollback => {
            return Err(CoreError::Apply("use disable/rollback helpers".into()));
        }
    }
    if plan.status == PlanStatus::ObserveOnly {
        return Err(CoreError::Blocked(
            "observe mode refuses network mutation".into(),
        ));
    }
    if plan.status == PlanStatus::Blocked {
        return Err(CoreError::Blocked(plan.explanation.clone()));
    }
    if !execute {
        return Err(CoreError::Blocked(
            "apply requires --confirm or a prior successful generation on boot".into(),
        ));
    }
    paths.ensure_dirs()?;
    let id = next_generation_id();
    let journal_conn = init_database(&paths.state_db)?;
    begin_apply_journal(&journal_conn, &id, config)?;
    let backup = backup_generated(paths)?;
    let mut guard = ApplyGuard {
        paths,
        config,
        runner,
        backup,
        committed: false,
    };
    for file in &plan.files {
        let dest = paths.generated_dir.join(&file.relative_path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = file.contents.clone();
        let sensitive = file.relative_path.ends_with("sing-box.json")
            || (file.relative_path.ends_with(".conf") && file.contents.contains("PrivateKey"));
        fs::write(&dest, &body)?;
        #[cfg(unix)]
        if sensitive {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&dest, perms)?;
        }
        #[cfg(not(unix))]
        let _ = sensitive;
    }

    assign_address(runner, &config.lan.interface, config.lan.address.as_deref())?;
    assign_address(runner, &config.wan.interface, config.wan.address.as_deref())?;
    ensure_wan_gateway_onlink(runner, config)?;

    let sysctl_src = paths.generated_dir.join("sysctl.d/99-gateway-kit.conf");
    if sysctl_src.exists() {
        let src = sysctl_src.to_string_lossy().into_owned();
        let _ = runner.run("install", &["-D", "-m", "644", src.as_str(), SYSCTL_DROPIN]);
    }

    let nft = paths.generated_dir.join("nftables.conf");
    // nft 1.0.x does not support the `destroy table` statement in a ruleset
    // file. Delete only our table as a best-effort pre-step, then load the
    // complete declarative table. Other nft tables remain untouched.
    let _ = runner.run(
        "nft",
        &["delete", "table", "inet", &config.firewall.table_name],
    );
    run_required(
        runner,
        "nft",
        &["-f", nft.to_str().unwrap_or("nftables.conf")],
    )?;
    let _ = runner.run("sysctl", &["-w", "net.ipv4.ip_forward=1"]);
    let _ = runner.run("sysctl", &["-w", "net.ipv4.conf.all.route_localnet=1"]);
    let table_id = config.routing.policy_table_id.to_string();
    let mark = gateway_model::TPROXY_MARK.to_string();
    let _ = runner.run("ip", &["rule", "del", "fwmark", &mark, "lookup", &table_id]);
    run_required(
        runner,
        "ip",
        &["rule", "add", "fwmark", &mark, "lookup", &table_id],
    )?;
    run_required(
        runner,
        "ip",
        &[
            "route",
            "replace",
            "local",
            "0.0.0.0/0",
            "dev",
            "lo",
            "table",
            &table_id,
        ],
    )?;
    if config.wireguard.enabled {
        apply_wireguard(runner, paths, config)?;
    }
    start_forwarding_unit(paths, runner);
    start_data_plane_units(paths, config, runner);
    if paths.manages_host_units() {
        wait_for_tproxy(runner);
    }

    let geo_cache = paths.generated_dir.join("sing-box-cache.db");
    let singbox_config = paths.generated_dir.join("sing-box.json");
    let health = crate::check_applied_with_artifacts(
        config,
        runner,
        paths.manages_host_units(),
        Some(&singbox_config),
        Some(&geo_cache),
    );
    if health.status != gateway_model::HealthStatus::Healthy {
        return Err(CoreError::Apply(format!(
            "health failed: {}",
            health.failed_checks.join("; ")
        )));
    }

    let mut conn = init_database(&paths.state_db)?;
    commit_generation(&mut conn, &id, config, &plan.explanation)?;
    let _ = record_event(&conn, "apply", &id);
    guard.committed = true;
    Ok(ApplyResult {
        generation_id: id,
        message: format!("applied ({reason:?})"),
    })
}

pub fn disable(
    paths: &Paths,
    config: &AppConfig,
    runner: &impl CommandRunner,
    execute: bool,
) -> Result<ApplyResult, CoreError> {
    if !execute {
        return Err(CoreError::Blocked("disable requires --confirm".into()));
    }
    let _ = runner.run(
        "nft",
        &["delete", "table", "inet", &config.firewall.table_name],
    );
    let _ = runner.run("rm", &["-f", SYSCTL_DROPIN]);
    let table_id = config.routing.policy_table_id.to_string();
    let mark = gateway_model::TPROXY_MARK.to_string();
    let _ = runner.run("ip", &["rule", "del", "fwmark", &mark, "lookup", &table_id]);
    let _ = runner.run("ip", &["route", "flush", "table", &table_id]);
    if config.wireguard.enabled {
        let wg = paths
            .generated_dir
            .join(format!("{}.conf", config.wireguard.interface));
        let _ = runner.run("wg-quick", &["down", wg.to_str().unwrap_or("wg0.conf")]);
    }
    stop_data_plane_units(paths, runner);
    let mut conn = init_database(&paths.state_db)?;
    clear_active_generation(&mut conn)?;
    insert_generation(
        &conn,
        &next_generation_id(),
        "disabled",
        config,
        "emergency bypass",
    )?;
    let _ = record_event(&conn, "disable", "emergency bypass");
    Ok(ApplyResult {
        generation_id: "disabled".into(),
        message: "removed gateway_kit nft table and stopped product data-plane units".into(),
    })
}

pub fn restore_on_boot(
    paths: &Paths,
    secrets: &Secrets,
    runner: &impl CommandRunner,
) -> Result<Option<ApplyResult>, CoreError> {
    let config = crate::default_or_load_config(&paths.config)?;
    let conn = init_database(&paths.state_db)?;
    if let Some((id, json)) = pending_apply_journal(&conn)? {
        let interrupted = serde_json::from_str::<AppConfig>(&json).unwrap_or(config.clone());
        let _ = record_event(&conn, "apply_recovery", &id);
        let _ = disable(paths, &interrupted, runner, true);
        return Err(CoreError::Apply(format!(
            "recovered interrupted apply {id}; dataplane is in bypass"
        )));
    }
    if config.mode != gateway_model::OperatingMode::Gateway {
        return Ok(None);
    }
    let Some((_, json)) = latest_successful_generation(&conn)? else {
        return Ok(None);
    };
    let previous: AppConfig =
        serde_json::from_str(&json).map_err(|e| CoreError::ConfigParse(e.to_string()))?;
    let plan = crate::build_plan(&previous, secrets, &crate::discover_host(&previous, runner))?;
    if plan.status != gateway_model::PlanStatus::Ready {
        return Ok(None);
    }
    match apply_plan(
        paths,
        &previous,
        secrets,
        &plan,
        ApplyReason::BootRestore,
        runner,
        true,
    ) {
        Ok(result) => {
            let _ = record_event(&conn, "boot_restore", &result.message);
            Ok(Some(result))
        }
        Err(error) => {
            let _ = record_event(&conn, "boot_restore_failed", &error.to_string());
            let _ = disable(paths, &previous, runner, true);
            Err(error)
        }
    }
}

pub fn rollback(
    paths: &Paths,
    current: &AppConfig,
    secrets: &Secrets,
    runner: &impl CommandRunner,
    execute: bool,
) -> Result<ApplyResult, CoreError> {
    let conn = init_database(&paths.state_db)?;
    let Some((id, json)) = previous_successful_generation(&conn)? else {
        return disable(paths, current, runner, execute);
    };
    let previous: AppConfig =
        serde_json::from_str(&json).map_err(|e| CoreError::ConfigParse(e.to_string()))?;
    let report = crate::discover_host(&previous, runner);
    let plan = crate::build_plan(&previous, secrets, &report)?;
    apply_plan(
        paths,
        &previous,
        secrets,
        &plan,
        ApplyReason::BootRestore,
        runner,
        execute,
    )?;
    // The persisted desired configuration must follow the runtime generation;
    // otherwise the watchdog would inspect the rolled-back dataplane using
    // the newer, no-longer-active configuration.
    crate::save_config(&paths.config, &previous)?;
    Ok(ApplyResult {
        generation_id: id,
        message: "restored last successful generation".into(),
    })
}

fn uses_product_systemd(paths: &Paths) -> bool {
    paths.manages_host_units()
}

fn start_data_plane_units(paths: &Paths, config: &AppConfig, runner: &impl CommandRunner) {
    if uses_product_systemd(paths) {
        let _ = runner.run("systemctl", &["enable", "gateway-kit-singbox"]);
        if config.dhcp.enabled {
            let _ = runner.run("systemctl", &["enable", "gateway-kit-dnsmasq"]);
        } else {
            let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-dnsmasq"]);
        }
    }
    let _ = runner.run("systemctl", &["restart", "gateway-kit-singbox"]);
    if config.dhcp.enabled {
        let _ = runner.run("systemctl", &["restart", "gateway-kit-dnsmasq"]);
    }
}

fn start_forwarding_unit(paths: &Paths, runner: &impl CommandRunner) {
    if !uses_product_systemd(paths) {
        return;
    }
    let _ = runner.run("systemctl", &["enable", "gateway-kit-forwarding"]);
    let _ = runner.run("systemctl", &["restart", "gateway-kit-forwarding"]);
}

fn stop_data_plane_units(paths: &Paths, runner: &impl CommandRunner) {
    if uses_product_systemd(paths) {
        let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-forwarding"]);
        let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-singbox"]);
        let _ = runner.run("systemctl", &["disable", "--now", "gateway-kit-dnsmasq"]);
        return;
    }
    let _ = runner.run("systemctl", &["stop", "gateway-kit-singbox"]);
    let _ = runner.run("systemctl", &["stop", "gateway-kit-dnsmasq"]);
}

fn wait_for_tproxy(runner: &impl CommandRunner) {
    let needle = format!(":{}", gateway_model::TPROXY_PORT);
    for _ in 0..15 {
        if let Ok(out) = runner.run("ss", &["-lntu"])
            && out.status == Some(0)
            && out.stdout.contains(&needle)
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}

fn backup_generated(paths: &Paths) -> Result<Option<std::path::PathBuf>, CoreError> {
    if paths.generated_dir.exists() {
        let dest = paths.backup_dir.join(format!("gen-{}", crate::unix_now()));
        copy_dir(&paths.generated_dir, &dest)?;
        return Ok(Some(dest));
    }
    Ok(None)
}

fn restore_generated(paths: &Paths, backup: &std::path::Path) -> Result<(), CoreError> {
    if paths.generated_dir.exists() {
        fs::remove_dir_all(&paths.generated_dir)?;
    }
    copy_dir(backup, &paths.generated_dir)
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> Result<(), CoreError> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

fn run_required(
    runner: &impl CommandRunner,
    program: &str,
    args: &[&str],
) -> Result<(), CoreError> {
    match runner.run(program, args) {
        Ok(out) if out.status.unwrap_or(1) == 0 => Ok(()),
        Ok(out) => Err(CoreError::Apply(format!(
            "{program} failed: {}",
            out.stderr
        ))),
        Err(error) => Err(CoreError::Apply(error.to_string())),
    }
}

fn apply_wireguard(
    runner: &impl CommandRunner,
    paths: &Paths,
    config: &AppConfig,
) -> Result<(), CoreError> {
    let iface = config.wireguard.interface.as_str();
    let conf = paths.generated_dir.join(format!("{iface}.conf"));
    let conf_s = conf.to_str().unwrap_or("wg0.conf");
    let present = interface_present(runner, iface);
    if present {
        if let Some(stripped) = stripped_wg_conf(runner, paths, iface, conf_s) {
            let stripped_s = stripped.to_str().unwrap_or("wg0.sync.conf");
            if run_required(runner, "wg", &["syncconf", iface, stripped_s]).is_ok() {
                return install_wg_peer_routes(runner, config);
            }
        }
        let _ = runner.run("wg-quick", &["down", conf_s]);
    }
    run_required(runner, "wg-quick", &["up", conf_s])?;
    install_wg_peer_routes(runner, config)
}

fn install_wg_peer_routes(
    runner: &impl CommandRunner,
    config: &AppConfig,
) -> Result<(), CoreError> {
    let iface = config.wireguard.interface.as_str();
    for cidr in config.wireguard.peer_allowed_ips.split(',') {
        let cidr = cidr.trim();
        if cidr.is_empty() || cidr == "0.0.0.0/0" || cidr == "::/0" {
            continue;
        }
        run_required(runner, "ip", &["route", "replace", cidr, "dev", iface])?;
    }
    Ok(())
}

fn interface_present(runner: &impl CommandRunner, iface: &str) -> bool {
    match runner.run("ip", &["-o", "link", "show", "dev", iface]) {
        Ok(out) if out.status == Some(0) => out.stdout.contains(iface),
        _ => false,
    }
}

fn stripped_wg_conf(
    runner: &impl CommandRunner,
    paths: &Paths,
    iface: &str,
    conf: &str,
) -> Option<std::path::PathBuf> {
    let out = runner.run("wg-quick", &["strip", conf]).ok()?;
    if out.status != Some(0) || out.stdout.trim().is_empty() {
        return None;
    }
    let dest = paths.generated_dir.join(format!("{iface}.sync.conf"));
    fs::write(&dest, &out.stdout).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&dest) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = fs::set_permissions(&dest, perms);
        }
    }
    Some(dest)
}

fn assign_address(
    runner: &impl CommandRunner,
    iface: &str,
    address: Option<&str>,
) -> Result<(), CoreError> {
    let Some(address) = address.filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if iface.is_empty() {
        return Ok(());
    }
    run_required(runner, "ip", &["addr", "replace", address, "dev", iface])
}

fn ensure_wan_gateway_onlink(
    runner: &impl CommandRunner,
    config: &AppConfig,
) -> Result<(), CoreError> {
    let Some(gw) = config.wan.gateway.as_deref().filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if config.wan.interface.is_empty() {
        return Ok(());
    }
    let host = format!("{gw}/32");
    run_required(
        runner,
        "ip",
        &[
            "route",
            "replace",
            host.as_str(),
            "dev",
            &config.wan.interface,
        ],
    )
}
