//! Data classes (slice 4i, routing): the file's strict parse, how it is read at the pinned commit, the lease against a class,
//! and what a start does with it. Fake engines only; the only processes are `git`.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use orbyn_contracts::RejectionWhy;
use serde_json::{Value, json};

use super::harness::{Turn, World, brief, key, world_with};
use crate::crew::classes::{
    BUILT_IN, ENGINES_MAX, FILE_MAX, GLOBS_MAX, NAME_MAX, PathClasses, REASON_MAX, RULES_MAX,
    base_commit, classify, parse_file, read_class_file,
};
use crate::testing::run_git;

fn rule(name: &str, paths: &[&str], engines: &[&str]) -> Value {
    json!({ "name": name, "paths": paths, "engines": engines, "reason": format!("{name} stays with its engines.") })
}

fn file(rules: Vec<Value>) -> String {
    json!({ "classes": { "version": 1, "rules": rules } }).to_string()
}

fn git_out(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// Stores `content` as a blob and puts it in the index at `path` with `mode`, with no file in the checkout.
fn stage(repo: &Path, mode: &str, path: &str, content: &str) {
    let mut child = Command::new("git")
        .args(["hash-object", "-w", "--stdin"])
        .current_dir(repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    std::io::Write::write_all(child.stdin.as_mut().unwrap(), content.as_bytes()).unwrap();
    let sha = String::from_utf8(child.wait_with_output().unwrap().stdout)
        .unwrap()
        .trim()
        .to_owned();
    run_git(
        repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("{mode},{sha},{path}"),
        ],
    );
}

fn commit(repo: &Path, message: &str) {
    run_git(
        repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.test",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            message,
        ],
    );
}

/// Writes `.orbyn/crew.json` in the checkout, and commits it.
fn commit_crew(repo: &Path, text: &str) {
    std::fs::create_dir_all(repo.join(".orbyn")).unwrap();
    std::fs::write(repo.join(".orbyn/crew.json"), text).unwrap();
    run_git(repo, &["add", "-f", ".orbyn/crew.json"]);
    commit(repo, "rules");
}

async fn two() -> World {
    world_with(
        true,
        Arc::new(|_, _| Turn::Hang),
        &[("alpha", false), ("bravo", false)],
    )
    .await
}

/// The rejections of the fake engines. The world also lists Claude and Codex, which are not installed and which a rule blocks as it does the rest.
fn fakes(rejected: &[orbyn_contracts::RouteRejection]) -> Vec<&orbyn_contracts::RouteRejection> {
    rejected
        .iter()
        .filter(|r| ["alpha", "bravo", "charlie"].contains(&r.engine.as_str()))
        .collect()
}

fn rows(w: &World, task: &str) -> Vec<crate::store::AutopilotEvent> {
    w.core
        .store
        .crew_autopilot_events(task)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "route_class")
        .collect()
}

