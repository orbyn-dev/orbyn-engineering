//! Owned temporary Git repositories, fake forge replies and the normal fake-engine admission path.
//! No installed forge CLI, account, network remote or provider is used.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::autopilot_merge::{gitq, passed};
use super::harness::{Turn, World, report, world_forge};
use super::review_support::commit_all;
use crate::crew::{MergeOutcome, MergeTarget, ReviewAnswer};
use crate::error::CoreError;
use crate::source_control::forge::{Forge, with_crew_fixture};
use crate::store::ForgePhase;
use crate::testing::{TempDir, run_git};

const NOW: i64 = 1_800_000_000;

struct Setup {
    w: World,
    remote: TempDir,
    target: MergeTarget,
    task: String,
}

async fn setup() -> Setup {
    setup_with_rules(r#"{"checks":[{"run":"echo repository-check"}]}"#).await
}

async fn setup_with_rules(rules: &str) -> Setup {
    let w = world_forge(Arc::new(|turn, repo| {
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(
            repo.join("src/feature.txt"),
            format!("implemented {turn}\n"),
        )
        .unwrap();
        commit_all(repo, "feature");
        Turn::Report(report("done"))
    }))
    .await;
    // The checks are committed before the task starts; the candidate cannot define its own verifier.
    std::fs::create_dir_all(w.repo.join(".orbyn")).unwrap();
    std::fs::write(w.repo.join(".orbyn/crew.json"), rules).unwrap();
    commit_all(&w.repo, "repository checks");
    let branch = gitq(&w.repo, &["branch", "--show-current"]).await;
    let remote = TempDir::new();
    run_git(remote.path(), &["init", "--bare", "-q"]);
    run_git(
        &w.repo,
        &["remote", "add", "origin", &remote.path().to_string_lossy()],
    );
    run_git(
        &w.repo,
        &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
    );
    let target = MergeTarget {
        repo: w.repo.clone(),
        branch,
        owed: Some(Vec::new()),
    };
    let task = passed(&w, &["src/**"], "Implement a small feature.").await;
    Setup {
        w,
        remote,
        target,
        task,
    }
}

#[derive(Default)]
struct RemoteState {
    base: String,
    head: String,
    source: String,
    branch: String,
    pending: bool,
    published: bool,
    merged: Option<String>,
    creates: usize,
    merges: usize,
    unknown_merge: bool,
    unknown_create: bool,
    unreadable_once: bool,
    wrong_merge_commit: bool,
    /// The pull reads so far, and the read after which the fixture withdraws the escape check of the task (0: never). Reads belong to
    /// the pass that is running, so a test arms it for the pass it wants to interrupt.
    pull_reads: usize,
    withdraw_on_read: usize,
}

type Fake =
    Arc<dyn Fn(Forge, &std::path::Path, &[&str]) -> crate::error::Result<String> + Send + Sync>;

async fn fake(setup: &Setup) -> (Arc<Mutex<RemoteState>>, Fake) {
    let state = Arc::new(Mutex::new(RemoteState {
        base: gitq(
            setup.remote.path(),
            &["rev-parse", &format!("refs/heads/{}", setup.target.branch)],
        )
        .await,
        head: setup
            .w
            .core
            .store
            .crew_results(&setup.task)
            .unwrap()
            .last()
            .unwrap()
            .commit
            .as_ref()
            .unwrap()
            .as_str()
            .to_owned(),
        source: format!("crew/{}", setup.task.replace(':', "-")),
        branch: setup.target.branch.clone(),
        ..RemoteState::default()
    }));
    let state_in = state.clone();
    let core = setup.w.core.clone();
    let task = setup.task.clone();
    let remote = setup.remote.path().to_owned();
    let repo = setup.w.repo.clone();
    let fixture: Fake = Arc::new(move |forge, _, args| {
        assert_eq!(forge, Forge::GitHub);
        // A fake with hostile GH_REPO/GH_HOST/default-repo settings accepts only explicit selectors.
        if args.first() == Some(&"api") {
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--hostname", "fixture.invalid"])
            );
            assert!(args[1].starts_with("repos/fixture/repo/"));
        } else {
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--repo", "fixture.invalid/fixture/repo"])
            );
        }
        let mut s = state_in.lock().unwrap();
        let value = match args {
            ["pr", "create", ..] => {
                s.creates += 1;
                s.published = true;
                if s.unknown_create {
                    return Err(CoreError::Git("unknown fake create response".into()));
                }
                return Ok("https://example.invalid/repo/pull/7".into());
            }
            ["pr", "merge", "7", ..] => {
                s.merges += 1;
                assert!(
                    args.windows(2)
                        .any(|pair| pair == ["--match-head-commit", s.head.as_str()])
                );
                let intent = core.store.crew_forge_merge(&task).unwrap().unwrap();
                let merged = if s.wrong_merge_commit {
                    intent.candidate
                } else {
                    intent.combined.unwrap()
                };
                // This is the fake server's repository, never a real forge or the app's main checkout.
                run_git(&remote, &["fetch", "-q", &repo.to_string_lossy(), &merged]);
                run_git(
                    &remote,
                    &["update-ref", &format!("refs/heads/{}", s.branch), &merged],
                );
                s.merged = Some(merged);
                if s.unknown_merge {
                    s.unreadable_once = true;
                    return Err(CoreError::Git("unknown fake merge response".into()));
                }
                return Ok(String::new());
            }
            ["api", path, ..] if path.contains("/pulls?state=") => {
                if s.published {
                    json!([{"number":7,"head":{"ref":s.source,"repo":{"full_name":"fixture/repo"}},"base":{"ref":s.branch,"repo":{"full_name":"fixture/repo"}}}])
                } else {
                    json!([])
                }
            }
            ["api", path, ..] if path.ends_with("/pulls/7") => {
                s.pull_reads += 1;
                if s.withdraw_on_read != 0 && s.pull_reads == s.withdraw_on_read {
                    // The person withdraws the check at this exact point of the merge, straight in the store.
                    core.store
                        .reader()
                        .execute(
                            "UPDATE crew_escape_checks SET state = 'withdrawn' WHERE task_id = ?1",
                            [&task],
                        )
                        .unwrap();
                }
                if s.unreadable_once {
                    s.unreadable_once = false;
                    return Err(CoreError::Git("fake read interrupted".into()));
                }
                json!({"number":7,"state": if s.merged.is_some() {"closed"} else {"open"},"merged":s.merged.is_some(),"merge_commit_sha":s.merged,"head":{"ref":s.source,"sha":s.head,"repo":{"full_name":"fixture/repo"}},"base":{"ref":s.branch,"sha":s.base,"repo":{"full_name":"fixture/repo"}}})
            }
            ["api", path, ..] if path.contains("/check-runs?") => {
                json!({"check_runs":[{"name":"ci","head_sha":s.head,"status":if s.pending {"in_progress"} else {"completed"},"conclusion":if s.pending {Value::Null} else {json!("success")}}]})
            }
            ["api", path, ..] if path.contains("/status?") => {
                json!({"sha":s.head,"total_count":0,"statuses":[]})
            }
            ["api", path, ..] if path.ends_with(&format!("/commits/{}", s.branch)) => {
                json!({"sha":s.merged.as_ref().unwrap_or(&s.base)})
            }
            _ => panic!("unexpected fake forge call: {args:?}"),
        };
        Ok(value.to_string())
    });
    (state, fixture)
}

