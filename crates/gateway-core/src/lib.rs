//! Discovery, rendering, apply, and SQLite state.

mod apply;
mod command;
mod discovery;
mod health;
mod monitor;
mod plan;
mod render;

pub use apply::{ApplyResult, apply_plan, disable, restore_on_boot, rollback};
pub use command::{CommandOutput, CommandRunner, SystemCommandRunner};
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

pub fn load_config(path: &Path) -> Result<AppConfig, CoreError> {
    let raw = fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CoreError::ConfigNotFound(path.to_path_buf())
        } else {
            CoreError::Filesystem(e)
        }
    })?;
    toml::from_str(&raw).map_err(|e| CoreError::ConfigParse(e.to_string()))
}

pub fn default_or_load_config(path: &Path) -> Result<AppConfig, CoreError> {
    if path.exists() {
        load_config(path)
    } else {
        Ok(AppConfig::default())
    }
}

pub fn load_secrets(path: &Path) -> Result<Secrets, CoreError> {
    if !path.exists() {
        return Ok(Secrets::default());
    }
    let raw = fs::read_to_string(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        if perms.mode() & 0o077 != 0 {
            perms.set_mode(0o600);
            fs::set_permissions(path, perms)?;
        }
    }
    toml::from_str(&raw).map_err(|e| CoreError::ConfigParse(e.to_string()))
}

pub fn save_config(path: &Path, config: &AppConfig) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        path,
        toml::to_string_pretty(config).map_err(|e| CoreError::ConfigParse(e.to_string()))?,
    )?;
    Ok(())
}

pub fn save_secrets(path: &Path, secrets: &Secrets) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let body =
        toml::to_string_pretty(secrets).map_err(|e| CoreError::ConfigParse(e.to_string()))?;
    fs::write(path, body)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

pub fn init_database(path: &Path) -> Result<Connection, CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON; CREATE TABLE IF NOT EXISTS schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);")?;
    migrate_database(&conn)?;
    Ok(conn)
}

const CURRENT_SCHEMA_VERSION: i64 = 3;

fn migrate_database(conn: &Connection) -> Result<(), CoreError> {
    let version: i64 = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .and_then(|value: String| value.parse().ok())
        .unwrap_or(0);

    if version < 2 {
        migrate_to_v2(conn)?;
    }
    if version < 3 {
        migrate_to_v3(conn)?;
    }
    if version > CURRENT_SCHEMA_VERSION {
        return Err(CoreError::UnsupportedSchema(version));
    }
    Ok(())
}