#[test]
fn the_file_is_parsed_strictly() {
    let ok = |rules: Vec<Value>| parse_file(&file(rules));
    let bad = |text: String, words: &str| {
        let why = parse_file(&text).expect_err(&text);
        assert!(why.contains(words), "{why:?} should mention {words:?}");
    };
    let (rules, notes) = ok(vec![
        rule("auth-paths", &["**/auth/**", "crates/keys/**"], &["claude"]),
        rule("locked", &["secrets/**"], &[]),
    ])
    .unwrap();
    assert_eq!(rules.len(), 2);
    assert!(
        rules[1].engines.is_empty(),
        "no engines is valid and blocks all"
    );
    assert!(notes.is_empty());
    // Other slices' keys are read elsewhere; an unknown one is only noted.
    let (_, notes) =
        parse_file(r#"{"lanes":[],"future":1,"classes":{"version":1,"rules":[]}}"#).unwrap();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("future"));
    // No `classes` is no rules.
    assert!(parse_file(r#"{"lanes":[]}"#).unwrap().0.is_empty());

    bad("not json".into(), "not valid JSON");
    bad("[]".into(), "JSON object");
    bad(
        r#"{"classes":{"version":2,"rules":[]}}"#.into(),
        "version 2",
    );
    bad(r#"{"classes":{"rules":[]}}"#.into(), "version");
    bad(
        r#"{"classes":{"version":1,"rules":[],"extra":1}}"#.into(),
        "extra",
    );
    let mut unknown = rule("a", &["a/**"], &[]);
    unknown["colour"] = json!("red");
    bad(file(vec![unknown]), "colour");
    bad(
        file(
            (0..=RULES_MAX)
                .map(|n| rule(&format!("r{n}"), &["a/**"], &[]))
                .collect(),
        ),
        "rules",
    );
    let many_paths: Vec<String> = (0..=GLOBS_MAX).map(|n| format!("d{n}/**")).collect();
    let many_paths: Vec<&str> = many_paths.iter().map(String::as_str).collect();
    bad(file(vec![rule("a", &many_paths, &[])]), "paths");
    bad(file(vec![rule("a", &[], &[])]), "paths");
    let many_engines: Vec<String> = (0..=ENGINES_MAX).map(|n| format!("e{n}")).collect();
    let many_engines: Vec<&str> = many_engines.iter().map(String::as_str).collect();
    bad(file(vec![rule("a", &["a/**"], &many_engines)]), "engines");
    for name in [
        "",
        "Upper",
        "has space",
        "under_score",
        &"a".repeat(NAME_MAX + 1),
    ] {
        bad(file(vec![rule(name, &["a/**"], &[])]), "name");
    }
    bad(
        file(vec![rule("a", &["a/**"], &[]), rule("a", &["b/**"], &[])]),
        "used twice",
    );
    let mut long = rule("a", &["a/**"], &[]);
    long["reason"] = json!("x".repeat(REASON_MAX + 1));
    bad(file(vec![long]), "reason");
    let mut empty = rule("a", &["a/**"], &[]);
    empty["reason"] = json!("  ");
    bad(file(vec![empty]), "reason");
    bad(
        file(vec![rule("a", &["a/**"], &["Not An Id"])]),
        "engine id",
    );
    for glob in [
        "a/?.rs", "a/[x]", "a/{b,c}", "/abs", "a\\b", "C:/x", "a/../b", "a//b",
    ] {
        bad(file(vec![rule("a", &[glob], &[])]), "path");
    }
    // A secret anywhere in the file refuses it, and the message names its class, never the text.
    let mut secret = rule("a", &["a/**"], &[]);
    secret["reason"] = json!(format!("token sk-{}", "a".repeat(30)));
    let why = parse_file(&file(vec![secret])).unwrap_err();
    assert!(
        why.contains("secret") || why.contains("key") || why.contains("token"),
        "{why}"
    );
    assert!(!why.contains("aaaaaaaa"), "{why}");
}

#[tokio::test]
async fn the_file_is_read_at_the_commit_and_only_when_it_is_a_plain_small_blob() {
    let w = two().await;
    let repo = w.repo.clone();
    let base = base_commit(&repo).await;
    // Nothing committed: no file, and not a problem.
    let none = read_class_file(&repo, base.as_deref()).await;
    assert!(!none.invalid() && none.rules.is_empty() && none.blob.is_none());
    assert_eq!(none.commit, base);
    // No commit to read at.
    assert!(read_class_file(&repo, None).await.invalid());

    commit_crew(&repo, &file(vec![rule("a", &["src/**"], &["alpha"])]));
    let at = base_commit(&repo).await;
    let good = read_class_file(&repo, at.as_deref()).await;
    assert!(!good.invalid(), "{:?}", good.problems);
    assert_eq!(
        good.blob.as_deref(),
        Some(git_out(&repo, &["rev-parse", "HEAD:.orbyn/crew.json"]).as_str())
    );
    // Over 64 KiB is refused before the blob is read.
    let big = format!(
        "{{\"classes\":{{\"version\":1,\"rules\":[]}},\"pad\":\"{}\"}}",
        "x".repeat(FILE_MAX as usize)
    );
    commit_crew(&repo, &big);
    let huge = read_class_file(&repo, base_commit(&repo).await.as_deref()).await;
    assert!(
        huge.invalid() && huge.problem_text().contains("64 KiB"),
        "{:?}",
        huge.problems
    );
    // The earlier commit still reads as it did: a read is pinned to its commit.
    let before = read_class_file(&repo, at.as_deref()).await;
    assert!(!before.invalid());

    // A symlink or a submodule in the place of the file is invalid, not missing.
    for (mode, what) in [("120000", "symlink"), ("160000", "submodule")] {
        commit_crew(&repo, &file(vec![]));
        run_git(&repo, &["rm", "-q", "--cached", ".orbyn/crew.json"]);
        if mode == "120000" {
            stage(&repo, mode, ".orbyn/crew.json", "../elsewhere.json");
        } else {
            let head = git_out(&repo, &["rev-parse", "HEAD"]);
            run_git(
                &repo,
                &[
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    &format!("{mode},{head},.orbyn/crew.json"),
                ],
            );
        }
        commit(&repo, "swap the file");
        let read = read_class_file(&repo, base_commit(&repo).await.as_deref()).await;
        assert!(read.invalid(), "{what}");
        assert!(read.problem_text().contains(what), "{:?}", read.problems);
    }
}

#[tokio::test]
async fn an_orbyn_folder_that_is_a_symlink_blocks_instead_of_reading_as_missing() {
    let w = two().await;
    stage(&w.repo, "120000", ".orbyn", "elsewhere");
    commit(&w.repo, "a link");
    let read = read_class_file(&w.repo, base_commit(&w.repo).await.as_deref()).await;
    assert!(read.invalid());
    assert!(
        read.problem_text().contains(".orbyn is a symlink"),
        "{:?}",
        read.problems
    );
}

#[tokio::test]
async fn a_start_without_the_file_routes_as_before_and_still_writes_its_row() {
    let w = two().await;
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["route"]["engine"], "alpha");
    let id = answer["task"].as_str().unwrap();
    let decision = w
        .core
        .store
        .crew_route_decisions(id)
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        decision
            .reasons
            .iter()
            .all(|r| r.code.as_str() != "path_classes")
    );
    let rows = rows(&w, id);
    assert_eq!(rows.len(), 1, "one row per routing");
    assert_eq!(rows[0].body["chosen"], "alpha");
    assert_eq!(
        rows[0].body["commit"],
        git_out(&w.repo, &["rev-parse", "HEAD"])
    );
    assert!(
        rows[0].body["blob"].is_null() && rows[0].body["classes"].as_array().unwrap().is_empty()
    );
}

#[tokio::test]
async fn a_class_keeps_the_other_engines_out_and_the_decision_and_the_row_name_the_file() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![rule("auth-paths", &["src/**"], &["bravo"])]),
    );
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    assert_eq!(
        answer["route"]["engine"], "bravo",
        "alpha would have won on id"
    );
    let id = answer["task"].as_str().unwrap();
    let decision = w
        .core
        .store
        .crew_route_decisions(id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(fakes(&decision.rejected).len(), 1);
    let blocked = fakes(&decision.rejected)[0];
    assert_eq!(
        (blocked.engine.as_str(), blocked.why),
        ("alpha", RejectionWhy::PathClass)
    );
    assert!(
        blocked.text.as_str().starts_with(
            "Blocked by rule: auth-paths \u{2014} allowed engines: bravo. auth-paths stays"
        ),
        "{}",
        blocked.text
    );
    let blob = git_out(&w.repo, &["rev-parse", "HEAD:.orbyn/crew.json"]);
    let head = git_out(&w.repo, &["rev-parse", "HEAD"]);
    let reason = decision
        .reasons
        .iter()
        .find(|r| r.code.as_str() == "path_classes")
        .unwrap();
    assert!(
        reason.text.as_str().contains(&blob) && reason.text.as_str().contains(&head),
        "{}",
        reason.text
    );
    let rows = rows(&w, id);
    assert_eq!(rows.len(), 1);
    let body = &rows[0].body;
    assert_eq!(
        (body["commit"].as_str(), body["blob"].as_str()),
        (Some(head.as_str()), Some(blob.as_str()))
    );
    assert_eq!(body["classes"][0]["name"], "auth-paths");
    assert_eq!(body["classes"][0]["pattern"], "src/**");
    assert_eq!(body["blocked"][0]["engine"], "alpha");
    assert_eq!(body["blocked"][0]["rule"], "auth-paths");
    assert_eq!(body["chosen"], "bravo");
    // The pinned base is the commit the rules were read at.
    assert_eq!(
        w.core
            .store
            .crew_run(id)
            .unwrap()
            .unwrap()
            .base_commit
            .as_deref(),
        Some(head.as_str())
    );
    // The routing as the route serves it.
    let view = w.core.crew_route_view(id).unwrap();
    assert_eq!(view.decision.unwrap().engine.as_str(), "bravo");
    assert!(view.refusal.is_none() && view.rows.len() == 1);
    assert_eq!(view.rows[0].blocked[0].rule, "auth-paths");
}