#[tokio::test]
async fn forge_merge_pending_ci_cannot_merge_and_unknown_response_reconciles_once() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        state.lock().unwrap().pending = true;
        let pending =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW)
                .await
                .unwrap();
        assert!(matches!(pending.as_slice(), [MergeOutcome::Blocked { .. }]));
        assert_eq!(state.lock().unwrap().merges, 0);
        {
            let mut remote = state.lock().unwrap();
            remote.pending = false;
            remote.unknown_merge = true;
        }
        assert!(
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 1)
                .await
                .is_err()
        );
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::MergeRequested
        );
        // Simulate the crash window after the exact receipt is durable but before Review/watch projection.
        let mut intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
        intent.phase = ForgePhase::Merged;
        intent.merge_commit = state.lock().unwrap().merged.clone();
        s.w.core.store.save_crew_forge_merge(&mut intent).unwrap();
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_none());
        // Default None means no periodic scheduling; startup still reconciles already requested effects.
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert!(
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 2)
                .await
                .unwrap()
                .is_empty()
        );
        {
            let remote = state.lock().unwrap();
            assert_eq!((remote.creates, remote.merges), (1, 1));
        }
        assert!(
            s.w.core
                .store
                .crew_gates(&s.task)
                .unwrap()
                .iter()
                .any(|g| g.stage == orbyn_contracts::GateStage::Merge)
        );
        let record = s.w.core.store.crew_merge(&s.task).unwrap().unwrap();
        assert_eq!(record.watch, "green");
        let events = s.w.core.store.events_after(&s.w.lead, 0, 1000).unwrap();
        s.w.core.crew_deliver_merge_notices(NOW + 3).unwrap();
        assert_eq!(
            s.w.core
                .store
                .events_after(&s.w.lead, 0, 1000)
                .unwrap()
                .len(),
            events.len()
        );
        // Even a direct compatibility call cannot move the local branch for a forge intent.
        let local = gitq(&s.w.repo, &["rev-parse", &s.target.branch]).await;
        assert!(
            s.w.core
                .crew_process_merges(&s.target, NOW + 4)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            gitq(&s.w.repo, &["rev-parse", &s.target.branch]).await,
            local
        );
        assert!(
            s.w.core
                .crew_answer_review(&s.task, ReviewAnswer::Revert, &s.target, NOW + 5)
                .await
                .is_err()
        );
    })
    .await;
}

