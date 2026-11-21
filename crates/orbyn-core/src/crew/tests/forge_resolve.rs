//! A forge merge in `needs_person`: the lane goes on past it, a person's answer (or a merge made on the forge) is the only way out,
//! and a merge request that may still land is never lost. Same fake forge as `forge_merge`.

use std::sync::{Arc, Mutex};

use super::autopilot_merge::passed;
use super::forge_merge::{NOW, RemoteState, Setup, WITHDRAW_AT, fake, make_proven_check, setup};
use crate::crew::MergeOutcome;
use crate::error::CoreError;
use crate::source_control::forge::with_crew_fixture;
use crate::store::ForgePhase;
use crate::testing::run_git;

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
        .set_merge_entry_state(
            &head,
            &crate::store::OPEN_MERGE_STATES,
            "blocked",
            Some("the target base moved"),
        )
        .unwrap()
        .expect_moved();
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
        let queue = s
            .w
            .core
            .autopilot_merge_queue(crate::crew::Page::new(None, None).unwrap())
            .await
            .unwrap();
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
                "test",
            )
            .unwrap();
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_none());
        assert_eq!(s.w.core.store.crew_forge_unfinished().unwrap().len(), 1);
        s.w.core
            .run_crew_merge_watch(tokio_util::sync::CancellationToken::new())
            .await
            .unwrap();
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_some());
        assert!(
            s.w.core
                .store
                .crew_forge_rejected_pending()
                .unwrap()
                .is_empty()
        );
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

/// An intent whose merge request was accepted but has not landed, then sent to `needs_person` because the PR looked changed.
async fn requested_then_stuck(s: &Setup, state: &Arc<Mutex<RemoteState>>) -> i64 {
    state.lock().unwrap().queued_merge = true;
    s.w.core
        .crew_queue_forge_merge(&s.task, &s.target, NOW)
        .await
        .unwrap();
    let first =
        s.w.core
            .crew_process_forge_merges(&s.target, NOW)
            .await
            .unwrap();
    assert!(
        matches!(first.as_slice(), [MergeOutcome::Blocked { why, .. }] if why.contains("merge requested")),
        "{first:?}"
    );
    let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
    assert_eq!(
        (intent.phase, intent.merge_sent),
        (ForgePhase::MergeRequested, true)
    );
    let real = state.lock().unwrap().head.clone();
    state.lock().unwrap().head = "d".repeat(40);
    s.w.core
        .crew_process_forge_merges(&s.target, NOW + 1)
        .await
        .unwrap();
    state.lock().unwrap().head = real;
    let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
    assert_eq!(
        (intent.phase, intent.merge_sent),
        (ForgePhase::NeedsPerson, true)
    );
    intent.revision
}

#[tokio::test]
async fn an_intent_with_a_merge_request_cannot_be_retried_and_a_reject_still_records_a_late_merge()
{
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let revision = requested_then_stuck(&s, &state).await;
        let retry =
            s.w.core
                .crew_forge_resolve(
                    &s.task,
                    &answer(orbyn_contracts::CrewForgeAction::Retry, revision),
                )
                .await;
        assert!(
            matches!(retry, Err(CoreError::Invalid(why)) if why.contains("cannot be sent again"))
        );
        assert_eq!(audit_rows(&s, &s.task), 0);
        let done =
            s.w.core
                .crew_forge_resolve(
                    &s.task,
                    &answer(orbyn_contracts::CrewForgeAction::Reject, revision),
                )
                .await
                .unwrap();
        assert_eq!(done.outcome, "rejected");
        let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
        assert_eq!(
            (intent.phase, intent.merge_sent),
            (ForgePhase::Rejected, true)
        );
        assert!(intent.note.unwrap().contains("left as it is"));
        assert_eq!(
            s.w.core.store.crew_forge_rejected_pending().unwrap().len(),
            1,
            "still in the reconcile set"
        );
        // The request is still pending: nothing is recorded and nothing is sent again.
        let merges = counts(&state).1;
        s.w.core
            .crew_process_forge_merges(&s.target, NOW + 2)
            .await
            .unwrap();
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_none());
        assert_eq!(counts(&state).1, merges);
        // The forge's queue lands it later: the merge is recorded and the entry is merged.
        merge_by_hand(&s, &state);
        let outcomes =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 3)
                .await
                .unwrap();
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::Merged { .. }]),
            "{outcomes:?}"
        );
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_some());
        assert_eq!(
            s.w.core.store.merge_entry(&s.task).unwrap().unwrap().state,
            "merged"
        );
        assert!(
            s.w.core
                .store
                .crew_forge_rejected_pending()
                .unwrap()
                .is_empty()
        );
    })
    .await;
}

