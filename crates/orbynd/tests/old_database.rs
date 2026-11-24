//! The real service started on a database the first release left behind (`fixtures/database_v1.sql`: schema version 1 with a
//! project, a thread, a finished run and an audit row). Every later migration runs on it at start; the person's data must
//! still be there afterwards, the upgrade must be announced once, and a second start must have nothing left to do.

mod support;

use support::{Options, Service, TempDir};

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