#[tokio::test]
async fn a_moved_forge_base_stops_before_publication() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        state.lock().unwrap().base = "c".repeat(40);
        let result =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW)
                .await
                .unwrap();
        assert!(matches!(
            result.as_slice(),
            [MergeOutcome::NeedsPerson { .. }]
        ));
        assert_eq!(state.lock().unwrap().creates, 0);
    })
    .await;
}

#[tokio::test]
async fn merge_tools_cannot_answer_for_the_person_or_set_ci_policy() {
    let s = setup().await;
    for forbidden in [
        json!({"task":s.task,"answer":"looks_good"}),
        json!({"task":s.task,"required_checks":[]}),
        json!({"task":s.task,"released":true}),
    ] {
        let (_, error) =
            s.w.call(&s.w.lead_token, "crew_answer_review", forbidden)
                .await;
        assert!(error);
    }
    let worker_token = super::harness::bind(&s.w.core, &s.w.worker(&s.task), "run-worker-attempt");
    for tool in crate::crew::MERGE_TOOLS {
        let (_, error) =
            s.w.call(
                &worker_token,
                tool,
                json!({"task":s.task,"branch":s.target.branch}),
            )
            .await;
        assert!(error, "a worker used {tool}");
    }
    assert!(
        s.w.core
            .merge_gate_from_evidence(&s.task, &[], &"a".repeat(40), &"b".repeat(40), NOW)
            .is_err()
    );
}

#[tokio::test]
async fn unknown_publication_finds_the_existing_pr_without_a_second_create() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        {
            let mut remote = state.lock().unwrap();
            remote.unknown_create = true;
            remote.pending = true;
        }
        assert!(
            s.w.core
                .crew_process_forge_merges(&s.target, NOW)
                .await
                .is_err()
        );
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::Publishing
        );
        let resumed =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 1)
                .await
                .unwrap();
        assert!(matches!(resumed.as_slice(), [MergeOutcome::Blocked { .. }]));
        let remote = state.lock().unwrap();
        assert_eq!((remote.creates, remote.merges), (1, 0));
    })
    .await;
}

#[tokio::test]
async fn lowering_the_mode_holds_forge_publication() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        s.w.core
            .set_crew_autopilot_mode(&crate::crew::ModeScope::Default, crate::crew::Mode::Off)
            .unwrap();
        let held =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW)
                .await
                .unwrap();
        assert!(matches!(held.as_slice(), [MergeOutcome::Blocked { .. }]));
        let remote = state.lock().unwrap();
        assert_eq!((remote.creates, remote.merges), (0, 0));
    })
    .await;
}

#[tokio::test]
async fn person_release_does_not_remove_a_locked_review_lane() {
    let s = setup_with_rules(
        r#"{"checks":[{"run":"echo repository-check"}],"lanes":[{"lane":"correctness","locked":true}]}"#,
    )
    .await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core.set_crew_autopilot_mode(&crate::crew::ModeScope::Default, crate::crew::Mode::Off).unwrap();
        s.w.core.crew_queue_forge_merge(&s.task, &s.target, NOW).await.unwrap();
        s.w.core.crew_release_merge(&s.task).unwrap();
        let result = s.w.core.crew_process_forge_merges(&s.target, NOW).await.unwrap();
        assert!(matches!(result.as_slice(), [MergeOutcome::Blocked { why, .. }] if why.contains("locked")));
        let remote = state.lock().unwrap();
        assert_eq!((remote.creates, remote.merges), (0, 0));
    }).await;
}

#[tokio::test]
async fn a_changed_published_head_cannot_use_previous_ci() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        state.lock().unwrap().pending = true;
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
        {
            let mut remote = state.lock().unwrap();
            remote.pending = false;
            remote.head = "d".repeat(40);
        }
        let result =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 1)
                .await
                .unwrap();
        assert!(matches!(
            result.as_slice(),
            [MergeOutcome::NeedsPerson { .. }]
        ));
        assert_eq!(state.lock().unwrap().merges, 0);
    })
    .await;
}

