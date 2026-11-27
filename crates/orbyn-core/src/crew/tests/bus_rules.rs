//! Committed repository limits through the real MCP bus, with fake engines only.

use serde_json::json;

use super::bus_support::{Duo, duo};
use super::classes::commit_crew;
use super::harness::{Turn, World, bind, brief, key, world};
use crate::crew::bus::{Limits, config::parse};
use crate::testing::run_git;

#[test]
fn bus_sections_are_strict_lower_only_and_diagnostics_do_not_quote_values() {
    let host = Limits::default().lowered(Some(2), Some(5));
    for source in [
        r#"{}"#,
        r#"{"bus":{}}"#,
        r#"{"bus":{"exchanges":9,"posts":4294967296,"future":"private text"}}"#,
    ] {
        assert_eq!(parse(source).unwrap().apply(host), host);
    }
    let zero = parse(r#"{"bus":{"exchanges":0,"posts":0}}"#)
        .unwrap()
        .apply(host);
    assert_eq!((zero.exchanges, zero.posts), (0, 0));
    for source in [
        r#"{"bus":null}"#,
        r#"{"bus":[]}"#,
        r#"{"bus":1}"#,
        r#"{"bus":{"posts":null}}"#,
        r#"{"bus":{"posts":-1}}"#,
        r#"{"bus":{"posts":1.5}}"#,
        r#"{"bus":{"posts":"private text"}}"#,
        r#"{"bus":{"posts":true}}"#,
        r#"{"bus":{"posts":1,"posts":2}}"#,
        r#"{"bus":{"exchanges":1,"exchanges":2}}"#,
        r#"{"bus":{},"bus":{}}"#,
        "[]",
        "{",
        "{} {}",
    ] {
        let error = parse(source).unwrap_err();
        assert!(!error.contains("private text"), "{error}");
    }
}

#[tokio::test]
async fn committed_bus_posts_include_board_writes_and_refresh_without_resetting_counts() {
    let d = duo().await;
    commit_crew(&d.w.repo, r#"{"bus":{"posts":2}}"#);
    let (answer, error) = d.call(&d.ta, "crew_post", Duo::fact("lead", "one")).await;
    assert!(!error, "{answer}");
    let (answer, error) = d
        .call(
            &d.ta,
            "crew_board_write",
            json!({
                "key":"limit.fact", "class":"decision", "value":"two", "base_rev":0
            }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(
        d.refused(&d.ta, "crew_post", Duo::fact("lead", "three"))
            .await,
        "post_limit_exceeded"
    );
    commit_crew(&d.w.repo, r#"{"bus":{"posts":3}}"#);
    let (answer, error) = d.call(&d.ta, "crew_post", Duo::fact("lead", "three")).await;
    assert!(!error, "{answer}");
    assert_eq!(
        d.refused(
            &d.ta,
            "crew_board_write",
            json!({
                "key":"limit.other", "class":"decision", "value":"four", "base_rev":0
            })
        )
        .await,
        "post_limit_exceeded"
    );
}

#[tokio::test]
async fn committed_bus_hops_tighten_the_existing_pair_counter() {
    let d = duo().await;
    commit_crew(&d.w.repo, r#"{"bus":{"exchanges":1}}"#);
    let (answer, error) = d.call(&d.ta, "crew_post", Duo::fact(&d.mb, "one")).await;
    assert!(!error, "{answer}");
    assert_eq!(
        d.refused(&d.tb, "crew_post", Duo::fact(&d.ma, "two")).await,
        "hop_limit_exceeded"
    );
}

#[tokio::test]
async fn a_missing_bus_section_keeps_default_bus_behavior() {
    let d = duo().await;
    commit_crew(&d.w.repo, r#"{}"#);
    for n in 0..3 {
        let (answer, error) = d
            .call(
                &d.ta,
                "crew_post",
                Duo::fact(&d.mb, &format!("message {n}")),
            )
            .await;
        assert!(!error, "{answer}");
    }
    assert_eq!(
        d.refused(&d.ta, "crew_post", Duo::fact(&d.mb, "four"))
            .await,
        "hop_limit_exceeded"
    );
}

#[tokio::test]
async fn whole_document_failures_refuse_the_start_without_starting_a_worker() {
    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    commit_crew(&w.repo, "{ not json");
    // Issue #795: a write task reads `.orbyn/crew.json` at the commit it starts from for the repository's own
    // checks, so a file that cannot be read refuses the start in the reader's words, before any task is stored.
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(error, "{answer}");
    assert_eq!(answer["code"], "invalid_object", "{answer}");
    assert!(
        answer["message"]
            .as_str()
            .unwrap()
            .contains(".orbyn/crew.json is not valid"),
        "{answer}"
    );
    assert!(
        w.crew_tasks.lock().unwrap().is_empty(),
        "no fake worker may have started"
    );
    w.core.shutdown().await;

    let d = duo().await;
    commit_crew(&d.w.repo, "{ not json");
    let (answer, error) = d
        .call(&d.ta, "crew_post", Duo::fact("lead", "refused"))
        .await;
    assert!(error, "{answer}");
    assert_eq!(d.w.core.store.count_crew_posts(&d.a, &d.ma).unwrap(), 0);
    d.w.core.shutdown().await;
}

#[tokio::test]
async fn a_damaged_pin_keeps_start_refusal_audit_and_blocks_an_existing_workers_bus() {
    let d = duo().await;
    let project = d.w.core.store.thread(&d.w.lead).unwrap().project_id;
    let started = d.w.crew_tasks.lock().unwrap().len();
    let db = rusqlite::Connection::open(d.w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO project_preferences(project_id,key,value) VALUES(?1,'crew_pinned_ref','not json')
         ON CONFLICT(project_id,key) DO UPDATE SET value=excluded.value",
        [&project],
    ).unwrap();
    let (answer, error) =
        d.w.start(
            brief(json!({
                "outcome":"A different task.", "files":[{"path":"lib/**","mode":"write"}]
            })),
            key(),
        )
        .await;
    assert!(
        !error && answer["routed"] == false && answer["state"] == "needs_person",
        "{answer}"
    );
    assert_eq!(
        d.w.crew_tasks.lock().unwrap().len(),
        started,
        "no additional engine may start"
    );
    let task = answer["task"].as_str().unwrap();
    assert!(!d.w.core.store.crew_results(task).unwrap().is_empty());
    let (answer, error) = d
        .call(&d.ta, "crew_post", Duo::fact("lead", "refused"))
        .await;
    assert!(error, "{answer}");
    assert!(
        answer["message"].as_str().unwrap().contains("damaged"),
        "{answer}"
    );
    assert_eq!(d.w.core.store.count_crew_posts(&d.a, &d.ma).unwrap(), 0);
    drop(db);
    d.w.core.shutdown().await;
}

#[tokio::test]
async fn ordinary_pin_storage_errors_are_not_deferred_or_defaulted() {
    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    let db = rusqlite::Connection::open(w._dir.path().join("data/orbyn.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE project_preferences").unwrap();
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(error, "{answer}");
    assert!(w.core.store.crew_tasks_in_crew(&w.lead).unwrap().is_empty());
    assert!(w.crew_tasks.lock().unwrap().is_empty());
    drop(db);
    w.core.shutdown().await;
}

#[tokio::test]
async fn malformed_committed_policy_refuses_start_before_admission_and_keeps_reads_available() {
    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    commit_crew(&w.repo, r#"{"bus":{"posts":null}}"#);
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(error, "{answer}");
    assert!(w.core.store.crew_tasks_in_crew(&w.lead).unwrap().is_empty());
    commit_crew(&w.repo, r#"{"bus":{"posts":1}}"#);
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error && answer["task"].is_string(), "{answer}");
    let worker = w.worker(answer["task"].as_str().unwrap());
    let token = bind(&w.core, &worker, "bus-invalid");
    commit_crew(&w.repo, r#"{"bus":null}"#);
    let (answer, error) = w
        .call(&token, "crew_post", Duo::fact("lead", "refused"))
        .await;
    assert!(error, "{answer}");
    let (answer, error) = w
        .call(&token, "crew_board_read", json!({"key":"empty"}))
        .await;
    assert!(!error && answer["fact"].is_null(), "{answer}");
    w.core.shutdown().await;
}

/// This must share one Core: separate worlds cannot expose accidental global Bus::lower.
#[tokio::test]
async fn two_projects_in_one_core_keep_their_own_post_limits_and_host_ceiling() {
    let d = duo().await;
    commit_crew(&d.w.repo, r#"{"bus":{"posts":1}}"#);
    let project = d.w.core.new_project("other", false, None).await.unwrap();
    d.w.core.store.trust_project(&project.id, "test").unwrap();
    let root = std::path::Path::new(&project.path);
    commit_crew(root, r#"{"bus":{"posts":999}}"#);
    let lead =
        d.w.core
            .create_thread(orbyn_contracts::CreateThread {
                project_id: Some(project.id),
                engine: "alpha".into(),
                model: None,
                title: None,
                worktree: None,
                skill_profile: None,
                base_branch: None,
                effort: None,
                options: Default::default(),
                branch_name_seed: None,
            })
            .await
            .unwrap();
    let lead_token = bind(&d.w.core, &lead.id, "other-lead");
    let (started, error) =
        d.w.call(
            &lead_token,
            "crew_start",
            json!({"brief":brief(json!({})),"answer_key":key()}),
        )
        .await;
    assert!(!error && started["task"].is_string(), "{started}");
    let task = started["task"].as_str().unwrap();
    let worker = d.w.worker(task);
    let token = bind(&d.w.core, &worker, "other-worker");
    d.w.core.crew_bus.lower(None, Some(2));
    for (token, n) in [(&d.ta, 1), (&token, 1), (&token, 2)] {
        let (answer, error) =
            d.w.call(
                token,
                "crew_post",
                Duo::fact("lead", &format!("message {n}")),
            )
            .await;
        assert!(!error, "{answer}");
    }
    assert_eq!(
        d.refused(&d.ta, "crew_post", Duo::fact("lead", "two"))
            .await,
        "post_limit_exceeded"
    );
    assert_eq!(
        d.refused(&token, "crew_post", Duo::fact("lead", "three"))
            .await,
        "post_limit_exceeded"
    );
    assert_eq!(d.w.core.crew_bus.limits().posts, 2);
    d.w.core.shutdown().await;
}

#[tokio::test]
async fn uncommitted_edits_do_not_change_rules_and_default_branch_can_only_tighten() {
    let d = duo().await;
    commit_crew(&d.w.repo, r#"{"bus":{"posts":1}}"#);
    super::classes::pin_origin_here(&d.w.repo);
    commit_crew(&d.w.repo, r#"{"bus":{"posts":5}}"#);
    std::fs::write(d.w.repo.join(".orbyn/crew.json"), r#"{"bus":{"posts":0}}"#).unwrap();
    let (answer, error) = d.call(&d.ta, "crew_post", Duo::fact("lead", "one")).await;
    assert!(!error, "{answer}");
    assert_eq!(
        d.refused(&d.ta, "crew_post", Duo::fact("lead", "two"))
            .await,
        "post_limit_exceeded"
    );
    let project = d.w.core.store.thread(&d.w.lead).unwrap().project_id;
    assert!(d.w.core.store.crew_pinned_ref(&project).unwrap().is_some());
    d.w.core.shutdown().await;
}

#[tokio::test]
async fn committed_oversized_and_linked_policy_is_refused_without_following_it() {
    let w = world(true, std::sync::Arc::new(|_, _| Turn::Hang)).await;
    let lead = w.core.store.thread(&w.lead).unwrap();
    commit_crew(
        &w.repo,
        &format!(r#"{{"bus":{{}},"pad":"{}"}}"#, "x".repeat(65536)),
    );
    let oversized = w.core.crew_repo_limits(&lead).await.unwrap_err();
    assert_eq!(oversized.code, orbyn_contracts::ErrorCode::InvalidObject);
    assert!(oversized.message.contains("over 64 KiB"), "{oversized:?}");
    // Create Git tree entries directly: this covers Windows too, without symlink privileges.
    for path in [".orbyn/crew.json", ".orbyn"] {
        run_git(&w.repo, &["rm", "-r", "-q", "--cached", ".orbyn"]);
        let file = w.repo.join("link-target");
        std::fs::write(&file, "../outside-secret-file").unwrap();
        let out = std::process::Command::new("git")
            .args(["hash-object", "-w", "link-target"])
            .current_dir(&w.repo)
            .output()
            .unwrap();
        assert!(out.status.success());
        let sha = String::from_utf8(out.stdout).unwrap();
        run_git(
            &w.repo,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("120000,{},{}", sha.trim(), path),
            ],
        );
        run_git(
            &w.repo,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.test",
                "commit",
                "-q",
                "-m",
                "link",
            ],
        );
        let linked = w.core.crew_repo_limits(&lead).await.unwrap_err();
        assert_eq!(linked.code, orbyn_contracts::ErrorCode::InvalidObject);
        assert!(
            linked
                .message
                .contains(&format!("{path} is a symlink in the repository")),
            "{linked:?}"
        );
    }
    w.core.shutdown().await;
}

#[tokio::test]
async fn a_worker_never_borrows_rules_from_a_moved_lead_project() {
    let d = duo().await;
    let other =
        d.w.core
            .new_project("elsewhere", false, None)
            .await
            .unwrap();
    d.w.core.store.trust_project(&other.id, "test").unwrap();
    // The fake lead token has no actual lead run. Real lead threads with runs cannot move.
    d.w.core.store.move_thread(&d.w.lead, &other).unwrap();
    let (answer, error) = d
        .call(&d.ta, "crew_post", Duo::fact("lead", "wrong project"))
        .await;
    assert!(error, "{answer}");
    assert_eq!(d.w.core.store.count_crew_posts(&d.a, &d.ma).unwrap(), 0);
    d.w.core.shutdown().await;
}

#[tokio::test]
async fn reopening_core_rereads_committed_policy_and_preserves_the_stored_counter() {
    let d = duo().await;
    commit_crew(&d.w.repo, r#"{"bus":{"posts":1}}"#);
    let (answer, error) = d.call(&d.ta, "crew_post", Duo::fact("lead", "one")).await;
    assert!(!error, "{answer}");
    let lead_id = d.w.lead.clone();
    let task = d.a.clone();
    let member = d.ma.clone();
    let config = d.w.core.config.clone();
    d.w.core.shutdown().await;
    let World { _dir, core, .. } = d.w;
    let old = std::sync::Arc::downgrade(&core);
    drop(core);
    assert!(
        old.upgrade().is_none(),
        "first Core and all run owners must be gone"
    );
    let reopened = crate::app::Core::open(config).unwrap();
    let lead = reopened.store.thread(&lead_id).unwrap();
    let limits = reopened
        .crew_repo_limits(&lead)
        .await
        .unwrap()
        .apply(Limits::default());
    assert_eq!(limits.posts, 1);
    assert_eq!(reopened.store.count_crew_posts(&task, &member).unwrap(), 1);
    reopened.shutdown().await;
    drop(reopened);
    drop(_dir);
}
