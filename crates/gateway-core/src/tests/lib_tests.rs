use super::*;
use crate::command::ScriptedRunner;
use crate::db::{has_column, table_exists};
use gateway_model::{ChangePlan, OperatingMode, PlanStatus, RenderedFile};

#[test]
fn example_config_parses_as_observe() {
    let raw = include_str!("../../../../config.example.toml");
    let config: AppConfig = toml::from_str(raw).expect("example");
    assert_eq!(config.mode, OperatingMode::Observe);
    assert_eq!(config.firewall.table_name, "gateway_kit");
    assert_eq!(config.routing.policy_table_id, 51820);
    let secrets = gateway_model::Secrets {
        ..Default::default()
    };
    config.validate(&secrets).unwrap();
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
        &gateway_model::Secrets {
            ..Default::default()
        },
        &PreflightReport::default(),
    )
    .unwrap();
    assert_eq!(plan.status, PlanStatus::ObserveOnly);
    assert!(plan.files.is_empty());
}
