//! The real service started on databases older releases left behind. Schema 1 is a frozen file
//! (`fixtures/database_v1.sql`); later versions are built by H4's `support::old_database` from the
//! real migration list, so they cannot drift from the schema a release actually shipped. Every
//! later migration runs at start; the person's data must still be there afterwards, the upgrade
//! must be announced once, and a second start must have nothing left to do. A database newer than
//! this build is refused without touching it.

mod support;

use support::{Options, Service, TempDir};

/// A database at `through` holding the rows `fixtures/database_rows.sql` says a release left.
fn old_database_with_rows(dir: &TempDir, through: i64) -> std::path::PathBuf {
    let data = dir.path().join("data");
    let project = dir.path().join("kept-project");
    std::fs::create_dir_all(&project).unwrap();
    let conn = support::old_database(&data, through);
    let rows = include_str!("fixtures/database_rows.sql").replace(
        "{{PROJECT_PATH}}",
        &support::canonical(&project).display().to_string(),
    );
    conn.execute_batch(&rows).unwrap();
    data
}

fn old_database(dir: &TempDir) -> std::path::PathBuf {
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let project = dir.path().join("kept-project");
    std::fs::create_dir_all(&project).unwrap();
    let sql = include_str!("fixtures/database_v1.sql").replace(
        "{{PROJECT_PATH}}",
        &support::canonical(&project).display().to_string(),
    );
    let conn = rusqlite::Connection::open(data.join("orbyn.sqlite3")).unwrap();
    conn.execute_batch(&sql).unwrap();
    data
}

fn start(data: &std::path::Path) -> Service {
    Service::start(
        data,
        Options {
            args: vec![],
            env: vec![],
        },
    )
}

#[tokio::test]
async fn a_database_from_the_first_release_is_upgraded_at_start_with_its_data_intact() {
    let dir = TempDir::new();
    let data = old_database(&dir);

    let service = start(&data);
    let info = service.json("/api/info").await;
    assert_eq!(info["database_upgrade"]["from"], 1, "{info}");
    assert!(
        info["database_upgrade"]["to"].as_i64().unwrap() > 1,
        "{info}"
    );

    let projects = service.json("/api/projects").await;
    let kept = projects
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "p-old")
        .unwrap_or_else(|| panic!("the old project is gone: {projects}"));
    assert_eq!(kept["name"], "Kept project");
    assert_eq!(kept["trusted"], true);

    let threads = service.json("/api/threads?project_id=p-old").await;
    let thread = threads
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "t-old")
        .unwrap_or_else(|| panic!("the old thread is gone: {threads}"));
    assert_eq!(thread["title"], "A thread from the first release");
    assert_eq!(thread["last_run_state"], "completed");

    let audit = service.json("/api/audit?limit=200").await;
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "thread.create" && row["target"] == "t-old"),
        "the old audit row is gone: {audit}"
    );
    service.stop().await;

    // The next start finds nothing to do and announces nothing.
    let service = start(&data);
    assert!(
        service
            .json("/api/info")
            .await
            .get("database_upgrade")
            .is_none()
    );
    assert!(
        service
            .json("/api/threads?project_id=p-old")
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == "t-old")
    );
    service.stop().await;
}

/// Migration 35 adds the account's owning column; starting from 34 is the version the unit test
/// `migration_versions_run_from_one_with_no_gap_through_the_account_owner` builds, so this edge case
/// upgrades a database from before that column through every later migration in one start.
#[tokio::test]
async fn a_database_from_before_the_account_owner_is_upgraded_at_start_and_keeps_its_rows() {
    let dir = TempDir::new();
    let data = old_database_with_rows(&dir, 34);

    let service = start(&data);
    let info = service.json("/api/info").await;
    assert_eq!(info["database_upgrade"]["from"], 34, "{info}");
    assert_eq!(
        info["database_upgrade"]["to"],
        support::latest_schema(),
        "{info}"
    );

    let projects = service.json("/api/projects").await;
    let kept = projects
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "p-mid")
        .unwrap_or_else(|| panic!("the old project is gone: {projects}"));
    assert_eq!(kept["name"], "Kept project");
    assert_eq!(kept["trusted"], true);
    let threads = service.json("/api/threads?project_id=p-mid").await;
    let thread = threads
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "t-mid")
        .unwrap_or_else(|| panic!("the old thread is gone: {threads}"));
    assert_eq!(thread["title"], "A thread from an earlier release");
    assert_eq!(thread["last_run_state"], "completed");
    let audit = service.json("/api/audit?limit=200").await;
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["action"] == "thread.create" && row["target"] == "t-mid"),
        "the old audit row is gone: {audit}"
    );
    service.stop().await;

    // One upgrade only: a second start announces nothing and still reads the rows.
    let service = start(&data);
    assert!(
        service
            .json("/api/info")
            .await
            .get("database_upgrade")
            .is_none()
    );
    assert!(
        service
            .json("/api/threads?project_id=p-mid")
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["id"] == "t-mid")
    );
    service.stop().await;

    // Every migration is recorded exactly once, with none applied again by the second start.
    let conn = rusqlite::Connection::open(data.join("orbyn.sqlite3")).unwrap();
    let recorded: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(recorded, support::latest_schema());
}

#[tokio::test]
async fn a_database_newer_than_this_build_is_refused_and_left_untouched() {
    let dir = TempDir::new();
    let data = dir.path().join("data");
    let project = dir.path().join("kept-project");
    std::fs::create_dir_all(&project).unwrap();
    let conn = support::old_database(&data, support::latest_schema());
    let rows = include_str!("fixtures/database_rows.sql").replace(
        "{{PROJECT_PATH}}",
        &support::canonical(&project).display().to_string(),
    );
    conn.execute_batch(&rows).unwrap();
    let future = support::latest_schema() + 1;
    conn.execute(
        "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, 'future')",
        [future],
    )
    .unwrap();
    drop(conn);

    let error = Service::try_start(&data, Options::default())
        .err()
        .expect("a database newer than this build is refused");
    let wanted = format!(
        "database schema {future} is newer than supported schema {}",
        support::latest_schema()
    );
    assert!(error.contains(wanted.as_str()), "{error}");

    // The refusal wrote nothing: the future version and the person's rows are as they were.
    let conn = rusqlite::Connection::open(data.join("orbyn.sqlite3")).unwrap();
    let recorded: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(recorded, future);
    let kept: String = conn
        .query_row("SELECT name FROM projects WHERE id = 'p-mid'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(kept, "Kept project");
}