#[tokio::test]
async fn a_reject_reads_the_forge_for_a_publication_with_no_pr_number() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        state.lock().unwrap().unknown_create = true;
        assert!(
            s.w.core
                .crew_process_forge_merges(&s.target, NOW)
                .await
                .is_err()
        );
        // The create answered nothing and the forge shows no PR: the intent waits for a person with no PR number.
        state.lock().unwrap().published = false;
        s.w.core
            .crew_process_forge_merges(&s.target, NOW + 1)
            .await
            .unwrap();
        let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
        assert_eq!((intent.phase, intent.pull), (ForgePhase::NeedsPerson, None));
        // A retry cannot tell whether it would publish twice.
        let retry =
            s.w.core
                .crew_forge_resolve(
                    &s.task,
                    &answer(orbyn_contracts::CrewForgeAction::Retry, intent.revision),
                )
                .await;
        assert!(matches!(retry, Err(CoreError::Invalid(_))));
        // The PR shows up and was merged on the forge: a reject records that merge instead.
        {
            let mut remote = state.lock().unwrap();
            remote.published = true;
        }
        merge_by_hand(&s, &state);
        let done =
            s.w.core
                .crew_forge_resolve(
                    &s.task,
                    &answer(orbyn_contracts::CrewForgeAction::Reject, intent.revision),
                )
                .await
                .unwrap();
        assert_eq!(
            (done.outcome.as_str(), done.phase.as_str()),
            ("merged", "merged")
        );
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .pull,
            Some(7)
        );
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_some());
    })
    .await;
}

#[tokio::test]
async fn a_reject_with_no_pr_found_says_the_branch_may_have_been_pushed() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        s.w.core
            .crew_queue_forge_merge(&s.task, &s.target, NOW)
            .await
            .unwrap();
        state.lock().unwrap().unknown_create = true;
        assert!(
            s.w.core
                .crew_process_forge_merges(&s.target, NOW)
                .await
                .is_err()
        );
        state.lock().unwrap().published = false;
        s.w.core
            .crew_process_forge_merges(&s.target, NOW + 1)
            .await
            .unwrap();
        let revision =
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .revision;
        s.w.core
            .crew_forge_resolve(
                &s.task,
                &answer(orbyn_contracts::CrewForgeAction::Reject, revision),
            )
            .await
            .unwrap();
        let note =
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .note
                .unwrap();
        assert!(note.contains("may have been pushed"), "{note}");
        assert!(
            s.w.core
                .store
                .crew_forge_rejected_pending()
                .unwrap()
                .is_empty()
        );
    })
    .await;
}

#[tokio::test]
async fn a_scoped_lead_does_not_read_or_settle_another_crews_waiting_entry() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        stuck_with_pr(&s, &state).await;
        merge_by_hand(&s, &state);
        let reads = state.lock().unwrap().pull_reads;
        let outcomes =
            s.w.core
                .process_forge_merges_for(&s.target, Some("another-crew"), NOW + 2)
                .await
                .unwrap();
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::NeedsPerson { .. }]),
            "{outcomes:?}"
        );
        assert_eq!(
            state.lock().unwrap().pull_reads,
            reads,
            "no forge read for another crew's entry"
        );
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&s.task)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::NeedsPerson
        );
        // Its own lane records it, and the audit row says the lane did.
        let own =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 3)
                .await
                .unwrap();
        assert!(matches!(own.as_slice(), [MergeOutcome::Merged { .. }]));
        let via: Vec<String> =
            s.w.core
                .store
                .list_audit(None, 500)
                .unwrap()
                .iter()
                .filter(|r| r.action == "crew.merge.forge.resolve")
                .map(|r| r.detail["via"].as_str().unwrap_or_default().to_owned())
                .collect();
        assert_eq!(via, ["lane reconcile"]);
    })
    .await;
}

#[tokio::test]
async fn a_retry_after_the_base_moved_is_refused_up_front() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let (head, revision) = stuck_head(&s).await;
        state.lock().unwrap().base = "c".repeat(40);
        let retry =
            s.w.core
                .crew_forge_resolve(
                    &head,
                    &answer(orbyn_contracts::CrewForgeAction::Retry, revision),
                )
                .await;
        assert!(
            matches!(&retry, Err(CoreError::Invalid(why)) if why.contains("base moved")),
            "{retry:?}"
        );
        assert_eq!(audit_rows(&s, &head), 0);
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&head)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::NeedsPerson
        );
    })
    .await;
}

#[tokio::test]
async fn the_queues_own_reject_of_a_waiting_forge_entry_ends_it_through_the_audited_resolve() {
    let s = setup().await;
    let (_state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let (head, _) = stuck_head(&s).await;
        s.w.core
            .crew_reject_merge(&head, "No longer wanted.")
            .unwrap();
        assert_eq!(audit_rows(&s, &head), 1);
        assert_eq!(
            s.w.core
                .store
                .crew_forge_merge(&head)
                .unwrap()
                .unwrap()
                .phase,
            ForgePhase::Rejected
        );
        assert_eq!(
            s.w.core.store.merge_entry(&head).unwrap().unwrap().state,
            "rejected"
        );
    })
    .await;
}