#[tokio::test]
async fn two_classes_leave_only_what_both_allow() {
    let w = world_with(
        true,
        Arc::new(|_, _| Turn::Hang),
        &[("alpha", false), ("bravo", false), ("charlie", false)],
    )
    .await;
    commit_crew(
        &w.repo,
        &file(vec![
            rule("one", &["src/**"], &["alpha", "bravo"]),
            rule("two", &["src/base.txt"], &["bravo", "charlie"]),
        ]),
    );
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap();
    assert_eq!(answer["route"]["engine"], "bravo");
    let decision = w
        .core
        .store
        .crew_route_decisions(id)
        .unwrap()
        .pop()
        .unwrap();
    let blocked: Vec<(&str, RejectionWhy)> = fakes(&decision.rejected)
        .iter()
        .map(|r| (r.engine.as_str(), r.why))
        .collect();
    assert_eq!(
        blocked,
        [
            ("alpha", RejectionWhy::PathClass),
            ("charlie", RejectionWhy::PathClass)
        ]
    );
    let body = &rows(&w, id)[0].body;
    let names: Vec<&str> = body["classes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["one", "two"]);
    assert!(
        body["classes"][1]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s == "src/base.txt")
    );
}

#[tokio::test]
async fn an_empty_intersection_starts_nothing_and_keeps_every_rejection_for_the_person() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![
            rule("only-alpha", &["src/**"], &["alpha"]),
            rule("only-bravo", &["src/**"], &["bravo"]),
        ]),
    );
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    assert_eq!(
        (answer["state"].as_str(), answer["routed"].as_bool()),
        (Some("needs_person"), Some(false))
    );
    let id = answer["task"].as_str().unwrap();
    assert!(answer["refusal"]["rejected"].as_array().unwrap().len() >= 2);
    assert!(
        answer["refusal"]["message"]
            .as_str()
            .unwrap()
            .contains("only-alpha"),
        "{answer}"
    );
    assert!(w.core.store.crew_route_decisions(id).unwrap().is_empty());
    assert!(
        w.core
            .store
            .crew_run(id)
            .unwrap()
            .unwrap()
            .worker_thread_id
            .is_none()
    );
    assert!(w.prompts.lock().unwrap().is_empty(), "nothing was started");
    let rows = rows(&w, id);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].body["chosen"].is_null());
    let all = rows[0].body["rejected"].as_array().unwrap().len();
    assert!(
        all >= 2,
        "every rejected engine is kept, not only the sentence"
    );
    assert_eq!(rows[0].body["blocked"].as_array().unwrap().len(), all);
    // The route serves the same refusal as the item shows.
    let view = w.core.crew_route_view(id).unwrap();
    assert_eq!(view.state, "needs_person");
    let refusal = view.refusal.unwrap();
    assert_eq!(fakes(&refusal.rejected).len(), 2);
    assert!(
        refusal
            .rejected
            .iter()
            .all(|r| r.why == RejectionWhy::PathClass)
    );
    assert_eq!(
        refusal.message,
        answer["refusal"]["message"].as_str().unwrap()
    );
}

