//! Corrective forge tasks with owned Git repositories, fake engines and fake forge receipts.
use super::autopilot_merge::{gitq, passed};
use super::forge_merge::{NOW, Setup, fake, setup, setup_world};
use super::harness::{Turn, report};
use super::review_support::commit_all;
use crate::crew::{MergeTarget, ReviewAnswer};
use crate::source_control::forge::with_crew_fixture;
use crate::store::ForgePhase;
use crate::testing::run_git;
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn a_person_can_prepare_a_distinct_core_correction_without_moving_their_checkout() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
        let original = s.w.core.store.crew_merge(&s.task).unwrap().unwrap();
        let checkout = gitq(&s.w.repo, &["rev-parse", "HEAD"]).await;
        let branch = gitq(&s.w.repo, &["rev-parse", &s.target.branch]).await;
        let before =
            s.w.core
                .store
                .crew_work_tasks(&s.w.lead, 100)
                .unwrap()
                .len();
        assert_eq!(state.lock().unwrap().creates, 1);

        s.w.core
            .crew_answer_review(&s.task, ReviewAnswer::Revert, &s.target, NOW + 1)
            .await
            .unwrap();

        let tasks = s.w.core.store.crew_work_tasks(&s.w.lead, 100).unwrap();
        assert_eq!(tasks.len(), before + 1);
        let correction = tasks.iter().find(|task| task.id != s.task).unwrap();
        assert_eq!(correction.state, "passed");
        assert!(
            s.w.core
                .store
                .crew_route_decisions(&correction.id)
                .unwrap()
                .is_empty()
        );
        assert!(
            s.w.core
                .store
                .crew_run(&correction.id)
                .unwrap()
                .unwrap()
                .worker_thread_id
                .is_none()
        );
        let current = s.w.core.store.crew_merge(&s.task).unwrap().unwrap();
        assert_eq!(current.merge_commit, original.merge_commit);
        assert_ne!(current.review_state, "reverted");
        assert_eq!(gitq(&s.w.repo, &["rev-parse", "HEAD"]).await, checkout);
        assert_eq!(
            gitq(&s.w.repo, &["rev-parse", &s.target.branch]).await,
            branch
        );
        s.w.core
            .crew_answer_review(&s.task, ReviewAnswer::Revert, &s.target, NOW + 2)
            .await
            .unwrap();
        assert_eq!(
            s.w.core
                .store
                .crew_work_tasks(&s.w.lead, 100)
                .unwrap()
                .len(),
            tasks.len()
        );
        // The request is audited once, by the person, naming exactly the commit that will be pushed.
        let stored =
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .unwrap();
        let requested: Vec<_> =
            s.w.core
                .store
                .crew_autopilot_events(&s.task)
                .unwrap()
                .into_iter()
                .filter(|event| event.kind == "correction_requested")
                .collect();
        assert_eq!(requested.len(), 1, "{requested:?}");
        let body = &requested[0].body;
        assert_eq!(body["actor"], "person");
        assert_eq!(body["parent"], s.task.as_str());
        assert_eq!(body["parent_commit"], original.merge_commit.as_str());
        assert_eq!(body["base"], stored.base.as_str());
        assert_eq!(body["candidate"], stored.candidate.as_str());
        assert_eq!(body["tree"], stored.tree.as_str());
    })
    .await;
}