#[tokio::test]
async fn a_queue_reject_of_an_intent_with_a_merge_request_still_records_a_late_merge() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        requested_then_stuck(&s, &state).await;
        // What the escape-check withdraw does to a queued merge of the check.
        s.w.core
            .crew_reject_merge(&s.task, "The check is no longer proven for this task.")
            .unwrap();
        let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
        assert_eq!(
            (intent.phase, intent.merge_sent),
            (ForgePhase::Rejected, true)
        );
        assert_eq!(
            s.w.core.store.merge_entry(&s.task).unwrap().unwrap().state,
            "rejected"
        );
        assert_eq!(
            s.w.core.store.crew_forge_rejected_pending().unwrap().len(),
            1
        );
        merge_by_hand(&s, &state);
        let outcomes =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 3)
                .await
                .unwrap();
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::Merged { .. }]),
            "{outcomes:?}"
        );
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_some());
    })
    .await;
}

fn settled_rows(s: &Setup) -> usize {
    s.w.core
        .store
        .list_audit(None, 500)
        .unwrap()
        .iter()
        .filter(|r| r.action == "crew.merge.forge.resolve" && r.detail["settled"] == true)
        .count()
}

#[tokio::test]
async fn a_rejected_request_whose_pr_closed_unmerged_or_changed_leaves_the_reconcile_set() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let revision = requested_then_stuck(&s, &state).await;
        s.w.core
            .crew_forge_resolve(
                &s.task,
                &answer(orbyn_contracts::CrewForgeAction::Reject, revision),
            )
            .await
            .unwrap();
        // Still open: it keeps waiting, pass after pass.
        s.w.core
            .crew_process_forge_merges(&s.target, NOW + 2)
            .await
            .unwrap();
        assert_eq!(
            s.w.core.store.crew_forge_rejected_pending().unwrap().len(),
            1
        );
        assert_eq!(settled_rows(&s), 0);
        // Closed without a merge: settled, no merge recorded, and the flag is cleared.
        state.lock().unwrap().closed = true;
        let outcomes =
            s.w.core
                .crew_process_forge_merges(&s.target, NOW + 3)
                .await
                .unwrap();
        assert!(outcomes.is_empty(), "{outcomes:?}");
        let intent = s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap();
        assert_eq!(
            (intent.phase, intent.merge_sent),
            (ForgePhase::Rejected, false)
        );
        assert!(
            s.w.core
                .store
                .crew_forge_rejected_pending()
                .unwrap()
                .is_empty()
        );
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_none());
        assert_eq!(settled_rows(&s), 1);
    })
    .await;
    // A PR that is no longer the pinned one cannot be merged by the request, so it settles too.
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    with_crew_fixture(fixture, async {
        let revision = requested_then_stuck(&s, &state).await;
        s.w.core
            .crew_forge_resolve(
                &s.task,
                &answer(orbyn_contracts::CrewForgeAction::Reject, revision),
            )
            .await
            .unwrap();
        state.lock().unwrap().head = "d".repeat(40);
        s.w.core
            .crew_process_forge_merges(&s.target, NOW + 2)
            .await
            .unwrap();
        assert!(
            s.w.core
                .store
                .crew_forge_rejected_pending()
                .unwrap()
                .is_empty()
        );
        assert_eq!(settled_rows(&s), 1);
    })
    .await;
}

#[tokio::test]
async fn a_lane_error_after_the_merge_was_saved_reports_the_merge_not_needs_person() {
    let s = setup().await;
    let (state, fixture) = fake(&s).await;
    let head = state.lock().unwrap().head.clone();
    make_proven_check(&s, &head);
    with_crew_fixture(fixture, async {
        stuck_with_pr(&s, &state).await;
        // The forge says merged with a commit this checkout cannot fetch: recording the merge fails after it is saved.
        state.lock().unwrap().merged = Some("e".repeat(40));
        let outcomes = s.w.core.crew_process_forge_merges(&s.target, NOW + 2).await.unwrap();
        assert!(
            matches!(outcomes.as_slice(), [MergeOutcome::Blocked { why, .. }] if why.contains("recording the merge failed")),
            "{outcomes:?}"
        );
        assert_eq!(s.w.core.store.crew_forge_merge(&s.task).unwrap().unwrap().phase, ForgePhase::Merged);
        assert!(s.w.core.store.crew_merge(&s.task).unwrap().is_none());
        assert_eq!(s.w.core.store.crew_forge_unfinished().unwrap().len(), 1, "recovery still has it");
    })
    .await;
}