#[tokio::test]
async fn an_explicit_engine_a_rule_blocks_is_refused_never_swapped() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    let tagged = brief(json!({ "engine": { "id": "alpha", "assigned_by": "person" } }));
    let (answer, error) = w.start(tagged, key()).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["routed"], false);
    assert_eq!(answer["refusal"]["requested"]["engine"], "alpha");
    assert!(
        answer["refusal"]["message"]
            .as_str()
            .unwrap()
            .starts_with("Blocked by rule: keep"),
        "{answer}"
    );
    let id = answer["task"].as_str().unwrap();
    assert!(
        w.core.store.crew_route_decisions(id).unwrap().is_empty(),
        "bravo was not put in its place"
    );
    // The tag on the engine the rule allows is routed.
    let allowed = brief(
        json!({ "outcome": "Another.", "engine": { "id": "bravo", "assigned_by": "person" } }),
    );
    let (answer, _) = w.start(allowed, key()).await;
    assert_eq!(answer["route"]["engine"], "bravo", "{answer}");
}

#[tokio::test]
async fn an_invalid_file_blocks_every_engine_and_says_why() {
    let w = two().await;
    commit_crew(&w.repo, r#"{"classes":{"version":3,"rules":[]}}"#);
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    assert_eq!(answer["state"], "needs_person");
    let message = answer["refusal"]["message"].as_str().unwrap();
    assert!(message.contains("version 3"), "{message}");
    let id = answer["task"].as_str().unwrap();
    let rejected = answer["refusal"]["rejected"].as_array().unwrap();
    assert!(rejected.len() >= 2);
    for one in rejected {
        assert_eq!(one["why"], "path_class");
        assert!(
            one["text"]
                .as_str()
                .unwrap()
                .starts_with("Blocked by rule: crew.json invalid"),
            "{one}"
        );
    }
    let body = &rows(&w, id)[0].body;
    assert!(body["problem"].as_str().unwrap().contains("version 3"));
    assert!(
        body["blob"].is_string(),
        "the blob that was refused is recorded"
    );
    // The result item carries the sentence the person reads.
    let result = w.core.store.crew_results(id).unwrap().pop().unwrap();
    assert!(result.summary.contains("version 3"), "{}", result.summary);
}

#[tokio::test]
async fn an_edit_after_the_base_is_not_in_force_and_the_view_says_so() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    // The checkout's copy is loosened but not committed.
    std::fs::write(
        w.repo.join(".orbyn/crew.json"),
        file(vec![rule("keep", &["src/**"], &["alpha", "bravo"])]),
    )
    .unwrap();
    let view = w.core.crew_classes_view(&w.lead).await.unwrap();
    assert!(view.working_copy_differs);
    assert!(
        view.working_copy_note
            .unwrap()
            .contains("not in force until it is committed")
    );
    assert_eq!(view.rules.len(), 1);
    assert_eq!(
        view.rules[0].engines.len(),
        1,
        "the committed rule, not the working copy"
    );
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    assert_eq!(
        answer["route"]["engine"], "bravo",
        "the committed file decided"
    );
    // Committing it again later does not move a task that already started.
    let id = answer["task"].as_str().unwrap().to_owned();
    let before = rows(&w, &id)[0].body.clone();
    run_git(&w.repo, &["add", "-f", ".orbyn/crew.json"]);
    commit(&w.repo, "loosen");
    assert_eq!(rows(&w, &id)[0].body, before);
    let view = w.core.crew_classes_view(&w.lead).await.unwrap();
    assert!(!view.working_copy_differs && view.working_copy_note.is_none());
    assert_eq!(view.rules[0].engines.len(), 2);
}