#[tokio::test]
async fn actual_merge_mismatch_is_durable_with_the_review_record() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .set_crew_autopilot_mode(&crate::crew::ModeScope::Default, crate::crew::Mode::Off)
            .unwrap();
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        s.w.core.crew_release_merge(&s.task).unwrap();
        state.lock().unwrap().wrong_merge_commit = true;
        let result =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW)
                .await
                .unwrap();
        assert!(matches!(result.as_slice(), [MergeOutcome::Merged { .. }]));
        let record = s.w.core.store.crew_merge(&s.task).unwrap().unwrap();
        assert!(record.needs_person.as_deref().unwrap().contains("differs"));
        assert_eq!(record.review_state, "waiting");
        assert!(
            s.w.core
                .crew_review_queue(NOW + 1)
                .unwrap()
                .iter()
                .any(|item| item.task_id == s.task && item.needs_person.is_some())
        );
        assert_eq!(
            s.w.core.store.merge_entry(&s.task).unwrap().unwrap().state,
            "merged"
        );
    })
    .await;
}

#[tokio::test]
async fn later_merge_evidence_and_gates_do_not_change_the_passed_review_commit() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let task = s.w.core.store.crew_task(&s.task).unwrap().unwrap();
        let original = s.w.core.verification_evidence(&task).unwrap();
        let candidate = s.w.core.review_commit(&task, &s.w.repo).await.unwrap();
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        state.lock().unwrap().pending = true;
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
        let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
        assert_ne!(intent.combined.as_deref(), Some(candidate.as_str()));
        assert!(s.w.core.store.crew_evidence(&s.task).unwrap().len() > original.len());
        let mut gate =
            s.w.core
                .store
                .crew_gates(&s.task)
                .unwrap()
                .last()
                .unwrap()
                .clone();
        gate.stage = orbyn_contracts::GateStage::Merge;
        gate.evidence = intent
            .evidence
            .iter()
            .map(|id| orbyn_contracts::CrewHash::new(id.clone()).unwrap())
            .collect();
        gate.id = orbyn_contracts::CrewObject::Gate(gate.clone())
            .computed_id()
            .unwrap();
        s.w.core
            .store
            .add_crew_object(&s.task, &orbyn_contracts::CrewObject::Gate(gate))
            .unwrap();
        run_git(&s.w.repo, &["checkout", "--detach", "-q", &intent.base]);
        assert_eq!(
            s.w.core.review_commit(&task, &s.w.repo).await.unwrap(),
            candidate
        );
        assert_eq!(s.w.core.verification_evidence(&task).unwrap(), original);
    })
    .await;
}

#[tokio::test]
async fn ordinary_owed_lane_needs_the_candidate_but_a_person_can_stand_in() {
    let s = setup().await;
    let (_, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .set_crew_autopilot_mode(&crate::crew::ModeScope::Default, crate::crew::Mode::Off)
            .unwrap();
        let mut target = s.target.clone();
        target.owed = Some(vec!["correctness".into()]);
        let entry =
            s.w.core
                .crew_queue_forge_merge(&s.task, &target, NOW)
                .await
                .unwrap();
        let old_commit =
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .base;
        // A synthetic stored lane is passed, but its commit is stale. State alone must not authorize this candidate.
        let reviewer = passed(&s.w, &["src/**"], "Synthetic lane fixture.").await;
        s.w.core
            .store
            .add_crew_review(&crate::store::CrewReview {
                task_id: reviewer,
                subject_id: s.task.clone(),
                lane: "correctness".into(),
                role: "review".into(),
                round: 1,
                commit: old_commit,
                check_of: None,
            })
            .unwrap();
        assert!(
            s.w.core
                .forge_review_precondition(&entry, &target)
                .await
                .unwrap()
                .unwrap()
                .contains("current candidate")
        );
        s.w.core.crew_release_merge(&s.task).unwrap();
        let released = s.w.core.store.merge_entry(&s.task).unwrap().unwrap();
        assert_eq!(
            s.w.core
                .forge_review_precondition(&released, &target)
                .await
                .unwrap(),
            None
        );
    })
    .await;
}

