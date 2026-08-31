//! Gateway-Kit CLI and agent.

mod http;
mod service;

use clap::{Parser, Subcommand};
use gateway_core::{
    Paths, SystemCommandRunner, check_applied_with_artifacts, default_or_load_config, disable,
    discover_host, init_database, latest_successful_generation, load_secrets, persist_preflight,
    record_event, restore_on_boot, save_config,
};
use gateway_model::{AppConfig, OperatingMode};
use service::AppService;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "gateway-kit",
    version,
    about = "Linux soft-router control plane"
)]
struct Cli {
    #[arg(long)]
    local: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Version,
    Doctor,
    Discover,
    Status,
    Plan,
    Apply {
        #[arg(long)]
        confirm: bool,
    },
    Rollback {
        #[arg(long)]
        confirm: bool,
    },
    Disable {
        #[arg(long)]
        confirm: bool,
    },
    Agent {
        #[arg(long)]
        once: bool,
        /// Override UI listen address, e.g. 127.0.0.1:7677
        #[arg(long)]
        listen: Option<String>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gateway-kit: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let paths = if cli.local {
        Paths::workspace()
    } else {
        Paths::production()
    };
    match cli.command {
        Command::Version => println!("gateway-kit {}", env!("CARGO_PKG_VERSION")),
        Command::Doctor => {
            let (config, report) = scan(&paths)?;
            println!(
                "doctor: mode={:?} observations={} conflicts={} blockers={}",
                config.mode,
                report.observations.len(),
                report.conflicts.len(),
                report.has_blockers()
            );
            for c in &report.conflicts {
                println!(
                    "- [{}] {}: {}",
                    format!("{:?}", c.severity).to_lowercase(),
                    c.title,
                    c.detail
                );
            }
        }
        Command::Discover => {
            let (_, report) = scan(&paths)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Status => {
            let (config, report) = scan(&paths)?;
            let payload = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "mode": config.mode,
                "ui": format!("{}:{}", config.ui.bind, config.ui.port),
                "blockers": report.has_blockers(),
            });
            println!("{payload}");
        }
        Command::Plan => {
            let plan = AppService::new(paths.clone()).plan()?;
            println!("{}", serde_json::to_string_pretty(&plan.redacted())?);
        }
        Command::Apply { confirm } => {
            let result = AppService::new(paths.clone()).apply(confirm)?;
            println!("apply: {} {}", result.generation_id, result.message);
        }
        Command::Rollback { confirm } => {
            let result = AppService::new(paths.clone()).rollback(confirm)?;
            println!("rollback: {}", result.message);
        }
        Command::Disable { confirm } => {
            let result = AppService::new(paths.clone()).disable(confirm)?;
            println!("disable: {}", result.message);
        }
        Command::Agent { once, listen } => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(agent_main(paths, once, listen))?;
        }
    }
    Ok(())
}

fn scan(
    paths: &Paths,
) -> Result<(AppConfig, gateway_model::PreflightReport), Box<dyn std::error::Error>> {
    let (config, _, report) = scan_full(paths)?;
    Ok((config, report))
}

fn scan_full(
    paths: &Paths,
) -> Result<
    (
        AppConfig,
        gateway_model::Secrets,
        gateway_model::PreflightReport,
    ),
    Box<dyn std::error::Error>,
> {
    let service = AppService::new(paths.clone());
    let result = service.load_all()?;
    let conn = init_database(&paths.state_db)?;
    record_event(&conn, "discover", "ok")?;
    Ok(result)
}

async fn agent_main(
    paths: Paths,
    once: bool,
    listen: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    paths.ensure_dirs()?;
    if !paths.config.exists() {
        save_config(&paths.config, &AppConfig::default())?;
    }
    let config = default_or_load_config(&paths.config)?;
    let secrets = load_secrets(&paths.secrets)?;
    let report = discover_host(&config, &SystemCommandRunner);
    let conn = init_database(&paths.state_db)?;
    persist_preflight(&conn, &report)?;

    let _ = restore_on_boot(&paths, &secrets, &SystemCommandRunner);

    if once {
        println!(
            "gateway-kit agent once: ui={}:{} mode={:?}",
            config.ui.bind, config.ui.port, config.mode
        );
        return Ok(());
    }
    let health_paths = paths.clone();
    tokio::spawn(async move {
        health_watch(health_paths).await;
    });
    http::serve(paths, listen).await?;
    Ok(())
}

async fn health_watch(paths: Paths) {
    let mut strikes = 0u8;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.tick().await;
    loop {
        interval.tick().await;
        let Ok(config) = default_or_load_config(&paths.config) else {
            continue;
        };
        if config.mode != OperatingMode::Gateway {
            strikes = 0;
            continue;
        }
        let Ok(conn) = init_database(&paths.state_db) else {
            continue;
        };
        if latest_successful_generation(&conn).ok().flatten().is_none() {
            strikes = 0;
            continue;
        }
        let geo_cache = paths.generated_dir.join("sing-box-cache.db");
        let singbox_config = paths.generated_dir.join("sing-box.json");
        let health = check_applied_with_artifacts(
            &config,
            &SystemCommandRunner,
            paths.manages_host_units(),
            Some(&singbox_config),
            Some(&geo_cache),
        );
        if health.status == gateway_model::HealthStatus::Healthy {
            strikes = 0;
            continue;
        }
        strikes = strikes.saturating_add(1);
        let _ = record_event(
            &conn,
            "health_fail",
            &format!("strike {strikes}: {}", health.failed_checks.join("; ")),
        );
        if strikes >= 3 {
            match gateway_core::rollback(
                &paths,
                &config,
                &load_secrets(&paths.secrets).unwrap_or_default(),
                &SystemCommandRunner,
                true,
            ) {
                Ok(result) => {
                    let _ = record_event(
                        &conn,
                        "health_rollback",
                        &format!("three failed health checks: {}", result.message),
                    );
                }
                Err(error) => {
                    let _ = disable(&paths, &config, &SystemCommandRunner, true);
                    let _ = record_event(
                        &conn,
                        "health_bypass",
                        &format!("rollback failed after three health checks: {error}"),
                    );
                }
            }
            strikes = 0;
        }
    }
}