#[tokio::test]
async fn a_brief_with_no_files_counts_as_every_class_and_a_far_lease_as_none() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![rule("docs-only", &["docs/**"], &["bravo"])]),
    );
    let research = brief(json!({ "files": [], "allowed_effects": ["read_repo"] }));
    let (answer, error) = w.start(research, key()).await;
    assert!(!error, "{answer}");
    assert_eq!(
        answer["route"]["engine"], "bravo",
        "an unknown brief is checked against every class"
    );
    let id = answer["task"].as_str().unwrap();
    assert!(
        rows(&w, id)[0].body["unknown"]
            .as_str()
            .unwrap()
            .contains("no files")
    );
    // A lease on other files is in no class, so the default engine takes it.
    let (answer, _) = w.start(brief(json!({ "outcome": "Source." })), key()).await;
    assert_eq!(answer["route"]["engine"], "alpha", "{answer}");
    let id = answer["task"].as_str().unwrap();
    assert!(
        rows(&w, id)[0].body["classes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn the_dry_run_agrees_with_the_start() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    let (answer, error) = w
        .call(
            &w.lead_token,
            "crew_route",
            json!({ "brief": brief(json!({})) }),
        )
        .await;
    assert!(!error, "{answer}");
    assert_eq!(answer["decision"]["engine"], "bravo");
    let tagged = brief(json!({ "engine": { "id": "alpha", "assigned_by": "lead" } }));
    let (answer, _) = w
        .call(&w.lead_token, "crew_route", json!({ "brief": tagged }))
        .await;
    assert_eq!(answer["routed"], false);
    assert!(
        answer["refusal"]["message"]
            .as_str()
            .unwrap()
            .starts_with("Blocked by rule: keep")
    );
}

async fn classes_of(w: &World, patterns: &[&str]) -> PathClasses {
    let base = base_commit(&w.repo).await;
    let file = read_class_file(&w.repo, base.as_deref()).await;
    assert!(!file.invalid(), "{:?}", file.problems);
    let patterns: Vec<String> = patterns.iter().map(|p| (*p).to_owned()).collect();
    classify(&w.repo, base.as_deref(), file, &patterns).await
}

fn names(classes: &PathClasses) -> Vec<String> {
    classes
        .hits
        .iter()
        .map(|h| classes.rule(h).name.clone())
        .collect()
}

#[tokio::test]
async fn a_lease_is_checked_against_a_class_by_walk_listing_and_symlinks() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![
            rule("keys", &["crates/keys/**"], &["bravo"]),
            rule("auth", &["**/auth/**"], &["bravo"]),
            rule("env", &["*.env"], &["bravo"]),
        ]),
    );
    // A lease above a class's directory may hold it; a lease beside it does not.
    assert_eq!(
        names(&classes_of(&w, &["crates/**"]).await),
        ["keys", "auth"]
    );
    assert_eq!(
        names(&classes_of(&w, &["docs/**"]).await),
        ["auth"],
        "a floating pattern may be made anywhere"
    );
    // An exact file is matched as a path, folded.
    assert_eq!(names(&classes_of(&w, &["prod.ENV"]).await), ["env"]);
    assert!(names(&classes_of(&w, &["src/lib.rs"]).await).is_empty());

    // A tracked symlink in a lease reaches what it points to, inside the tree.
    stage(&w.repo, "100644", "crates/keys/k.txt", "k\n");
    stage(&w.repo, "120000", "notes/shortcut", "../crates/keys");
    commit(&w.repo, "links");
    let reached = classes_of(&w, &["notes/**"]).await;
    assert!(
        names(&reached).contains(&"keys".to_owned()),
        "{:?}",
        names(&reached)
    );
    assert!(reached.unknown.is_none());
    // One that points outside, or at nothing, counts as every class.
    stage(&w.repo, "120000", "notes/out", "../../etc/passwd");
    commit(&w.repo, "out");
    let out = classes_of(&w, &["notes/**"]).await;
    assert!(
        out.unknown.as_deref().unwrap().contains("outside"),
        "{:?}",
        out.unknown
    );
    assert_eq!(names(&out), ["keys", "auth", "env"]);
    run_git(&w.repo, &["rm", "-q", "--cached", "notes/out"]);
    stage(&w.repo, "120000", "notes/gone", "../nowhere/at/all");
    commit(&w.repo, "dangling");
    let dangling = classes_of(&w, &["notes/**"]).await;
    assert!(
        dangling
            .unknown
            .as_deref()
            .unwrap()
            .contains("not in the repository"),
        "{:?}",
        dangling.unknown
    );
    // A symlink on the way to an exact path leads somewhere too.
    run_git(&w.repo, &["rm", "-q", "--cached", "notes/gone"]);
    commit(&w.repo, "tidy");
    let through = classes_of(&w, &["notes/shortcut/k.txt"]).await;
    assert!(names(&through).contains(&"keys".to_owned()));
}