#[tokio::test]
async fn another_crew_cannot_overtake_an_unknown_publication() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core.crew_queue_forge_merge(&s.task, &s.target, NOW).await.unwrap();
        state.lock().unwrap().unknown_create = true;
        assert!(s.w.core.crew_process_forge_merges(&s.target, NOW).await.is_err());
        let project = s.w.core.store.thread(&s.w.lead).unwrap().project_id;
        let other = s.w.core.create_thread(orbyn_contracts::CreateThread {
            project_id: Some(project), engine: "alpha".into(), model: None, title: Some("other lead".into()),
            worktree: None, skill_profile: None, base_branch: None, effort: None, options: Default::default(),
        }).await.unwrap();
        let token = super::harness::bind(&s.w.core, &other.id, "other-lead-run");
        let (answer, error) = s.w.call(&token, "crew_start", json!({"brief":super::harness::brief(json!({"files":[{"path":"src/**","mode":"write"}],"outcome":"Later queued work."})),"answer_key":{"checks":[{"id":"ok","kind":"command","params":{"run":"echo other-check"},"proves":"It runs."}],"protected_paths":[]}})).await;
        assert!(!error, "{answer}");
        let other_task = answer["task"].as_str().unwrap();
        s.w.until_state(other_task, "passed").await;
        s.w.core.crew_queue_forge_merge(other_task, &s.target, NOW + 1).await.unwrap();
        let before = state.lock().unwrap().creates;
        let result = s.w.core.process_forge_merges_for(&s.target, Some(&other.id), NOW + 2).await.unwrap();
        assert!(matches!(result.as_slice(), [MergeOutcome::Blocked { why, .. }] if why.contains("another crew")));
        assert_eq!(state.lock().unwrap().creates, before);
    }).await;
}

#[tokio::test]
async fn person_comment_admits_a_scoped_fix_through_the_normal_fake_engine() {
    let s = setup().await;
    let (_, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
        let record = s.w.core.store.crew_merge(&s.task).unwrap().unwrap();
        run_git(
            &s.w.repo,
            &["checkout", "--detach", "-q", &record.merge_commit],
        );
        let before =
            s.w.core
                .store
                .crew_work_tasks(&s.w.lead, 100)
                .unwrap()
                .len();
        s.w.core
            .crew_answer_review(
                &s.task,
                ReviewAnswer::Comment(
                    "Correct the small feature within its existing scope.".into(),
                ),
                &s.target,
                NOW + 1,
            )
            .await
            .unwrap();
        let tasks = s.w.core.store.crew_work_tasks(&s.w.lead, 100).unwrap();
        assert_eq!(tasks.len(), before + 1);
        let fix = tasks.iter().find(|task| task.id != s.task).unwrap();
        let original = s.w.core.store.crew_brief(&s.task).unwrap().unwrap();
        let admitted = s.w.core.store.crew_brief(&fix.id).unwrap().unwrap();
        assert_eq!(admitted.files, original.files);
        assert_eq!(admitted.allowed_effects, original.allowed_effects);
        let key = s.w.core.store.crew_answer_key(&fix.id).unwrap().unwrap();
        assert!(
            key.checks
                .iter()
                .any(|check| check.id.as_str() == "merge-comment-scope")
        );
        s.w.until_state(&fix.id, "passed").await;
        // The same stored Comment cannot admit another task after a restart/retry.
        let (_, error) =
            s.w.call(
                &s.w.lead_token,
                "crew_answer_review",
                json!({"task":s.task,"branch":s.target.branch}),
            )
            .await;
        assert!(!error);
        assert_eq!(
            s.w.core
                .store
                .crew_work_tasks(&s.w.lead, 100)
                .unwrap()
                .len(),
            before + 1
        );
    })
    .await;
}

/// A check row for the task, as a proven check of its own escape (the foreign key to the escape log is off on this connection: the
/// merge side reads only the check and the task record).
fn make_proven_check(s: &Setup, commit: &str) {
    let conn = s.w.core.store.reader();
    conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    conn.execute(
        "INSERT INTO crew_escape_check_tasks (task_id, escape_id, created_unix) VALUES (?1, 'esc_forge', 1)",
        [&s.task],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO crew_escape_checks (escape_id, project_id, path, asserts, run, fix_commit, task_id, state, attempts, diff_commit, diff_hash, diff_paths, proposed_unix, updated_unix)
         VALUES ('esc_forge', 'p', 'tests/a.txt', 'a', 'r', ?1, ?2, 'proven', 1, ?3, ?4, '[\"tests/a.txt\"]', 1, 1)",
        rusqlite::params!["f".repeat(40), s.task, commit, "0".repeat(64)],
    )
    .unwrap();
}