fn migrate_to_v2(conn: &Connection) -> Result<(), CoreError> {
    // The pre-v2 generations table stored a hash rather than a replayable
    // config snapshot. Archive it instead of fabricating an unsafe rollback
    // target; successful generations will be recorded in the new table.
    if table_exists(conn, "generations")? && !has_column(conn, "generations", "config_json")? {
        conn.execute(
            "ALTER TABLE generations RENAME TO generations_legacy_v1",
            [],
        )?;
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS generations (
            id TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            config_json TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            details TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS resources (
            resource_id TEXT PRIMARY KEY,
            resource_type TEXT NOT NULL,
            ownership TEXT NOT NULL,
            owner TEXT NOT NULL,
            summary TEXT NOT NULL,
            last_seen INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS conflicts (
            id TEXT PRIMARY KEY,
            severity TEXT NOT NULL,
            resource_id TEXT NOT NULL,
            title TEXT NOT NULL,
            detail TEXT NOT NULL,
            recommendation TEXT NOT NULL,
            first_seen INTEGER NOT NULL DEFAULT 0,
            last_seen INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS probe_results (
            name TEXT PRIMARY KEY,
            status TEXT NOT NULL,
            detail TEXT NOT NULL,
            last_seen INTEGER NOT NULL
         );",
    )?;
    if table_exists(conn, "conflicts")? && !has_column(conn, "conflicts", "first_seen")? {
        conn.execute(
            "ALTER TABLE conflicts ADD COLUMN first_seen INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if table_exists(conn, "conflicts")? && !has_column(conn, "conflicts", "last_seen")? {
        conn.execute(
            "ALTER TABLE conflicts ADD COLUMN last_seen INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    Ok(())
}

fn migrate_to_v3(conn: &Connection) -> Result<(), CoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS runtime_state (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
         );",
    )?;
    conn.execute(
        "INSERT INTO schema_meta(key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![CURRENT_SCHEMA_VERSION.to_string()],
    )?;
    let active_generation_missing = conn
        .query_row(
            "SELECT value FROM runtime_state WHERE key = 'active_generation'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_none();
    if active_generation_missing && let Some((id, _)) = latest_successful_generation_legacy(conn)? {
        conn.execute(
            "INSERT INTO runtime_state(key, value, updated_at) VALUES ('active_generation', ?1, ?2)",
            params![id, unix_now()],
        )?;
    }
    Ok(())
}

fn latest_successful_generation_legacy(
    conn: &Connection,
) -> Result<Option<(String, String)>, CoreError> {
    Ok(conn
        .query_row(
            "SELECT id, config_json FROM generations WHERE status = 'success' ORDER BY created_at DESC, id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, CoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![table],
        |row| row.get(0),
    )?)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, CoreError> {
    let mut stmt = conn.prepare("SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2 LIMIT 1")?;
    Ok(stmt
        .query_row(params![table, column], |_| Ok(()))
        .optional()?
        .is_some())
}

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

pub fn next_generation_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("g{}-{}", now.as_secs(), now.subsec_nanos())
}

pub fn persist_preflight(conn: &Connection, report: &PreflightReport) -> Result<(), CoreError> {
    let now = unix_now();
    let tx = conn.unchecked_transaction()?;
    for item in &report.observations {
        tx.execute(
            "INSERT INTO resources(resource_id, resource_type, ownership, owner, summary, last_seen)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(resource_id) DO UPDATE SET
               resource_type=excluded.resource_type, ownership=excluded.ownership,
               owner=excluded.owner, summary=excluded.summary, last_seen=excluded.last_seen",
            params![
                item.resource_id,
                format!("{:?}", item.resource_type),
                format!("{:?}", item.ownership),
                format!("{:?}", item.owner),
                item.summary,
                now
            ],
        )?;
    }
    for conflict in &report.conflicts {
        tx.execute(
            "INSERT INTO conflicts(id, severity, resource_id, title, detail, recommendation, first_seen, last_seen)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?7)
             ON CONFLICT(id) DO UPDATE SET
               severity=excluded.severity, detail=excluded.detail,
               recommendation=excluded.recommendation, last_seen=excluded.last_seen",
            params![
                conflict.id,
                format!("{:?}", conflict.severity),
                conflict.resource_id,
                conflict.title,
                conflict.detail,
                conflict.recommendation,
                now
            ],
        )?;
    }
    for probe in &report.probes {
        let status = match probe.status {
            ProbeStatus::Detected => "detected",
            ProbeStatus::NotPresent => "not_present",
            ProbeStatus::Unavailable => "unavailable",
            ProbeStatus::Failed => "failed",
        };
        tx.execute(
            "INSERT INTO probe_results(name, status, detail, last_seen)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(name) DO UPDATE SET status=excluded.status, detail=excluded.detail, last_seen=excluded.last_seen",
            params![probe.name, status, probe.detail, now],
        )?;
    }
    tx.commit()?;
    Ok(())
}

pub fn record_event(conn: &Connection, kind: &str, payload: &str) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO events(kind, payload, created_at) VALUES (?1,?2,?3)",
        params![kind, payload, unix_now()],
    )?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EventRecord {
    pub kind: String,
    pub payload: String,
    pub created_at: i64,
}

pub fn recent_events(conn: &Connection, limit: i64) -> Result<Vec<EventRecord>, CoreError> {
    let mut stmt = conn.prepare(
        "SELECT kind, payload, created_at FROM events ORDER BY created_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |row| {
        Ok(EventRecord {
            kind: row.get(0)?,
            payload: row.get(1)?,
            created_at: row.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn last_successful_generation(
    conn: &Connection,
) -> Result<Option<(String, String)>, CoreError> {
    latest_successful_generation(conn)
}

/// Return the most recently committed successful generation.
pub fn latest_successful_generation(
    conn: &Connection,
) -> Result<Option<(String, String)>, CoreError> {
    let active = conn
        .query_row(
            "SELECT value FROM runtime_state WHERE key = 'active_generation'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(id) = active {
        return Ok(conn
            .query_row(
                "SELECT id, config_json FROM generations WHERE id = ?1 AND status = 'success'",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?);
    }
    Ok(conn
        .query_row(
            "SELECT id, config_json FROM generations WHERE status = 'success' ORDER BY created_at DESC, id DESC LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

/// Return the successful generation immediately before the active one.
///
/// The latest generation is used for boot restore. Rollback must deliberately
/// select the previous generation so that invoking rollback after a successful
/// apply actually changes the system back to the prior configuration.
pub fn previous_successful_generation(
    conn: &Connection,
) -> Result<Option<(String, String)>, CoreError> {
    let active = conn
        .query_row(
            "SELECT value FROM runtime_state WHERE key = 'active_generation'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if let Some(active_id) = active {
        if active_id.is_empty() {
            return Ok(None);
        }
        return Ok(conn
            .query_row(
                "SELECT id, config_json FROM generations WHERE status = 'success' AND id <> ?1 ORDER BY created_at DESC, id DESC LIMIT 1",
                params![active_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?);
    }
    Ok(conn
        .query_row(
            "SELECT id, config_json FROM generations WHERE status = 'success' ORDER BY created_at DESC, id DESC LIMIT 1 OFFSET 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

pub fn insert_generation(
    conn: &Connection,
    id: &str,
    status: &str,
    config: &AppConfig,
    details: &str,
) -> Result<(), CoreError> {
    let json = serde_json::to_string(config).map_err(|e| CoreError::ConfigParse(e.to_string()))?;
    conn.execute(
        "INSERT INTO generations(id, status, config_json, created_at, details) VALUES (?1,?2,?3,?4,?5)",
        params![id, status, json, unix_now(), details],
    )?;
    Ok(())
}

/// Persist the in-flight apply before touching the host. This is the durable
/// recovery point used when the process is killed during a dataplane change.
pub fn begin_apply_journal(
    conn: &Connection,
    id: &str,
    config: &AppConfig,
) -> Result<(), CoreError> {
    let json = serde_json::to_string(config).map_err(|e| CoreError::ConfigParse(e.to_string()))?;
    conn.execute(
        "INSERT INTO runtime_state(key, value, updated_at) VALUES ('pending_apply', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![format!("{id}\n{json}"), unix_now()],
    )?;
    Ok(())
}

/// Return the durable apply journal, if a previous process stopped mid-apply.
pub fn pending_apply_journal(conn: &Connection) -> Result<Option<(String, String)>, CoreError> {
    let Some(value) = conn
        .query_row(
            "SELECT value FROM runtime_state WHERE key = 'pending_apply'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    let Some((id, json)) = value.split_once('\n') else {
        return Ok(Some((value, String::new())));
    };
    Ok(Some((id.to_string(), json.to_string())))
}

fn clear_pending_apply(conn: &Connection) -> Result<(), CoreError> {
    conn.execute("DELETE FROM runtime_state WHERE key = 'pending_apply'", [])?;
    Ok(())
}

/// Atomically record a successful generation and make it the active runtime generation.
pub fn commit_generation(
    conn: &mut Connection,
    id: &str,
    config: &AppConfig,
    details: &str,
) -> Result<(), CoreError> {
    let json = serde_json::to_string(config).map_err(|e| CoreError::ConfigParse(e.to_string()))?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO generations(id, status, config_json, created_at, details) VALUES (?1,'success',?2,?3,?4)",
        params![id, json, unix_now(), details],
    )?;
    tx.execute(
        "INSERT INTO runtime_state(key, value, updated_at) VALUES ('active_generation', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![id, unix_now()],
    )?;
    tx.execute("DELETE FROM runtime_state WHERE key = 'pending_apply'", [])?;
    tx.commit()?;
    Ok(())
}

/// Clear the active generation after an emergency bypass.
pub fn clear_active_generation(conn: &mut Connection) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO runtime_state(key, value, updated_at) VALUES ('active_generation', '', ?1)
         ON CONFLICT(key) DO UPDATE SET value = '', updated_at = excluded.updated_at",
        params![unix_now()],
    )?;
    clear_pending_apply(conn)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::command::ScriptedRunner;
    use gateway_model::{ChangePlan, OperatingMode, PlanStatus, RenderedFile};

    #[test]
    fn example_config_parses_as_observe() {
        let raw = include_str!("../../../config.example.toml");
        let config: AppConfig = toml::from_str(raw).expect("example");
        assert_eq!(config.mode, OperatingMode::Observe);
        assert_eq!(config.firewall.table_name, "gateway_kit");
        assert_eq!(config.routing.policy_table_id, 51820);
        config.validate(&gateway_model::Secrets::default()).unwrap();
    }

    #[test]
    fn latest_and_previous_generations_are_distinct() {
        let dir = std::env::temp_dir().join(format!("gk-generation-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("state.db");
        let conn = init_database(&db).unwrap();
        let mut first = AppConfig::default();
        first.system.hostname = "first".into();
        let mut second = first.clone();
        second.system.hostname = "second".into();
        insert_generation(&conn, "g-first", "success", &first, "first").unwrap();
        insert_generation(&conn, "g-second", "success", &second, "second").unwrap();

        let latest = latest_successful_generation(&conn).unwrap().unwrap();
        let previous = previous_successful_generation(&conn).unwrap().unwrap();
        assert_eq!(latest.0, "g-second");
        assert_eq!(previous.0, "g-first");
        assert_ne!(latest.1, previous.1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn commit_generation_updates_active_pointer_atomically() {
        let dir = std::env::temp_dir().join(format!("gk-active-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let db = dir.join("state.db");
        let mut conn = init_database(&db).unwrap();
        let mut first = AppConfig::default();
        first.system.hostname = "first".into();
        let mut second = first.clone();
        second.system.hostname = "second".into();
        begin_apply_journal(&conn, "g-pending", &second).unwrap();
        assert_eq!(
            pending_apply_journal(&conn).unwrap().unwrap().0,
            "g-pending"
        );
        commit_generation(&mut conn, "g-first", &first, "first").unwrap();
        commit_generation(&mut conn, "g-second", &second, "second").unwrap();

        assert_eq!(
            latest_successful_generation(&conn).unwrap().unwrap().0,
            "g-second"
        );
        assert_eq!(
            previous_successful_generation(&conn).unwrap().unwrap().0,
            "g-first"
        );
        let active: String = conn
            .query_row(
                "SELECT value FROM runtime_state WHERE key = 'active_generation'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, "g-second");
        assert!(pending_apply_journal(&conn).unwrap().is_none());
        clear_active_generation(&mut conn).unwrap();
        assert!(latest_successful_generation(&conn).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_apply_enters_bypass_and_restores_generated_files() {
        let dir = std::env::temp_dir().join(format!("gk-apply-fail-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let mut paths = Paths::workspace();
        paths.generated_dir = dir.join("generated");
        paths.backup_dir = dir.join("backups");
        paths.state_db = dir.join("state.db");
        paths.ensure_dirs().unwrap();
        fs::write(paths.generated_dir.join("sentinel.txt"), "old generation").unwrap();

        let plan = ChangePlan {
            status: PlanStatus::Ready,
            explanation: "test failure".into(),
            issues: Vec::new(),
            actions: Vec::new(),
            files: vec![RenderedFile {
                relative_path: "sentinel.txt".into(),
                contents: "new generation".into(),
            }],
        };
        let runner = ScriptedRunner::default();
        runner.push_unavailable("nft");
        let result = apply_plan(
            &paths,
            &AppConfig::default(),
            &Secrets::default(),
            &plan,
            gateway_model::ApplyReason::UserConfirm,
            &runner,
            true,
        );
        assert!(result.is_err());
        assert_eq!(
            fs::read_to_string(paths.generated_dir.join("sentinel.txt")).unwrap(),
            "old generation"
        );
        assert!(
            runner
                .calls
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .any(|(program, args)| program == "nft" && args.starts_with(&["delete".into()]))
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn workspace_paths_are_relative() {
        let paths = Paths::workspace();
        assert_eq!(paths.config, PathBuf::from("config.toml"));
    }

    #[test]
    fn production_paths_live_under_etc() {
        let paths = Paths::from_root(PathBuf::from("/"));
        assert!(
            paths.config.ends_with("etc/gateway-kit/config.toml")
                || paths.config.ends_with("etc\\gateway-kit\\config.toml")
        );
        assert!(paths.generated_dir.ends_with("generated"));
    }

    #[test]
    fn preflight_persists() {
        let conn = init_database(Path::new(":memory:")).unwrap();
        let report = discover_host(&AppConfig::default(), &ScriptedRunner::default());
        persist_preflight(&conn, &report).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM probe_results", [], |r| r.get(0))
            .unwrap();
        assert!(count >= 1);
    }

    #[test]
    fn database_migration_archives_legacy_generation_schema() {
        let dir = std::env::temp_dir().join(format!("gk-migrate-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE schema_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO schema_meta(key, value) VALUES ('schema_version', '1');
             CREATE TABLE generations (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                config_hash TEXT,
                created_at INTEGER NOT NULL,
                details TEXT NOT NULL
             );
             CREATE TABLE conflicts (
                id TEXT PRIMARY KEY,
                severity TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                title TEXT NOT NULL,
                detail TEXT NOT NULL,
                recommendation TEXT NOT NULL
             );",
        )
        .unwrap();
        drop(conn);

        let conn = init_database(&path).unwrap();
        assert!(table_exists(&conn, "generations_legacy_v1").unwrap());
        assert!(has_column(&conn, "generations", "config_json").unwrap());
        assert!(has_column(&conn, "conflicts", "first_seen").unwrap());
        assert!(has_column(&conn, "conflicts", "last_seen").unwrap());
        let version: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version, "3");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn persist_preflight_accepts_legacy_conflicts_first_seen() {
        let dir = std::env::temp_dir().join(format!("gk-db-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE conflicts (
                id TEXT PRIMARY KEY,
                severity TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                title TEXT NOT NULL,
                detail TEXT NOT NULL,
                recommendation TEXT NOT NULL,
                first_seen INTEGER NOT NULL,
                last_seen INTEGER NOT NULL
            );",
        )
        .unwrap();
        drop(conn);
        let conn = init_database(&path).unwrap();
        let mut report = PreflightReport::default();
        report.conflicts.push(gateway_model::Conflict {
            id: "missing-sing-box".into(),
            severity: gateway_model::ConflictSeverity::Warning,
            resource_id: "bin:sing-box".into(),
            title: "sing-box".into(),
            detail: "missing".into(),
            recommendation: "install".into(),
        });
        persist_preflight(&conn, &report).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn observe_plan_is_observe_only() {
        let plan = build_plan(
            &AppConfig::default(),
            &gateway_model::Secrets::default(),
            &PreflightReport::default(),
        )
        .unwrap();
        assert_eq!(plan.status, PlanStatus::ObserveOnly);
        assert!(plan.files.is_empty());
    }

    #[test]
    fn gateway_plan_renders_split_routing() {
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("PRIV".into()),
            wireguard_peer_public_key: Some("PUB".into()),
            ..Default::default()
        };
        let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
        assert_eq!(plan.status, PlanStatus::Ready);
        let nft = plan
            .files
            .iter()
            .find(|f| f.relative_path.ends_with("nftables.conf"))
            .unwrap();
        assert!(nft.contents.contains("table inet gateway_kit"));
        assert!(nft.contents.contains("meta nfproto ipv6 drop"));
        assert!(nft.contents.contains("oifname \"eth0\" masquerade"));
        assert!(nft.contents.contains("tproxy ip to 127.0.0.1:7895"));
        assert!(!nft.contents.contains("policy drop"));
        assert!(nft.contents.contains("chain output"));
        assert!(
            nft.contents
                .contains("ip daddr { 192.168.50.1, 127.0.0.1 }")
        );
        assert!(!nft.contents.contains("tcp dport 22 return\n    tcp dport"));
        let redacted = plan.redacted();
        let wg_pub = redacted
            .files
            .iter()
            .find(|f| f.relative_path.ends_with("wg0.conf"))
            .unwrap();
        assert!(!wg_pub.contents.contains("PRIV"));
        let sb = plan
            .files
            .iter()
            .find(|f| f.relative_path.ends_with("sing-box.json"))
            .unwrap();
        assert!(sb.contents.contains("192.168.40.0/24"));
        assert!(sb.contents.contains("geoip-cn"));
        assert!(nft.contents.contains("ip daddr 192.168.40.0/24 return"));
        assert!(
            nft.contents
                .contains("iifname \"eth0\" oifname \"eth1\" accept")
        );
        assert!(sb.contents.contains("jsdelivr.net"));
        assert!(!sb.contents.contains("githubusercontent"));
        assert!(sb.contents.contains("wg-out"));
        assert!(sb.contents.contains("\"bind_interface\": \"eth0\""));
        assert!(sb.contents.contains("\"bind_interface\": \"wg0\""));
        assert!(sb.contents.contains("\"auto_detect_interface\": false"));
        let wg = plan
            .files
            .iter()
            .find(|f| f.relative_path.ends_with("wg0.conf"))
            .unwrap();
        assert!(wg.contents.contains("PRIV"));
        assert!(!plan.explanation.contains("PRIV"));
        let dhcp = plan
            .files
            .iter()
            .find(|f| f.relative_path.contains("dnsmasq"))
            .unwrap();
        assert!(dhcp.contents.contains("dhcp-option=3,192.168.50.1"));
        assert!(dhcp.contents.contains("dhcp-option=6,192.168.50.1"));
        assert!(dhcp.contents.contains("no-resolv"));
        assert!(dhcp.contents.contains("server=127.0.0.1#5353"));
        assert!(dhcp.contents.contains("listen-address=192.168.50.1"));
        assert!(nft.contents.contains("dport 53 return"));
        assert!(nft.contents.contains("chain prerouting_nat"));
        assert!(
            nft.contents
                .contains("iifname \"eth1\" udp dport 53 dnat ip to 192.168.50.1:53")
        );
        assert!(sb.contents.contains("\"dns-in\""));
        assert!(sb.contents.contains("\"proxy-dns\""));
        assert!(sb.contents.contains("\"strategy\": \"ipv4_only\""));
    }

    #[test]
    fn china_direct_off_does_not_pull_geoip() {
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.routing.china_direct = false;
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("PRIV".into()),
            wireguard_peer_public_key: Some("PUB".into()),
            ..Default::default()
        };
        let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
        let sb = plan
            .files
            .iter()
            .find(|f| f.relative_path.ends_with("sing-box.json"))
            .unwrap();
        assert!(!sb.contents.contains("geoip-cn"));
        assert!(sb.contents.contains("\"final\": \"wg-out\""));
    }

    #[test]
    fn blockers_stop_apply() {
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("PRIV".into()),
            wireguard_peer_public_key: Some("PUB".into()),
            ..Default::default()
        };
        let mut report = PreflightReport::default();
        report.conflicts.push(gateway_model::Conflict {
            id: "ufw".into(),
            severity: gateway_model::ConflictSeverity::Blocker,
            resource_id: "ufw".into(),
            title: "UFW".into(),
            detail: "on".into(),
            recommendation: "off".into(),
        });
        let plan = build_plan(&cfg, &secrets, &report).unwrap();
        assert_eq!(plan.status, PlanStatus::Blocked);
        let err = apply_plan(
            &Paths::workspace(),
            &cfg,
            &gateway_model::Secrets::default(),
            &plan,
            gateway_model::ApplyReason::UserConfirm,
            &ScriptedRunner::default(),
            false,
        )
        .unwrap_err();
        assert!(matches!(err, CoreError::Blocked(_)));
    }

    #[test]
    fn confirm_apply_writes_and_can_disable() {
        let dir = std::env::temp_dir().join(format!("gk-test-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let mut paths = Paths::workspace();
        paths.generated_dir = dir.join("generated");
        paths.backup_dir = dir.join("backups");
        paths.state_db = dir.join("state.db");
        paths.ensure_dirs().unwrap();
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("PRIV".into()),
            wireguard_peer_public_key: Some("PUB".into()),
            ..Default::default()
        };
        let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
        let runner = ScriptedRunner::succeeding();
        apply_plan(
            &paths,
            &cfg,
            &secrets,
            &plan,
            gateway_model::ApplyReason::UserConfirm,
            &runner,
            true,
        )
        .unwrap();
        assert!(paths.generated_dir.join("nftables.conf").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(paths.generated_dir.join("sing-box.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let calls = runner.calls.lock().unwrap_or_else(|e| e.into_inner());
        assert!(calls.iter().any(|(p, args)| {
            p == "ip"
                && args.first().map(String::as_str) == Some("rule")
                && args.get(1).map(String::as_str) == Some("add")
        }));
        assert!(
            !calls.iter().any(|(p, args)| {
                p == "wg"
                    && args.first().map(String::as_str) == Some("syncconf")
                    && args.iter().any(|a| a.ends_with("wg0.conf"))
            }),
            "wg syncconf must use stripped .sync.conf, not wg-quick Address/Table file"
        );
        assert!(
            !calls
                .iter()
                .any(|(p, args)| { p == "systemctl" && args.iter().any(|a| a == "enable") }),
            "workspace apply must not enable host systemd units"
        );
        drop(calls);
        disable(&paths, &cfg, &runner, true).unwrap();
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_replaces_iface_addrs_not_main_default() {
        let dir = std::env::temp_dir().join(format!("gk-addr-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let mut paths = Paths::workspace();
        paths.generated_dir = dir.join("generated");
        paths.backup_dir = dir.join("backups");
        paths.state_db = dir.join("state.db");
        paths.ensure_dirs().unwrap();
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("PRIV".into()),
            wireguard_peer_public_key: Some("PUB".into()),
            ..Default::default()
        };
        let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
        let runner = ScriptedRunner::succeeding();
        apply_plan(
            &paths,
            &cfg,
            &secrets,
            &plan,
            gateway_model::ApplyReason::UserConfirm,
            &runner,
            true,
        )
        .unwrap();
        let calls = runner.calls.lock().unwrap_or_else(|e| e.into_inner());
        assert!(calls.iter().any(|(p, args)| {
            p == "ip"
                && args.first().map(String::as_str) == Some("addr")
                && args.iter().any(|a| a == "eth1")
        }));
        assert!(calls.iter().any(|(p, args)| {
            p == "ip"
                && args.first().map(String::as_str) == Some("addr")
                && args.iter().any(|a| a == "eth0")
        }));
        assert!(!calls.iter().any(|(p, args)| {
            p == "ip"
                && args
                    .windows(2)
                    .any(|w| w[0] == "route" && w[1] == "default")
        }));
        assert!(
            calls
                .iter()
                .any(|(p, args)| { p == "ip" && args.iter().any(|a| a.ends_with("/32")) })
        );
        drop(calls);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_installs_non_default_wg_routes_not_main_default() {
        let dir = std::env::temp_dir().join(format!("gk-wg-rt-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let mut paths = Paths::workspace();
        paths.generated_dir = dir.join("generated");
        paths.backup_dir = dir.join("backups");
        paths.state_db = dir.join("state.db");
        paths.ensure_dirs().unwrap();
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        cfg.wireguard.peer_allowed_ips = "10.66.0.1/32,0.0.0.0/0".into();
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("PRIV".into()),
            wireguard_peer_public_key: Some("PUB".into()),
            ..Default::default()
        };
        let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
        let runner = ScriptedRunner::succeeding();
        apply_plan(
            &paths,
            &cfg,
            &secrets,
            &plan,
            gateway_model::ApplyReason::UserConfirm,
            &runner,
            true,
        )
        .unwrap();
        let calls = runner.calls.lock().unwrap_or_else(|e| e.into_inner());
        assert!(calls.iter().any(|(p, args)| {
            p == "ip"
                && args.windows(4).any(|w| {
                    w[0] == "route" && w[1] == "replace" && w[2] == "10.66.0.1/32" && w[3] == "dev"
                })
        }));
        assert!(!calls.iter().any(|(p, args)| {
            p == "ip"
                && args
                    .windows(2)
                    .any(|w| w[0] == "route" && w[1] == "default")
        }));
        drop(calls);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn boot_restore_replays_last_successful_generation() {
        let dir = std::env::temp_dir().join(format!("gk-boot-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let mut paths = Paths::workspace();
        paths.config = dir.join("config.toml");
        paths.secrets = dir.join("secrets.toml");
        paths.generated_dir = dir.join("generated");
        paths.backup_dir = dir.join("backups");
        paths.state_db = dir.join("state.db");
        paths.ensure_dirs().unwrap();
        let mut cfg = AppConfig::default();
        cfg.mode = OperatingMode::Gateway;
        cfg.wan.dns = vec!["192.0.2.53".into()];
        cfg.wan.interface = "eth0".into();
        cfg.lan.interface = "eth1".into();
        cfg.wireguard.enabled = true;
        cfg.wireguard.address = "10.66.0.2/32".into();
        cfg.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("PRIV".into()),
            wireguard_peer_public_key: Some("PUB".into()),
            ..Default::default()
        };
        crate::save_config(&paths.config, &cfg).unwrap();
        crate::save_secrets(&paths.secrets, &secrets).unwrap();
        let plan = build_plan(&cfg, &secrets, &PreflightReport::default()).unwrap();
        let runner = ScriptedRunner::succeeding();
        apply_plan(
            &paths,
            &cfg,
            &secrets,
            &plan,
            gateway_model::ApplyReason::UserConfirm,
            &runner,
            true,
        )
        .unwrap();
        let restored = restore_on_boot(&paths, &secrets, &runner)
            .unwrap()
            .expect("boot restore");
        assert!(restored.message.contains("BootRestore"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn boot_recovery_bypasses_interrupted_apply_even_if_config_was_changed() {
        let dir = std::env::temp_dir().join(format!("gk-recovery-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let mut paths = Paths::workspace();
        paths.config = dir.join("config.toml");
        paths.secrets = dir.join("secrets.toml");
        paths.generated_dir = dir.join("generated");
        paths.backup_dir = dir.join("backups");
        paths.state_db = dir.join("state.db");
        paths.ensure_dirs().unwrap();

        let mut interrupted = AppConfig::default();
        interrupted.mode = OperatingMode::Gateway;
        interrupted.wan.dns = vec!["192.0.2.53".into()];
        interrupted.wan.interface = "eth0".into();
        interrupted.lan.interface = "eth1".into();
        interrupted.wireguard.enabled = true;
        interrupted.wireguard.address = "10.66.0.2/32".into();
        interrupted.wireguard.peer_endpoint = "203.0.113.9:51820".into();
        crate::save_config(&paths.config, &AppConfig::default()).unwrap();
        let conn = init_database(&paths.state_db).unwrap();
        begin_apply_journal(&conn, "g-interrupted", &interrupted).unwrap();

        let result = restore_on_boot(
            &paths,
            &gateway_model::Secrets::default(),
            &ScriptedRunner::succeeding(),
        );
        assert!(result.is_err());
        let conn = init_database(&paths.state_db).unwrap();
        assert!(pending_apply_journal(&conn).unwrap().is_none());
        assert!(latest_successful_generation(&conn).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_secrets_does_not_log_and_unix_mode_is_private() {
        let dir = std::env::temp_dir().join(format!("gk-sec-{}", unix_now()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.toml");
        let secrets = gateway_model::Secrets {
            wireguard_private_key: Some("SUPERSECRET".into()),
            ..Default::default()
        };
        crate::save_secrets(&path, &secrets).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains("SUPERSECRET"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn install_script_is_observe_only() {
        let sh = include_str!("../../../packaging/install.sh");
        let unit = include_str!("../../../packaging/gateway-kit.service");
        let wait = include_str!("../../../packaging/gateway-kit-wait-network.sh");
        assert!(sh.contains("observe"));
        assert!(sh.contains("http://127.0.0.1:7676"));
        assert!(sh.contains("ip -4 -o addr show"));
        assert!(sh.contains("gateway-kit-wait-network.sh"));
        assert!(unit.contains("EnvironmentFile=-/etc/gateway-kit/generated/forwarding.env"));
        assert!(unit.contains("ExecStartPre=/usr/local/libexec/gateway-kit-wait-network.sh"));
        assert!(unit.contains("Restart=on-failure"));
        assert!(wait.contains("LAN_IF"));
        assert!(wait.contains("WAN_IF"));
        assert!(wait.contains("ip link show dev"));
        assert!(!sh.contains("wg-quick"));
        assert!(!sh.contains("ip route"));
        assert!(!sh.contains("ip addr"));
    }
}