#[tokio::test]
async fn paths_are_folded_and_a_bad_path_is_refused_by_the_tester() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![rule("auth", &["src/Auth/**"], &["bravo"])]),
    );
    let paths: Vec<String> = [
        "SRC/auth/Login.rs",
        "src/cafe\u{301}.rs",
        "src/../x",
        "a\\b",
        "C:/x",
        "/abs",
        "docs/a.md",
    ]
    .iter()
    .map(|p| (*p).to_owned())
    .collect();
    let test = w.core.crew_class_test(&w.lead, &paths).await.unwrap();
    let by = |path: &str| test.paths.iter().find(|p| p.path == path).unwrap();
    let hit = by("SRC/auth/Login.rs");
    assert!(hit.valid && hit.classes[0].name == "auth");
    let engines: Vec<&str> = hit
        .allowed_engines
        .as_ref()
        .unwrap()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    assert_eq!(engines, ["bravo"]);
    assert!(
        hit.route_card
            .starts_with("auth: auth stays with its engines. Allowed engines: bravo."),
        "{}",
        hit.route_card
    );
    assert!(by("docs/a.md").classes.is_empty() && by("docs/a.md").allowed_engines.is_none());
    assert!(by("docs/a.md").route_card.starts_with(BUILT_IN));
    for bad in ["src/../x", "a\\b", "C:/x", "/abs"] {
        let one = by(bad);
        assert!(!one.valid && one.problem.is_some(), "{bad}");
    }
    assert!(w.core.crew_class_test(&w.lead, &[]).await.is_err());
    let eleven: Vec<String> = (0..11).map(|n| format!("p{n}")).collect();
    assert!(w.core.crew_class_test(&w.lead, &eleven).await.is_err());
}