#[tokio::test]
async fn a_core_correction_earns_its_review_gate_and_reconciles_one_published_pr() {
    let w = super::harness::world_forge_review(Arc::new(|turn, repo| {
        if turn == 0 {
            std::fs::write(repo.join("src/feature.txt"), "feature to revert\n").unwrap();
            commit_all(repo, "feature");
            Turn::Report(report("done"))
        } else {
            super::review_support::reviewed(vec![])
        }
    }))
    .await;
    let s = setup_world(r#"{"checks":[{"run":"echo repository-check"}]}"#, w).await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
        let original = s.w.core.store.crew_merge(&s.task).unwrap().unwrap();
        let checkout = gitq(&s.w.repo, &["rev-parse", "HEAD"]).await;
        let target = MergeTarget {
            repo: s.target.repo.clone(),
            branch: s.target.branch.clone(),
            owed: None,
        };
        let pending =
            s.w.core
                .autopilot_answer(
                    &s.task,
                    &orbyn_contracts::AutopilotAnswerBody {
                        merge_commit: original.merge_commit.clone(),
                        text: None,
                    },
                    crate::crew::Answering::Revert,
                    NOW + 1,
                )
                .await
                .unwrap();
        assert_ne!(pending.review_state, "reverted");
        assert!(pending.revert_commit.is_none());
        let correction =
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .unwrap();
        assert_eq!(pending.candidate.as_deref(), Some(correction.candidate.as_str()));
        assert_eq!(pending.base.as_deref(), Some(correction.base.as_str()));
        let gate_before =
            s.w.core
                .store
                .merge_entry(&correction.task_id)
                .unwrap()
                .unwrap();
        assert!(
            s.w.core
                .forge_review_precondition(&gate_before, &target)
                .await
                .unwrap()
                .unwrap()
                .contains("correctness")
        );
        let (review, error) =
            s.w.lead(
                "crew_review",
                json!({"task":correction.task_id,"lanes":["correctness"]}),
            )
            .await;
        assert!(!error, "{review}");
        let reviewer = review["lanes"][0]["task"]
            .as_str()
            .expect("the normal reviewer starts");
        s.w.until_state(reviewer, "passed").await;
        let worker = s.w.core.store.crew_run(reviewer).unwrap().unwrap();
        assert_eq!(
            std::fs::canonicalize(worker.worktree.unwrap()).unwrap(),
            std::fs::canonicalize(&correction.checkout).unwrap()
        );
        assert_eq!(
            s.w.core.store.thread(&s.w.lead).unwrap().cwd,
            s.w.repo.to_string_lossy()
        );
        assert_eq!(gitq(&s.w.repo, &["rev-parse", "HEAD"]).await, checkout);
        s.w.until("the corrective lane Gate passes", |w| {
            w.core
                .review_lane_gate(&correction.task_id, "correctness", &correction.candidate)
                .unwrap()
                .is_some_and(|gate| gate.decision == orbyn_contracts::GateDecision::Pass)
        })
        .await;
        let intent =
            s.w.core
                .store
                .crew_forge_merge(&correction.task_id)
                .unwrap()
                .unwrap();
        {
            let mut remote = state.lock().unwrap();
            remote.base = correction.base.clone();
            remote.head = correction.candidate.clone();
            remote.source = intent.source.clone();
            remote.merged = None;
            remote.published = false;
            remote.pending = true;
            remote.unknown_create = true;
        }
        assert!(
            s.w.core
                .crew_process_forge_merges(&target, NOW + 2)
                .await
                .is_err()
        );
        assert_eq!(
            state.lock().unwrap().creates,
            2,
            "one original PR and one corrective PR"
        );
        // The forge text is fixed by code and names the original PR; no agent text and no mention.
        let create = state.lock().unwrap().create_args.clone();
        let title = create
            .windows(2)
            .find(|pair| pair[0] == "--title")
            .map(|pair| pair[1].clone())
            .unwrap();
        assert_eq!(title, format!("Revert crew merge {}", &original.merge_commit[..12]));
        let text = create
            .windows(2)
            .find(|pair| pair[0] == "--body")
            .map(|pair| pair[1].clone())
            .unwrap();
        assert!(text.contains("#7"), "{text}");
        assert!(!title.contains('@') && !text.contains('@'), "{text}");
        s.w.core
            .crew_process_forge_merges(&target, NOW + 3)
            .await
            .unwrap();
        assert_eq!(
            state.lock().unwrap().creates,
            2,
            "unknown publication reconciles instead of creating twice"
        );
        assert_ne!(
            s.w.core
                .store
                .crew_merge(&s.task)
                .unwrap()
                .unwrap()
                .review_state,
            "reverted"
        );
        {
            let mut remote = state.lock().unwrap();
            remote.pending = false;
            remote.unknown_merge = true;
        }
        // The person's route sends the merge request; startup afterwards only reads the unknown reply.
        // The fixture's reply to the merge request is lost, so this pass errors after the request was sent.
        let _ = s.w.core.crew_process_forge_merges(&target, NOW + 4).await;
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            state.lock().unwrap().merges,
            2,
            "one merge request per original/corrective PR despite an unknown reply"
        );
        let reverted = s.w.core.store.crew_merge(&s.task).unwrap().unwrap();
        assert_eq!(reverted.review_state, "reverted");
        assert_eq!(reverted.reverted_commit, state.lock().unwrap().merged);
        // Crash after the corrective merge record, before projecting the original parent.
        {
            let conn = s.w.core.store.reader();
            conn.execute(
                "UPDATE crew_forge_corrections SET completed_commit=NULL WHERE task_id=?1",
                [&correction.task_id],
            )
            .unwrap();
            conn.execute(
                "UPDATE crew_merges SET review_state='waiting',reverted_commit=NULL WHERE task_id=?1",
                [&s.task],
            )
            .unwrap();
            conn.execute(
                "DELETE FROM crew_autopilot_events WHERE task_id=?1 AND kind='reverted'",
                [&s.task],
            )
            .unwrap();

        }
        let merge_owner=s.w.core.crew_merges.lock().await;
        let mut recovery=Box::pin(s.w.core.resume_forge_corrections(NOW+5));
        tokio::select! {
            biased;
            result=&mut recovery=>panic!("startup projection must wait for the area/merge owner: {result:?}"),
            _=tokio::task::yield_now()=>{}
        }
        assert_eq!(s.w.core.store.crew_merge(&s.task).unwrap().unwrap().review_state,"waiting");
        drop(merge_owner);
        recovery.await.unwrap();
        assert_eq!(
            s.w.core
                .store
                .crew_merge(&s.task)
                .unwrap()
                .unwrap()
                .review_state,
            "reverted"
        );
        assert!(
            s.w.core
                .store
                .crew_pending_corrections()
                .unwrap()
                .is_empty()
        );

        // Crash after parent projection, with area projection unavailable. Completion must remain pending.
        {
            let conn = s.w.core.store.reader();
            conn.execute(
                "UPDATE crew_forge_corrections SET completed_commit=NULL WHERE task_id=?1",
                [&correction.task_id],
            )
            .unwrap();
            conn.execute(
                "DELETE FROM crew_autopilot_events WHERE task_id=?1 AND kind='reverted'",
                [&s.task],
            )
            .unwrap();
            conn.execute_batch("ALTER TABLE crew_area_modes RENAME TO correction_held_area_modes")
                .unwrap();

        }
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(s.w.core.store.crew_pending_corrections().unwrap().len(), 1);
        s.w.core
            .store
            .reader()
            .execute_batch("ALTER TABLE correction_held_area_modes RENAME TO crew_area_modes")
            .unwrap();
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert!(
            s.w.core
                .store
                .crew_pending_corrections()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            s.w.core
                .store
                .crew_autopilot_events(&s.task)
                .unwrap()
                .iter()
                .filter(|event| event.kind == "reverted")
                .count(),
            1
        );

        assert_eq!(state.lock().unwrap().creates, 2);
        assert!(
            s.w.core
                .store
                .crew_route_decisions(&correction.task_id)
                .unwrap()
                .is_empty()
        );
        // Complete and unread: the checkout and the object reference are gone, the person's checkout is untouched.
        assert!(!std::path::Path::new(&correction.checkout).exists());
        let kept = crate::source_control::git::git(
            &s.w.repo,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/orbyn/corrections/{}", &correction.task_id[7..]),
            ],
        )
        .await;
        assert!(kept.is_err(), "the correction reference is removed");
        assert_eq!(gitq(&s.w.repo, &["rev-parse", "HEAD"]).await, checkout);
    })
    .await;
}