#[tokio::test]
async fn a_check_withdrawn_just_before_the_merge_request_is_saved_never_reaches_the_forge() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        s.w.core.crew_queue_forge_merge(&s.task, &s.target, NOW).await.unwrap();
        // The first pass publishes and waits for CI. The second runs to the final write, and the check is withdrawn after the
        // re-check that comes before it (the last pull read of the pass).
        state.lock().unwrap().pending = true;
        s.w.core.crew_process_forge_merges(&s.target, NOW).await.unwrap();
        {
            let mut remote = state.lock().unwrap();
            remote.pending = false;
            remote.withdraw_on_read = remote.pull_reads + WITHDRAW_AT;
        }
        let outcomes = s.w.core.crew_process_forge_merges(&s.target, NOW + 1).await;
        let outcomes = outcomes.expect("a failed re-check answers Blocked, never an error");
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::Blocked { why, .. }] if why.contains("withdrawn")),
            "{outcomes:?}"
        );
        assert_eq!(state.lock().unwrap().merges, 0, "the forge was never asked to merge");
        let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
        assert_eq!(intent.phase, ForgePhase::NeedsPerson);
        assert!(intent.note.unwrap().contains("withdrawn"));
        assert_eq!(s.w.core.store.merge_entry(&s.task).unwrap().unwrap().state, "blocked");
        let check_state: String = s.w.core.store.reader()
            .query_row("SELECT state FROM crew_escape_checks WHERE task_id = ?1", [&s.task], |r| r.get(0))
            .unwrap();
        assert_eq!(check_state, "withdrawn");
        // Another pass stays blocked and still asks the forge for nothing.
        let again = s.w.core.crew_process_forge_merges(&s.target, NOW + 2).await.unwrap();
        assert!(!matches!(again.as_slice(), [MergeOutcome::Merged { .. }]));
        assert_eq!(state.lock().unwrap().merges, 0);
    })
    .await;
}

#[tokio::test]
async fn a_check_proven_on_another_commit_cannot_merge_on_the_forge() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    make_proven_check(&s, &"a".repeat(40));
    with_crew_fixture(fixture, async {
        let queued =
            s.w.core
                .crew_queue_forge_merge(&s.task, &s.target, NOW)
                .await;
        let why = queued
            .expect_err("the queued commit is not the proven one")
            .public_message();
        assert!(why.contains("different commit"), "{why}");
        assert_eq!(state.lock().unwrap().creates, 0);
    })
    .await;
}

/// Which pull read of the second pass the withdraw follows: the receipt read comes first, then (after the re-check) the fresh one.
const WITHDRAW_AT: usize = 2;

// A forge merge in `needs_person`: the lane goes on past it, and a person's answer is the only way out of it.

fn answer(
    action: orbyn_contracts::CrewForgeAction,
    seen: i64,
) -> orbyn_contracts::CrewForgeResolve {
    orbyn_contracts::CrewForgeResolve {
        action,
        seen_revision: seen,
    }
}

fn counts(state: &Arc<Mutex<RemoteState>>) -> (usize, usize) {
    let remote = state.lock().unwrap();
    (remote.creates, remote.merges)
}

fn audit_rows(s: &Setup, task: &str) -> usize {
    s.w.core
        .store
        .list_audit(None, 500)
        .unwrap()
        .iter()
        .filter(|row| row.action == "crew.merge.forge.resolve" && row.target == task)
        .count()
}

/// A second passed task, queued first and put in `needs_person` the way the pipeline leaves it (no PR). Its candidate is not the
/// fake forge's, so any forge call for it would panic or be counted.
async fn stuck_head(s: &Setup) -> (String, i64) {
    let head = passed(&s.w, &["src/**"], "An earlier change.").await;
    s.w.core
        .crew_queue_forge_merge(&head, &s.target, NOW - 10)
        .await
        .unwrap();
    let mut intent = s.w.core.store.crew_forge_merge(&head).unwrap().unwrap();
    intent.phase = ForgePhase::NeedsPerson;
    intent.note = Some("the target base moved".into());
    s.w.core.store.save_crew_forge_merge(&mut intent).unwrap();
    s.w.core
        .store
        .set_merge_entry_state(&head, "blocked", Some("the target base moved"))
        .unwrap();
    (head, intent.revision)
}