#[tokio::test]
async fn the_views_exist_only_with_the_preview_flag() {
    let w = world_with(false, Arc::new(|_, _| Turn::Hang), &[("alpha", false)]).await;
    assert!(w.core.crew_classes_view(&w.lead).await.is_err());
    assert!(
        w.core
            .crew_class_test(&w.lead, &["a".to_owned()])
            .await
            .is_err()
    );
    assert!(w.core.crew_route_view("t").is_err());
}

#[tokio::test]
async fn a_lease_is_keyed_on_its_literal_spelling_and_a_case_variant_counts_as_every_class() {
    let w = two().await;
    commit_crew(
        &w.repo,
        &file(vec![
            rule("keys", &["crates/keys/**"], &["bravo"]),
            rule("secret", &["crates/secret/**"], &[]),
        ]),
    );
    stage(&w.repo, "100644", "crates/keys/k.txt", "k\n");
    stage(&w.repo, "100644", "docs/a.md", "a\n");
    stage(&w.repo, "120000", "docs/link", "../crates/keys");
    commit(&w.repo, "docs and a link");
    // The lease spelled as the folder is: the link under it is followed, and nothing else is pulled in.
    let plain = classes_of(&w, &["docs/**"]).await;
    assert!(plain.unknown.is_none(), "{:?}", plain.unknown);
    assert_eq!(names(&plain), ["keys"]);
    // Spelled in another case it lists nothing, yet names the folder on a case-folding file system: every class.
    for spelled in [
        &["DOCS/**"][..],
        &["Docs/**"],
        &["Docs/**", "docs/**"],
        &["docs/**", "Docs/**"],
    ] {
        let out = classes_of(&w, spelled).await;
        assert!(
            out.unknown
                .as_deref()
                .unwrap()
                .contains("only when case is ignored"),
            "{spelled:?}: {:?}",
            out.unknown
        );
        assert_eq!(names(&out), ["keys", "secret"], "{spelled:?}");
    }
    // A folder that exists under no spelling is one the worker would create: not unknown.
    let fresh = classes_of(&w, &["Newdir/**"]).await;
    assert!(fresh.unknown.is_none() && fresh.hits.is_empty());
}

