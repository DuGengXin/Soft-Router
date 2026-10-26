use super::*;

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

pub(crate) const CURRENT_SCHEMA_VERSION: i64 = 3;

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

pub(crate) fn table_exists(conn: &Connection, table: &str) -> Result<bool, CoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        params![table],
        |row| row.get(0),
    )?)
}

pub(crate) fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, CoreError> {
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