#[tokio::test]
async fn a_core_correction_refuses_a_dirty_or_retargeted_review_checkout() {
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
        s.w.core
            .crew_answer_review(&s.task, ReviewAnswer::Revert, &s.target, NOW + 1)
            .await
            .unwrap();
        let correction =
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .unwrap();
        let owner = s.w.core.store.thread(&s.w.lead).unwrap();
        std::fs::write(
            std::path::Path::new(&correction.checkout).join("src/foreign.txt"),
            "new work",
        )
        .unwrap();
        assert!(
            s.w.core
                .correction_review_thread(&owner, &correction.task_id)
                .await
                .is_err()
        );
        std::fs::remove_file(std::path::Path::new(&correction.checkout).join("src/foreign.txt"))
            .unwrap();
        run_git(
            std::path::Path::new(&correction.checkout),
            &["checkout", "--detach", "-q", &correction.base],
        );
        assert!(
            s.w.core
                .correction_review_thread(&owner, &correction.task_id)
                .await
                .is_err()
        );
        assert_eq!(s.w.core.store.thread(&s.w.lead).unwrap().cwd, owner.cwd);
    })
    .await;
}

#[tokio::test]
async fn a_core_correction_resumes_the_same_binding_after_interrupted_task_admission() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
        s.w.core
            .crew_answer_review(&s.task, ReviewAnswer::Revert, &s.target, NOW + 1)
            .await
            .unwrap();
        let correction =
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .unwrap();
        // Model the interruption after immutable binding and checkout but before task admission.
        s.w.core
            .store
            .reader()
            .execute("DELETE FROM crew_tasks WHERE id=?1", [&correction.task_id])
            .unwrap();
        s.w.core
            .crew_answer_review(&s.task, ReviewAnswer::Revert, &s.target, NOW + 2)
            .await
            .unwrap();
        let recovered =
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .unwrap();
        assert_eq!(recovered, correction);
        assert_eq!(
            s.w.core
                .store
                .crew_task(&correction.task_id)
                .unwrap()
                .unwrap()
                .state,
            "passed"
        );
        assert_eq!(
            state.lock().unwrap().creates,
            1,
            "task recovery itself publishes nothing"
        );
        s.w.core
            .store
            .reader()
            .execute(
                "DELETE FROM crew_merge_queue WHERE task_id=?1",
                [&correction.task_id],
            )
            .unwrap();
        assert!(
            s.w.core
                .store
                .crew_forge_merge(&correction.task_id)
                .unwrap()
                .is_some()
        );
        assert!(
            s.w.core
                .store
                .crew_task(&correction.task_id)
                .unwrap()
                .is_some()
        );
        s.w.core
            .crew_answer_review(&s.task, ReviewAnswer::Revert, &s.target, NOW + 3)
            .await
            .unwrap();
        assert_eq!(
            s.w.core
                .store
                .merge_entry(&correction.task_id)
                .unwrap()
                .unwrap()
                .candidate,
            correction.candidate
        );
        assert_eq!(state.lock().unwrap().creates, 1);
    })
    .await;
}