#[tokio::test]
async fn a_huge_tree_or_many_patterns_end_as_every_class_never_as_none() {
    let w = two().await;
    // 64 rules of 32 patterns each: every path costs 2,048 glob tests, so a few hundred files are over the brief's budget.
    let rules: Vec<Value> = (0..64)
        .map(|r| {
            let paths: Vec<String> = (0..32).map(|p| format!("zz{r}-{p}/**")).collect();
            let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
            rule(&format!("r{r}"), &paths, &[])
        })
        .collect();
    commit_crew(&w.repo, &file(rules));
    std::fs::create_dir_all(w.repo.join("big")).unwrap();
    for n in 0..1100 {
        std::fs::write(w.repo.join(format!("big/f{n}.txt")), "x\n").unwrap();
    }
    std::fs::create_dir_all(w.repo.join("small")).unwrap();
    std::fs::write(w.repo.join("small/a.txt"), "x\n").unwrap();
    run_git(&w.repo, &["add", "big", "small"]);
    commit(&w.repo, "files");
    let started = std::time::Instant::now();
    let big = classes_of(&w, &["big/**"]).await;
    assert!(
        big.unknown.as_deref().unwrap().contains("too many"),
        "{:?}",
        big.unknown
    );
    assert_eq!(big.hits.len(), 64, "over the budget, every class counts");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "bounded work"
    );
    // A small lease under the same rules is told as usual: in none of them.
    let small = classes_of(&w, &["small/**"]).await;
    assert!(
        small.unknown.is_none() && small.hits.is_empty(),
        "{:?}",
        small.unknown
    );
    // Many leases add up: the budget is per brief, not per lease.
    let many: Vec<String> = (0..600).map(|n| format!("big/f{n}.txt")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let all = classes_of(&w, &many).await;
    assert!(
        all.unknown.as_deref().unwrap().contains("too many"),
        "{:?}",
        all.unknown
    );
}

#[tokio::test]
async fn a_routing_that_fails_leaves_no_row_naming_an_engine() {
    let w = two().await;
    // A class named like the answer key's text makes the stored decision refuse itself (H1), after routing and before the row.
    commit_crew(
        &w.repo,
        &file(vec![rule(
            &super::harness::KEY_MARKER.to_lowercase(),
            &["src/**"],
            &["alpha", "bravo"],
        )]),
    );
    let mut leaky = key();
    leaky["checks"][1]["params"]["run"] = json!(super::harness::KEY_MARKER);
    let (answer, error) = w.start(brief(json!({})), leaky).await;
    assert!(error, "{answer}");
    let chosen: i64 = w
        .core
        .store
        .reader()
        .query_row(
            "SELECT COUNT(*) FROM crew_autopilot_events WHERE kind = 'route_class'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(chosen, 0);
}

#[tokio::test]
async fn a_recorded_routing_keeps_its_commit_blob_and_classes_when_head_and_the_file_move() {
    let w = two().await;
    commit_crew(&w.repo, &file(vec![rule("keep", &["src/**"], &["bravo"])]));
    let head = git_out(&w.repo, &["rev-parse", "HEAD"]);
    let blob = git_out(&w.repo, &["rev-parse", "HEAD:.orbyn/crew.json"]);
    let (answer, error) = w.start(brief(json!({})), key()).await;
    assert!(!error, "{answer}");
    let id = answer["task"].as_str().unwrap().to_owned();
    // The file changes in the checkout, is committed, and HEAD then goes back to an earlier commit.
    std::fs::write(w.repo.join(".orbyn/crew.json"), file(vec![])).unwrap();
    run_git(&w.repo, &["add", "-f", ".orbyn/crew.json"]);
    commit(&w.repo, "drop the rules");
    run_git(&w.repo, &["reset", "-q", "--hard", "HEAD~2"]);
    let view = w.core.crew_route_view(&id).unwrap();
    let row = &view.rows[0];
    assert_eq!(row.commit.as_deref(), Some(head.as_str()));
    assert_eq!(row.blob.as_deref(), Some(blob.as_str()));
    assert_eq!(row.classes[0].name, "keep");
    assert_eq!(view.decision.unwrap().engine.as_str(), "bravo");
}
