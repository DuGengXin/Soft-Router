//! Discovery, rendering, apply, and SQLite state.

mod apply;
mod command;
mod db;
mod discovery;
mod health;
mod monitor;
mod plan;
mod render;

pub use apply::{ApplyResult, apply_plan, disable, restore_on_boot, rollback};
pub use command::{CommandOutput, CommandRunner, SystemCommandRunner};
pub(crate) use db::CURRENT_SCHEMA_VERSION;
pub use db::*;
pub use discovery::discover_host;
pub use health::{
    check_applied, check_applied_with_artifacts, check_applied_with_cache, probe_uplinks,
};
pub use monitor::sample_host;
pub use plan::build_plan;
pub use render::{
    render_dnsmasq, render_forwarding_env, render_nftables, render_singbox,
    render_singbox_with_secrets, render_wg_quick, validate_proxy_uri,
};

use gateway_model::{AppConfig, PreflightReport, ProbeStatus, Secrets};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

pub const APP_NAME: &str = "gateway-kit";

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("configuration file not found: {0}")]
    ConfigNotFound(PathBuf),
    #[error("configuration parse failed: {0}")]
    ConfigParse(String),
    #[error("validation failed: {0}")]
    Validate(#[from] gateway_model::ValidateError),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("unsupported database schema version: {0} (current: {CURRENT_SCHEMA_VERSION})")]
    UnsupportedSchema(i64),
    #[error("filesystem error: {0}")]
    Filesystem(#[from] std::io::Error),
    #[error("apply blocked: {0}")]
    Blocked(String),
    #[error("apply failed: {0}")]
    Apply(String),
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub config: PathBuf,
    pub secrets: PathBuf,
    pub state_db: PathBuf,
    pub runtime_dir: PathBuf,
    pub log_dir: PathBuf,
    pub backup_dir: PathBuf,
    pub generated_dir: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Self::from_root(PathBuf::from("/"))
    }
}

impl Paths {
    pub fn from_root(root: PathBuf) -> Self {
        let etc = root.join("etc/gateway-kit");
        let var = root.join("var/lib/gateway-kit");
        Self {
            config: etc.join("config.toml"),
            secrets: etc.join("secrets.toml"),
            state_db: var.join("state.db"),
            runtime_dir: root.join("run/gateway-kit"),
            log_dir: root.join("var/log/gateway-kit"),
            backup_dir: root.join("var/backups/gateway-kit"),
            generated_dir: etc.join("generated"),
            root,
        }
    }

    pub fn production() -> Self {
        let mut paths = Self::from_root(PathBuf::from("/"));
        if let Ok(config) = std::env::var("GATEWAY_KIT_CONFIG") {
            let config = PathBuf::from(config);
            if let Some(etc) = config.parent() {
                paths.secrets = etc.join("secrets.toml");
                paths.generated_dir = etc.join("generated");
                paths.config = config;
            }
        }
        paths
    }

    pub fn workspace() -> Self {
        Self {
            root: PathBuf::from("."),
            config: PathBuf::from("config.toml"),
            secrets: PathBuf::from("secrets.toml"),
            state_db: PathBuf::from("state.db"),
            runtime_dir: PathBuf::from("run"),
            log_dir: PathBuf::from("logs"),
            backup_dir: PathBuf::from("backups"),
            generated_dir: PathBuf::from("generated"),
        }
    }

    pub fn manages_host_units(&self) -> bool {
        self.config == Path::new("/etc/gateway-kit/config.toml")
    }

    pub fn ensure_dirs(&self) -> Result<(), CoreError> {
        for path in [
            &self.runtime_dir,
            &self.log_dir,
            &self.backup_dir,
            &self.generated_dir,
        ] {
            fs::create_dir_all(path)?;
        }
        if let Some(parent) = self.state_db.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = self.config.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/lib_tests_extra.rs"]
mod tests_extra;