/// A previously accepted forge merge whose trusted check now reproduces a regression on the forge's main.
/// The watch is tested independently of the merge admission that normally rejects it.
async fn seed_broken_forge_merge(
    s: &Setup,
    state: &Arc<std::sync::Mutex<super::forge_merge::RemoteState>>,
) {
    // Seed a previously accepted forge merge whose trusted check now reproduces a regression.
    // The watch is being tested independently of the merge admission that normally rejects it.
    let entry =
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
    let mut intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
    let tree = gitq(
        &s.w.repo,
        &["rev-parse", &format!("{}^{{tree}}", entry.candidate)],
    )
    .await;
    let merge = gitq(
        &s.w.repo,
        &[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.invalid",
            "commit-tree",
            &tree,
            "-p",
            &intent.base,
            "-p",
            &entry.candidate,
            "-m",
            "accepted merge fixture",
        ],
    )
    .await;
    run_git(
        s.remote.path(),
        &["fetch", "-q", &s.w.repo.to_string_lossy(), &merge],
    );
    run_git(
        s.remote.path(),
        &[
            "update-ref",
            &format!("refs/heads/{}", s.target.branch),
            &merge,
        ],
    );
    intent.combined = Some(merge.clone());
    intent.pull = Some(7);
    intent.merge_commit = Some(merge.clone());
    intent.evidence =
        s.w.core
            .store
            .crew_evidence(&s.task)
            .unwrap()
            .iter()
            .map(|e| e.id.as_str().to_owned())
            .collect();
    // Observe the normal forward-only store transition sequence.
    for phase in [
        ForgePhase::Publishing,
        ForgePhase::AwaitingCi,
        ForgePhase::MergeRequested,
        ForgePhase::Merged,
    ] {
        intent.phase = phase;
        if phase == ForgePhase::MergeRequested {
            intent.merge_sent = true;
        }
        s.w.core.store.save_crew_forge_merge(&mut intent).unwrap();
    }
    s.w.core
        .store
        .add_crew_merge(&crate::store::MergeRecord {
            id: 0,
            task_id: s.task.clone(),
            crew_id: entry.crew_id.clone(),
            repo: entry.repo.clone(),
            branch: entry.branch.clone(),
            before_commit: intent.base.clone(),
            merge_commit: merge.clone(),
            candidate: entry.candidate.clone(),
            risk: entry.risk.clone(),
            reasons: entry.reasons.clone(),
            areas: entry.areas.clone(),
            engine: entry.engine.clone(),
            mode: entry.mode.clone(),
            files: entry.files,
            lines: entry.lines,
            proof: json!({"fixture":"previously accepted merge"}),
            not_verified: json!([]),
            merged_unix: NOW,
            due_unix: None,
            review_state: "waiting".into(),
            comment: None,
            watch: "unknown".into(),
            reverted_commit: None,
            needs_person: None,
        })
        .unwrap();
    state.lock().unwrap().merged = Some(merge);
}