#[tokio::test]
async fn a_needs_person_head_does_not_block_the_lane() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let (head, _) = stuck_head(&s).await;
        s.w.core.crew_queue_forge_merge(&s.task, &s.target, NOW).await.unwrap();
        let outcomes = s.w.core.crew_process_forge_merges(&s.target, NOW).await.unwrap();
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::NeedsPerson { task_id, .. }, MergeOutcome::Merged { .. }] if *task_id == head),
            "{outcomes:?}"
        );
        assert_eq!(counts(&state), (1, 1));
        // Nothing of the stuck entry moved, and it is still listed for a person.
        let intent = s.w.core.store.crew_forge_merge(&head).unwrap().unwrap();
        assert_eq!(intent.phase, ForgePhase::NeedsPerson);
        assert_eq!(s.w.core.store.merge_entry(&head).unwrap().unwrap().state, "blocked");
        let queue = s.w.core.autopilot_merge_queue(crate::crew::Page::new(None, None).unwrap()).unwrap();
        let row = queue.entries.iter().find(|e| e.task_id == head).unwrap();
        assert_eq!(row.forge.as_ref().map(|f| f.phase.as_str()), Some("needs_person"));
        assert_eq!(row.forge.as_ref().map(|f| f.revision), Some(intent.revision));
        // The next pass keeps skipping it, without a forge call.
        s.w.core.crew_process_forge_merges(&s.target, NOW + 1).await.unwrap();
        assert_eq!(counts(&state), (1, 1));
    })
    .await;
}

#[tokio::test]
async fn rejecting_a_needs_person_intent_frees_the_lane_and_sends_nothing() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let (head, revision) = stuck_head(&s).await;
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        // A view that is out of date is refused and changes nothing.
        let stale =
            s.w.core
                .crew_forge_resolve(
                    &head,
                    &answer(orbyn_contracts::CrewForgeAction::Reject, revision + 1),
                )
                .await;
        assert!(matches!(stale, Err(CoreError::Stale(_))), "{stale:?}");
        assert_eq!(audit_rows(&s, &head), 0);
        let done =
            s.w.core
                .crew_forge_resolve(
                    &head,
                    &answer(orbyn_contracts::CrewForgeAction::Reject, revision),
                )
                .await
                .unwrap();
        assert_eq!(
            (done.outcome.as_str(), done.phase.as_str()),
            ("rejected", "rejected")
        );
        assert_eq!(audit_rows(&s, &head), 1);
        assert_eq!(
            s.w.core.store.merge_entry(&head).unwrap().unwrap().state,
            "rejected"
        );
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&head)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::Rejected
        );
        assert_eq!(counts(&state), (0, 0), "nothing was sent to the forge");
        // The lane has only the next entry, and it merges.
        let outcomes =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 1)
                .await
                .unwrap();
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::Merged { .. }]),
            "{outcomes:?}"
        );
        // The same answer twice is stale: the intent is no longer waiting.
        let again =
            s.w.core
                .crew_forge_resolve(
                    &head,
                    &answer(orbyn_contracts::CrewForgeAction::Reject, revision + 1),
                )
                .await;
        assert!(matches!(again, Err(CoreError::Stale(_))));
        assert_eq!(audit_rows(&s, &head), 1);
    })
    .await;
}

/// The check task of the setup is withdrawn at the last read, so the intent is in `needs_person` with its PR (nothing was merged).
async fn stuck_with_pr(s: &Setup, state: &Arc<Mutex<RemoteState>>) -> i64 {
    s.w.core
        .crew_queue_forge_merge(&s.task, &s.target, NOW)
        .await
        .unwrap();
    state.lock().unwrap().pending = true;
    s.w.core
        .crew_process_forge_merges(&s.target, NOW)
        .await
        .unwrap();
    {
        let mut remote = state.lock().unwrap();
        remote.pending = false;
        remote.withdraw_on_read = remote.pull_reads + WITHDRAW_AT;
    }
    s.w.core
        .crew_process_forge_merges(&s.target, NOW + 1)
        .await
        .unwrap();
    let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
    assert_eq!(
        (intent.phase, intent.pull),
        (ForgePhase::NeedsPerson, Some(7))
    );
    intent.revision
}

fn set_check(s: &Setup, state: &str) {
    s.w.core
        .store
        .reader()
        .execute(
            "UPDATE crew_escape_checks SET state = ?2 WHERE task_id = ?1",
            rusqlite::params![s.task, state],
        )
        .unwrap();
}

