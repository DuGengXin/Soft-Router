//! Application-level orchestration shared by the CLI and HTTP API.

use gateway_core::{
    CoreError, Paths, SystemCommandRunner, apply_plan, build_plan, default_or_load_config, disable,
    discover_host, init_database, load_secrets, persist_preflight, rollback, save_config,
};
use gateway_model::{AppConfig, ApplyReason, ChangePlan, PreflightReport, Secrets};

/// Coordinates persistent configuration, discovery, planning, and mutations.
#[derive(Clone)]
pub(crate) struct AppService {
    paths: Paths,
}

impl AppService {
    pub(crate) fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub(crate) fn load_all(&self) -> Result<(AppConfig, Secrets, PreflightReport), CoreError> {
        self.paths.ensure_dirs()?;
        let config = default_or_load_config(&self.paths.config)?;
        let secrets = load_secrets(&self.paths.secrets)?;
        let report = discover_host(&config, &SystemCommandRunner);
        let conn = init_database(&self.paths.state_db)?;
        persist_preflight(&conn, &report)?;
        Ok((config, secrets, report))
    }

    pub(crate) fn plan(&self) -> Result<ChangePlan, CoreError> {
        let (config, secrets, report) = self.load_all()?;
        build_plan(&config, &secrets, &report)
    }

    pub(crate) fn apply(&self, confirm: bool) -> Result<gateway_core::ApplyResult, CoreError> {
        let (config, secrets, report) = self.load_all()?;
        let plan = build_plan(&config, &secrets, &report)?;
        apply_plan(
            &self.paths,
            &config,
            &secrets,
            &plan,
            ApplyReason::UserConfirm,
            &SystemCommandRunner,
            confirm,
        )
    }

    pub(crate) fn rollback(&self, confirm: bool) -> Result<gateway_core::ApplyResult, CoreError> {
        let (config, secrets, _) = self.load_all()?;
        rollback(
            &self.paths,
            &config,
            &secrets,
            &SystemCommandRunner,
            confirm,
        )
    }

    pub(crate) fn disable(&self, confirm: bool) -> Result<gateway_core::ApplyResult, CoreError> {
        let (config, _, _) = self.load_all()?;
        disable(&self.paths, &config, &SystemCommandRunner, confirm)
    }

    pub(crate) fn save_config(&self, config: &AppConfig) -> Result<(), CoreError> {
        save_config(&self.paths.config, config)
    }
}