async fn broken_world() -> Setup {
    let w = super::harness::world_forge_review(Arc::new(|turn, repo| {
        if turn == 0 {
            std::fs::write(repo.join("src/feature.txt"), "regression\n").unwrap();
            commit_all(repo, "regression");
            Turn::Report(report("done"))
        } else {
            super::review_support::reviewed(vec![])
        }
    }))
    .await;
    let good = gitq(&w.repo, &["rev-parse", "HEAD"]).await;
    let rules =
        json!({"checks":[{"run":format!("git diff --exit-code {good} HEAD -- src/feature.txt")}]});
    setup_world(&rules.to_string(), w).await
}

const READY: &str = "A verified corrective PR is ready. Choose Revert to publish it.";

#[tokio::test]
async fn a_certain_forge_watch_prepares_but_only_a_persons_revert_publishes() {
    let s = broken_world().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        seed_broken_forge_merge(&s, &state).await;
        let local = gitq(&s.w.repo, &["rev-parse", "HEAD"]).await;
        let target = MergeTarget {
            repo: s.target.repo.clone(),
            branch: s.target.branch.clone(),
            owed: None,
        };
        // The lead's own tools verify a corrective task but write nothing to the forge.
        let (answer, error) =
            s.w.call(
                &s.w.lead_token,
                "crew_watch_main",
                json!({"task": s.task, "branch": s.target.branch}),
            )
            .await;
        assert!(!error, "{answer}");
        let correction =
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .expect("a certain Crew culprit prepares a correction");
        assert_eq!(
            s.w.core
                .store
                .crew_task(&correction.task_id)
                .unwrap()
                .unwrap()
                .state,
            "passed"
        );
        let nothing_sent = |when: &str| {
            let remote = state.lock().unwrap();
            assert_eq!((remote.creates, remote.merges), (0, 0), "{when}");
            assert!(
                s.w.core
                    .store
                    .merge_entry(&correction.task_id)
                    .unwrap()
                    .is_none(),
                "{when}"
            );
            assert!(
                s.w.core
                    .store
                    .crew_forge_merge(&correction.task_id)
                    .unwrap()
                    .is_none(),
                "{when}"
            );
        };
        nothing_sent("after the lead's watch");
        assert_eq!(
            s.w.core
                .store
                .crew_merge(&s.task)
                .unwrap()
                .unwrap()
                .needs_person
                .as_deref(),
            Some(READY)
        );
        let (answer, error) =
            s.w.call(
                &s.w.lead_token,
                "crew_process_merges",
                json!({"task": s.task, "branch": s.target.branch}),
            )
            .await;
        assert!(!error, "{answer}");
        nothing_sent("after the lead processed the lane");
        s.w.core
            .crew_watch_forge_main(&target, None, NOW + 1)
            .await
            .unwrap();
        nothing_sent("after the local watch");
        // Startup and the periodic tick only read effects that were already requested.
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        nothing_sent("after startup");
        assert_ne!(
            s.w.core
                .store
                .crew_merge(&s.task)
                .unwrap()
                .unwrap()
                .review_state,
            "reverted"
        );
        assert_eq!(gitq(&s.w.repo, &["rev-parse", "HEAD"]).await, local);
        let events = s.w.core.store.crew_autopilot_events(&s.task).unwrap();
        let requested: Vec<_> = events
            .iter()
            .filter(|event| event.kind == "correction_requested")
            .collect();
        assert_eq!(requested.len(), 1, "{events:?}");
        assert_eq!(requested[0].body["actor"], "watch");
        assert_eq!(
            requested[0].body["candidate"],
            correction.candidate.as_str()
        );

        // Only the person's Revert queues it.
        s.w.core
            .crew_answer_review(&s.task, ReviewAnswer::Revert, &target, NOW + 2)
            .await
            .unwrap();
        assert!(
            s.w.core
                .store
                .merge_entry(&correction.task_id)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            state.lock().unwrap().creates,
            0,
            "queueing publishes nothing"
        );
        let (review, error) =
            s.w.lead(
                "crew_review",
                json!({"task":correction.task_id,"lanes":["correctness"]}),
            )
            .await;
        assert!(!error, "{review}");
        s.w.until_state(review["lanes"][0]["task"].as_str().unwrap(), "passed")
            .await;
        s.w.until("a watched correction earns normal review", |w| {
            w.core
                .review_lane_gate(&correction.task_id, "correctness", &correction.candidate)
                .unwrap()
                .is_some_and(|g| g.decision == orbyn_contracts::GateDecision::Pass)
        })
        .await;
        // This synthetic verifier explicitly reads the changed file. Preserve the existing
        // check-input hold: normal review cannot waive it, only a person's release can.
        let held =
            s.w.core
                .store
                .merge_entry(&correction.task_id)
                .unwrap()
                .unwrap();
        assert_eq!(held.state, "waiting");
        assert!(held.note.unwrap().contains("checks runs or reads"));
        s.w.core.crew_release_merge(&correction.task_id).unwrap();
        let correction_intent =
            s.w.core
                .store
                .crew_forge_merge(&correction.task_id)
                .unwrap()
                .unwrap();
        {
            let mut remote = state.lock().unwrap();
            remote.base = correction.base;
            remote.head = correction.candidate;
            remote.source = correction_intent.source;
            remote.merged = None;
            remote.pending = true;
        }
        s.w.core
            .crew_process_forge_merges(&target, NOW + 3)
            .await
            .unwrap();
        assert_eq!(
            state.lock().unwrap().creates,
            1,
            "the person's route publishes the corrective PR"
        );
        assert_ne!(
            s.w.core
                .store
                .crew_merge(&s.task)
                .unwrap()
                .unwrap()
                .review_state,
            "reverted"
        );
    })
    .await;
}