#[tokio::test]
async fn a_retry_runs_the_guard_again_and_merges_only_when_the_check_is_proven() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        let revision = stuck_with_pr(&s, &state).await;
        let done = s.w.core.crew_forge_resolve(&s.task, &answer(orbyn_contracts::CrewForgeAction::Retry, revision)).await.unwrap();
        assert_eq!((done.outcome.as_str(), done.phase.as_str()), ("retry", "awaiting_ci"));
        assert_eq!(s.w.core.store.merge_entry(&s.task).unwrap().unwrap().state, "queued");
        // The check is still withdrawn: the guard refuses again, and nothing reaches the forge.
        let blocked = s.w.core.crew_process_forge_merges(&s.target, NOW + 2).await.unwrap();
        assert!(matches!(blocked.as_slice(), [MergeOutcome::Blocked { why, .. }] if why.contains("withdrawn")), "{blocked:?}");
        assert_eq!(state.lock().unwrap().merges, 0);
        assert_eq!(s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap().phase, ForgePhase::AwaitingCi);
        // Once the check is proven again for this commit, the same intent merges.
        set_check(&s, "proven");
        let merged = s.w.core.crew_process_forge_merges(&s.target, NOW + 3).await.unwrap();
        assert!(matches!(merged.as_slice(), [MergeOutcome::Merged { .. }]), "{merged:?}");
        assert_eq!(state.lock().unwrap().merges, 1);
    })
    .await;
}

/// What a person merging the PR by hand does on the fake forge.
fn merge_by_hand(s: &Setup, state: &Arc<Mutex<RemoteState>>) {
    let combined =
        s.w.core
            .store
            .crew_forge_merge(&s.task)
            .unwrap()
            .unwrap()
            .combined
            .unwrap();
    let branch = s.target.branch.clone();
    run_git(
        s.remote.path(),
        &["fetch", "-q", &s.w.repo.to_string_lossy(), &combined],
    );
    run_git(
        s.remote.path(),
        &["update-ref", &format!("refs/heads/{branch}"), &combined],
    );
    state.lock().unwrap().merged = Some(combined);
}

#[tokio::test]
async fn a_pr_merged_by_hand_on_the_forge_settles_the_lane_without_a_person() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        stuck_with_pr(&s, &state).await;
        merge_by_hand(&s, &state);
        let outcomes =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 2)
                .await
                .unwrap();
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::Merged { .. }]),
            "{outcomes:?}"
        );
        assert_eq!(
            state.lock().unwrap().merges,
            0,
            "the forge was never asked to merge"
        );
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::Merged
        );
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_some());
        assert_eq!(audit_rows(&s, &s.task), 1);
    })
    .await;
}

#[tokio::test]
async fn a_person_answer_on_a_pr_already_merged_by_hand_records_the_merge_instead() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        let revision = stuck_with_pr(&s, &state).await;
        merge_by_hand(&s, &state);
        let done =
            s.w.core
                .crew_forge_resolve(
                    &s.task,
                    &answer(orbyn_contracts::CrewForgeAction::Reject, revision),
                )
                .await
                .unwrap();
        assert_eq!(
            (done.outcome.as_str(), done.phase.as_str()),
            ("merged", "merged")
        );
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_some());
        assert_eq!(
            s.w.core.store.merge_entry(&s.task).unwrap().unwrap().state,
            "merged"
        );
    })
    .await;
}

#[tokio::test]
async fn a_crash_after_the_merged_write_leaves_a_state_recovery_finishes() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        let revision = stuck_with_pr(&s, &state).await;
        merge_by_hand(&s, &state);
        // The first write of settling a hand merge, and then the process dies before the merge is recorded.
        let commit = state.lock().unwrap().merged.clone().unwrap();
        s.w.core
            .store
            .resolve_crew_forge_merge(
                &s.task,
                revision,
                &crate::store::ForgeResolution::Merged {
                    merge_commit: commit,
                },
                "core",
            )
            .unwrap();
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_none());
        assert_eq!(s.w.core.store.crew_forge_unfinished().unwrap().len(), 1);
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_some());
        assert!(s.w.core.store.crew_forge_unfinished().unwrap().is_empty());
    })
    .await;
}

#[tokio::test]
async fn a_retry_with_a_closed_pr_is_refused_and_leaves_it_resolvable() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        let revision = stuck_with_pr(&s, &state).await;
        // The head moved on the forge: the PR is not the one the intent pinned.
        state.lock().unwrap().head = "d".repeat(40);
        let refused =
            s.w.core
                .crew_forge_resolve(
                    &s.task,
                    &answer(orbyn_contracts::CrewForgeAction::Retry, revision),
                )
                .await;
        assert!(matches!(refused, Err(CoreError::Invalid(_))), "{refused:?}");
        assert_eq!(audit_rows(&s, &s.task), 0);
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::NeedsPerson
        );
        // A person can still drop it.
        let done =
            s.w.core
                .crew_forge_resolve(
                    &s.task,
                    &answer(orbyn_contracts::CrewForgeAction::Reject, revision),
                )
                .await
                .unwrap();
        assert_eq!(done.outcome, "rejected");
    })
    .await;
}