#[tokio::test]
async fn a_lead_in_another_crew_cannot_prepare_or_flag_a_break_it_does_not_own() {
    let s = broken_world().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        seed_broken_forge_merge(&s, &state).await;
        let target = MergeTarget {
            repo: s.target.repo.clone(),
            branch: s.target.branch.clone(),
            owed: None,
        };
        s.w.core
            .crew_watch_forge_main(&target, Some("another-crew"), NOW + 1)
            .await
            .unwrap();
        assert!(
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .is_none()
        );
        assert!(
            s.w.core
                .store
                .crew_merge(&s.task)
                .unwrap()
                .unwrap()
                .needs_person
                .is_none(),
            "another crew's lead cannot flag this merge"
        );
        s.w.core
            .crew_watch_forge_main(&target, Some(&s.w.lead), NOW + 2)
            .await
            .unwrap();
        assert!(
            s.w.core
                .store
                .crew_forge_correction(&s.task)
                .unwrap()
                .is_some()
        );
        assert_eq!(state.lock().unwrap().creates, 0);
    })
    .await;
}

#[tokio::test]
async fn corrective_startup_never_publishes_an_ordinary_prepared_task() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
        let later = passed(&s.w, &["src/**"], "Later ordinary queued task.").await;
        s.w.core
            .set_crew_autopilot_mode(&crate::crew::ModeScope::Default, crate::crew::Mode::Off)
            .unwrap();
        s.w.core
            .crew_queue_forge_merge(&later, &s.target, NOW + 1)
            .await
            .unwrap();
        s.w.core.crew_release_merge(&later).unwrap();
        s.w.core
            .set_crew_autopilot_mode(&crate::crew::ModeScope::Default, crate::crew::Mode::Full)
            .unwrap();
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&later)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::Prepared
        );
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(state.lock().unwrap().creates, 1);
        assert_eq!(state.lock().unwrap().merges, 1);
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&later)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::Prepared
        );
    })
    .await;
}
