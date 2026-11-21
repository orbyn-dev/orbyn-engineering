//! Create a check, part 2: approval, confinement, every way into the merge queue, withdraw and crash recovery, and old worker tasks.
//! The rig is the parent module's.

#![allow(clippy::wildcard_imports)]
use super::*;

// Review round: the proof never runs unrestricted and unreviewed, the merge queue refuses a check that is not proven, and the
// withdraw, crash and path cases.

impl Rig {
    /// Waits until the worker's change has been read and the proof waits for a person (the not-confinable branch).
    async fn until_waiting(&self) -> crate::store::EscapeCheckRecord {
        self.w
            .until("the proof waits for approval", |_| {
                let c = self.check();
                c.state == EscapeCheckState::Writing && c.diff_commit.is_some()
            })
            .await;
        self.check()
    }

    fn proof_folders(&self) -> usize {
        std::fs::read_dir(self.w.core.config.data_dir.join("crew-escape-proof"))
            .map_or(0, Iterator::count)
    }

    /// Every way into the merge queue, for the task of a check: the lead's tool, the Review queue's route (`crew_merge_request`), the
    /// forge entry and the queue entry itself. Each answer is its plain message, or `queued` when it got past the guard.
    async fn queue_attempts(&self, task: &str) -> Vec<String> {
        use crate::crew::{MergeOperation, MergeRequest, MergeTarget};
        let branch = git_out(&self.w.repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
        let target = MergeTarget {
            repo: self.w.repo.clone(),
            branch: branch.clone(),
            owed: None,
        };
        let core = &self.w.core;
        let mut found = Vec::new();
        let (tool, error) = self
            .w
            .call(
                &self.w.lead_token,
                "crew_queue_merge",
                json!({ "task": task }),
            )
            .await;
        found.push(if error {
            tool.to_string()
        } else {
            "queued".into()
        });
        found.push(
            match core
                .crew_merge_request(
                    task,
                    &MergeRequest {
                        operation: MergeOperation::Queue,
                        branch: branch.clone(),
                    },
                )
                .await
            {
                Ok(_) => "queued".into(),
                Err(error) => error.public_message(),
            },
        );
        found.push(
            match core.crew_queue_forge_merge(task, &target, unix_now()).await {
                Ok(_) => "queued".into(),
                Err(error) => error.public_message(),
            },
        );
        found.push(
            match core.crew_queue_merge(task, &target, unix_now()).await {
                Ok(_) => "queued".into(),
                Err(error) => error.public_message(),
            },
        );
        found
    }
}

fn unix_now() -> i64 {
    crate::util::unix_now()
}

#[tokio::test]
async fn an_unrestricted_proof_waits_for_a_persons_approval_of_the_exact_change() {
    let r = rig().await;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    let waiting = r.until_waiting().await;
    let (commit, hash) = (
        waiting.diff_commit.clone().unwrap(),
        waiting.diff_hash.clone().unwrap(),
    );
    assert_eq!(waiting.diff_paths, [PATH]);
    assert_eq!(
        (waiting.proof.clone(), waiting.approved_commit.clone()),
        (None, None)
    );

    // The proof did not run, however many ticks pass, and no proof folder was made.
    r.w.core.crew_escape_check_reconcile().await;
    r.w.core.crew_escape_check_reconcile().await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert_eq!(r.check().proof, None);
    assert_eq!(r.check().state, EscapeCheckState::Writing);
    assert_eq!(r.proof_folders(), 0);
    let row =
        r.w.core
            .crew_escape_detail(&r.escape)
            .unwrap()
            .check
            .unwrap();
    assert!(row.awaiting_approval && !row.approved, "{row:?}");
    assert_eq!(row.diff_paths, [PATH]);
    assert!(
        row.line.contains("approve") && row.line.contains("unrestricted"),
        "{}",
        row.line
    );

    // A different commit or hash is not what the person saw.
    let approve = |commit: String, diff_hash: String| wire::EscapeCheckApprove {
        project: r.project.clone(),
        seen: seen(Some(EscapeCheckState::Writing)),
        commit,
        diff_hash,
    };
    let core = &r.w.core;
    for body in [
        approve("a".repeat(40), hash.clone()),
        approve(commit.clone(), "b".repeat(64)),
    ] {
        let refused = core.crew_escape_check_approve(&r.escape, &body).await;
        assert!(matches!(refused, Err(CoreError::Stale(_))), "{refused:?}");
    }
    let bad = core
        .crew_escape_check_approve(&r.escape, &approve("xyz".into(), hash.clone()))
        .await;
    assert!(matches!(bad, Err(CoreError::Invalid(_))), "{bad:?}");
    assert_eq!(r.check().approved_commit, None);
    assert_eq!(
        r.audits().iter().filter(|a| a.ends_with("approve")).count(),
        0
    );
    // Another project cannot approve it.
    let elsewhere = core.new_project("other", false, None).await.unwrap().id;
    let mut foreign = approve(commit.clone(), hash.clone());
    foreign.project = elsewhere;
    assert!(matches!(
        core.crew_escape_check_approve(&r.escape, &foreign).await,
        Err(CoreError::NotFound(_))
    ));

    // The exact change runs it; the same approval again changes nothing.
    let answer = core
        .crew_escape_check_approve(&r.escape, &approve(commit.clone(), hash.clone()))
        .await
        .unwrap();
    assert!(answer.check.approved);
    assert_eq!(r.audits().last().unwrap(), "crew.escape.check.approve");
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    let audits = r.audits().len();
    core.crew_escape_check_approve(&r.escape, &approve(commit, hash))
        .await
        .unwrap();
    assert_eq!(r.audits().len(), audits);
    assert_eq!(r.w.task(&task).state, "passed");
    assert_eq!(r.proof_folders(), 0, "the proof's trees are removed");
}

/// Where the machine can restrict the proof it runs without an approval, with no network and no reading of the data folder.
/// A Mac that cannot restrict fails here instead of returning, so a green line means the confined run happened.
#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_confined_proof_runs_without_an_approval() {
    assert!(
        crate::crew::escapes::check_proof_confined_for_test(),
        "this Mac cannot confine a proof ({}), so a confined run without an approval was not exercised",
        crate::host::confine::proof_unavailable().unwrap_or_default()
    );
    let r = rig_with(true).await;
    r.propose().await;
    r.accept().await;
    r.w.until("the check ends", |_| {
        matches!(
            r.check().state,
            EscapeCheckState::Proven | EscapeCheckState::Failed
        )
    })
    .await;
    assert_eq!(
        r.check().state,
        EscapeCheckState::Proven,
        "{:?}",
        r.check().note
    );
    assert_eq!(
        r.audits(),
        ["crew.escape.check.propose", "crew.escape.check.accept"],
        "no approval was asked for"
    );
}

#[tokio::test]
async fn the_merge_queue_refuses_a_check_that_is_not_proven_by_every_way_in() {
    let r = rig().await;
    r.propose().await;
    let writing = r.accept().await.check.task_id.unwrap();
    // Writing (and waiting for approval): refused by the lead's tool, the Review queue's route, the forge entry and the queue entry.
    r.until_waiting().await;
    for answer in r.queue_attempts(&writing).await {
        assert!(
            answer.contains("not proven") && answer.contains("still being written"),
            "{answer}"
        );
    }
    // Approved: the proof runs and holds.
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    let proven = writing.clone();
    // Proven: every entry gets past the guard (it is then refused by the next gate: no required CI names are set on this device).
    for answer in r.queue_attempts(&proven).await {
        assert!(!answer.contains("not proven"), "{answer}");
    }
    // Withdrawn after it was proven: refused again, and an entry that got in while it was proven is turned down by the tick.
    r.w.core
        .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proven))
        .await
        .unwrap();
    for answer in r.queue_attempts(&proven).await {
        assert!(answer.contains("withdrawn"), "{answer}");
    }
    r.w.core.crew_escape_check_reconcile().await;
    if let Some(entry) = r.w.core.store.merge_entry(&proven).unwrap() {
        assert_eq!(entry.state, "rejected");
    }
}

#[tokio::test]
async fn a_failed_check_is_not_queueable_and_an_unreadable_one_fails_closed() {
    let r = rig().await;
    r.plan.lock().unwrap().pattern = "value".into();
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    for answer in r.queue_attempts(&task).await {
        assert!(
            answer.contains("failed") && answer.contains("not proven"),
            "{answer}"
        );
    }
    // A check row that cannot be read refuses, with a plain reason, and nothing is queued.
    r.w.core
        .store
        .reader()
        .execute_batch("UPDATE crew_escape_checks SET proof = '{\"x\":1}'")
        .unwrap();
    for answer in r.queue_attempts(&task).await {
        assert!(answer.contains("could not be read"), "{answer}");
    }
    assert!(r.w.core.store.merge_entry(&task).unwrap().is_none());
    // A task that is no check's is not affected.
    assert!(
        r.w.core
            .refuse_unproven_escape_check(&r.merge_task, None)
            .is_ok()
    );
}

#[tokio::test]
async fn a_withdrawn_checks_queued_merge_is_turned_down_by_the_tick() {
    let r = rig().await;
    r.plan.lock().unwrap().hang = true;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    r.w.until("the worker runs", |w| w.task(&task).state == "running")
        .await;
    r.w.core
        .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Writing))
        .await
        .unwrap();
    // A queue entry that got in around the guard (a race, or an older build) is turned down, and so is any entry on a withdrawn check.
    r.w.core
        .store
        .add_merge_entry(&crate::crew::fixtures::merge_entry(&task, "/repo", "x"))
        .unwrap();
    assert_eq!(
        r.w.core.store.merge_entry(&task).unwrap().unwrap().state,
        "waiting"
    );
    r.w.core.crew_escape_check_reconcile().await;
    assert_eq!(
        r.w.core.store.merge_entry(&task).unwrap().unwrap().state,
        "rejected"
    );
    assert_eq!(r.check().state, EscapeCheckState::Withdrawn);
}

#[tokio::test]
async fn a_check_stuck_in_writing_after_a_crash_goes_back_to_a_proposal() {
    let r = rig().await;
    r.propose().await;
    // The process ended after the audit row and `writing`, before the task existed.
    let check = r.check();
    r.w.core
        .store
        .accept_escape_check(
            &r.project,
            &r.escape,
            &seen(Some(EscapeCheckState::Proposed)),
            &crate::store::CheckProposal {
                path: &check.path,
                asserts: &check.asserts,
                run: &check.run,
                fix_commit: &check.fix_commit,
            },
            "sha256:00000000000000000000000000000000000000000000000000000000000000bb",
            unix_now(),
        )
        .unwrap();
    assert_eq!(r.check().state, EscapeCheckState::Writing);
    r.w.core.crew_escape_check_reconcile().await;
    let back = r.check();
    assert_eq!(back.state, EscapeCheckState::Proposed);
    assert_eq!(back.task_id, None);
    assert!(back.note.unwrap().contains("Accept it again"));
    assert_eq!(r.audits().last().unwrap(), "crew.escape.check.start_failed");
    // And accepting again starts a worker.
    assert_eq!(r.accept().await.check.state, EscapeCheckState::Writing);
}

#[tokio::test]
async fn withdraw_succeeds_for_a_writing_check_whose_task_is_missing_or_whose_proof_waits() {
    // The task never existed.
    let r = rig().await;
    r.propose().await;
    let check = r.check();
    r.w.core
        .store
        .accept_escape_check(
            &r.project,
            &r.escape,
            &seen(Some(EscapeCheckState::Proposed)),
            &crate::store::CheckProposal {
                path: &check.path,
                asserts: &check.asserts,
                run: &check.run,
                fix_commit: &check.fix_commit,
            },
            "sha256:00000000000000000000000000000000000000000000000000000000000000cc",
            unix_now(),
        )
        .unwrap();
    let answer =
        r.w.core
            .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Writing))
            .await
            .unwrap();
    assert_eq!(answer.check.state, EscapeCheckState::Withdrawn);

    // The task passed and the proof waits for approval: withdrawing stops it, and an approval after that is refused.
    let r = rig().await;
    r.propose().await;
    r.accept().await;
    let waiting = r.until_waiting().await;
    let answer =
        r.w.core
            .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Writing))
            .await
            .unwrap();
    assert_eq!(answer.check.state, EscapeCheckState::Withdrawn);
    let late =
        r.w.core
            .crew_escape_check_approve(
                &r.escape,
                &wire::EscapeCheckApprove {
                    project: r.project.clone(),
                    seen: seen(Some(EscapeCheckState::Withdrawn)),
                    commit: waiting.diff_commit.unwrap(),
                    diff_hash: waiting.diff_hash.unwrap(),
                },
            )
            .await;
    assert!(matches!(late, Err(CoreError::Invalid(_))), "{late:?}");
    r.w.core.crew_escape_check_reconcile().await;
    assert_eq!(r.check().state, EscapeCheckState::Withdrawn);
    assert_eq!(r.proof_folders(), 0);
}

#[tokio::test]
async fn an_accept_of_a_withdrawn_check_starts_no_worker() {
    let r = rig().await;
    r.propose().await;
    r.w.core
        .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proposed))
        .await
        .unwrap();
    let tasks = r.tasks();
    let late =
        r.w.core
            .crew_escape_check_accept(&r.escape, &r.act(EscapeCheckState::Withdrawn))
            .await;
    assert!(matches!(late, Err(CoreError::Invalid(_))), "{late:?}");
    assert_eq!(r.tasks(), tasks);
    // An accept whose proposal changed under it (it was withdrawn and proposed again) is stale, not a second worker.
    let again = wire::EscapeCheckPropose {
        path: "tests/other.txt".into(),
        seen: seen(Some(EscapeCheckState::Withdrawn)),
        ..r.propose_body()
    };
    r.w.core
        .crew_escape_check_propose(&r.escape, &again)
        .await
        .unwrap();
    assert_eq!(r.check().path, "tests/other.txt");
}

// Re-review round: an old worker task never becomes mergeable, and a withdraw never races a landing merge.

#[tokio::test]
async fn an_old_worker_task_is_never_queueable_after_a_retry_or_a_new_proposal() {
    let r = rig().await;
    r.plan.lock().unwrap().pattern = "value".into();
    r.propose().await;
    let old = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    // Retry clears the check's task; the old task still has no way in.
    r.w.core
        .crew_escape_check_retry(&r.escape, &r.act(EscapeCheckState::Failed))
        .unwrap();
    assert_eq!(r.check().task_id, None);
    for answer in r.queue_attempts(&old).await {
        assert!(answer.contains("earlier attempt"), "{answer}");
    }
    // Withdraw and propose again: the row is replaced, the old task is still refused.
    r.w.core
        .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proposed))
        .await
        .unwrap();
    let again = wire::EscapeCheckPropose {
        seen: seen(Some(EscapeCheckState::Withdrawn)),
        ..r.propose_body()
    };
    r.w.core
        .crew_escape_check_propose(&r.escape, &again)
        .await
        .unwrap();
    for answer in r.queue_attempts(&old).await {
        assert!(answer.contains("earlier attempt"), "{answer}");
    }
    // A second worker proves the check; only that task is accepted, the first stays refused.
    r.plan.lock().unwrap().pattern = "fixed".into();
    r.reset();
    let current = r.accept().await.check.task_id.unwrap();
    assert_ne!(current, old);
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    for answer in r.queue_attempts(&old).await {
        assert!(answer.contains("earlier attempt"), "{answer}");
    }
    for answer in r.queue_attempts(&current).await {
        assert!(
            !answer.contains("not proven") && !answer.contains("earlier attempt"),
            "{answer}"
        );
    }
    // A queue entry of the old task (a race, or an older build) is turned down by the tick.
    r.w.core
        .store
        .add_merge_entry(&crate::crew::fixtures::merge_entry(&old, "/repo", "x"))
        .unwrap();
    r.w.core.crew_escape_check_reconcile().await;
    assert_eq!(
        r.w.core.store.merge_entry(&old).unwrap().unwrap().state,
        "rejected"
    );
}

#[tokio::test]
async fn a_recorded_task_fails_closed_when_the_record_cannot_be_read() {
    let r = rig().await;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    r.w.core
        .store
        .reader()
        .execute_batch("ALTER TABLE crew_escape_check_tasks RENAME TO crew_escape_check_tasks_gone")
        .unwrap();
    for answer in r.queue_attempts(&task).await {
        assert!(answer.contains("could not be read"), "{answer}");
    }
}

#[test]
fn the_guard_judges_the_task_against_the_escapes_current_check() {
    use crate::crew::escapes::escape_check_blocks_queue as blocks;
    let mut check = crate::store::EscapeCheckRecord {
        escape_id: "e".into(),
        project_id: "p".into(),
        path: PATH.into(),
        asserts: "a".into(),
        run: "r".into(),
        fix_commit: "f".repeat(40),
        task_id: Some("t2".into()),
        state: EscapeCheckState::Proven,
        proof: None,
        note: None,
        attempts: 2,
        diff_commit: None,
        diff_hash: None,
        diff_paths: vec![],
        approved_commit: None,
        approved_hash: None,
        proposed_unix: 0,
        updated_unix: 0,
    };
    // Not a check's task at all.
    assert_eq!(blocks("t9", false, None, None), None);
    // Recorded, but its check is gone, names another task, or has none.
    assert!(
        blocks("t1", true, None, None)
            .unwrap()
            .contains("no longer exists")
    );
    assert!(
        blocks("t1", true, Some(&check), None)
            .unwrap()
            .contains("earlier attempt")
    );
    check.task_id = None;
    assert!(
        blocks("t1", true, Some(&check), None)
            .unwrap()
            .contains("earlier attempt")
    );
    // The current task: only while proven.
    check.task_id = Some("t2".into());
    assert_eq!(blocks("t2", true, Some(&check), None), None);
    // The change queued must be the one that was proven.
    check.diff_commit = Some("a".repeat(40));
    assert_eq!(
        blocks("t2", true, Some(&check), Some(&"a".repeat(40))),
        None
    );
    assert!(
        blocks("t2", true, Some(&check), Some(&"b".repeat(40)))
            .unwrap()
            .contains("different commit")
    );
    check.diff_commit = None;
    assert!(
        blocks("t2", true, Some(&check), Some(&"a".repeat(40)))
            .unwrap()
            .contains("different commit")
    );
    for state in [
        EscapeCheckState::Proposed,
        EscapeCheckState::Writing,
        EscapeCheckState::Failed,
        EscapeCheckState::Withdrawn,
        EscapeCheckState::Merged,
    ] {
        check.state = state;
        assert!(
            blocks("t2", true, Some(&check), None)
                .unwrap()
                .contains("not proven"),
            "{state:?}"
        );
    }
}

#[tokio::test]
async fn processing_blocks_a_task_whose_check_stopped_being_proven_after_it_was_queued() {
    let r = rig().await;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    r.w.core
        .store
        .add_merge_entry(&crate::crew::fixtures::merge_entry(
            &task,
            &r.w.repo.to_string_lossy(),
            "x",
        ))
        .unwrap();
    r.w.core
        .store
        .set_merge_entry_state(&task, &crate::store::OPEN_MERGE_STATES, "queued", None)
        .unwrap()
        .expect_moved();
    // The check is withdrawn under the queue's feet (here straight in the store, so nothing turns the entry down first).
    r.w.core
        .store
        .withdraw_escape_check(
            &r.project,
            &r.escape,
            &seen(Some(EscapeCheckState::Proven)),
            unix_now(),
        )
        .unwrap();
    let branch = git_out(&r.w.repo, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let target = crate::crew::MergeTarget {
        repo: r.w.repo.clone(),
        branch,
        owed: None,
    };
    let outcomes =
        r.w.core
            .crew_process_merges(&target, unix_now())
            .await
            .unwrap();
    assert!(
        outcomes.iter().any(|o| matches!(o, crate::crew::MergeOutcome::Blocked { why, .. } if why.contains("withdrawn"))),
        "{outcomes:?}"
    );
    assert_eq!(
        r.w.core.store.merge_entry(&task).unwrap().unwrap().state,
        "blocked"
    );
}

#[tokio::test]
async fn a_withdraw_is_refused_while_the_merge_is_landing_and_never_errors_after_it_committed() {
    let r = rig().await;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    r.w.core
        .store
        .add_merge_entry(&crate::crew::fixtures::merge_entry(&task, "/repo", "x"))
        .unwrap();
    // The entry is `merging`: refused in words, nothing changed.
    r.w.core
        .store
        .set_merge_entry_state(&task, &crate::store::OPEN_MERGE_STATES, "merging", None)
        .unwrap()
        .expect_moved();
    let refused =
        r.w.core
            .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proven))
            .await;
    let Err(CoreError::Invalid(why)) = refused else {
        panic!("{refused:?}")
    };
    assert!(why.contains("landing"), "{why}");
    assert_eq!(r.check().state, EscapeCheckState::Proven);
    assert!(!r.audits().iter().any(|a| a.ends_with("withdraw")));
    // A forge intent already requested is just as far along.
    r.w.core
        .store
        .set_merge_entry_state(&task, &crate::store::OPEN_MERGE_STATES, "queued", None)
        .unwrap()
        .expect_moved();
    // An entry that is already out of the queue: the withdraw succeeds and says so.
    r.w.core
        .store
        .set_merge_entry_state(
            &task,
            &crate::store::OPEN_MERGE_STATES,
            "rejected",
            Some("x"),
        )
        .unwrap()
        .expect_moved();
    let done =
        r.w.core
            .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proven))
            .await
            .unwrap();
    assert_eq!(done.check.state, EscapeCheckState::Withdrawn);
    assert!(
        done.check
            .note
            .unwrap_or_default()
            .contains("nothing to stop"),
        "the answer says the entry could not be stopped"
    );
}

/// Issue 703: a crash left the entry `merging`. While a merge is live in this process the withdraw stays refused (the lane's lock is
/// held); once recovery has settled the stale entry the withdraw goes through.
#[tokio::test]
async fn a_withdraw_works_once_a_crash_left_entry_is_recovered_and_stays_refused_while_a_merge_is_live()
 {
    let r = rig().await;
    r.propose().await;
    let task = r.accept().await.check.task_id.unwrap();
    assert_eq!(r.finish().await, EscapeCheckState::Proven);
    r.w.core
        .store
        .add_merge_entry(&crate::crew::fixtures::merge_entry(
            &task,
            "/no/such/repo",
            "x",
        ))
        .unwrap();
    r.w.core
        .store
        .set_merge_entry_state(&task, &crate::store::OPEN_MERGE_STATES, "merging", None)
        .unwrap()
        .expect_moved();
    {
        let _live = r.w.core.crew_merges.lock().await;
        let waiting = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            r.w.core.recover_crew_merging(unix_now()),
        )
        .await;
        assert!(waiting.is_err(), "recovery waits behind a live merge");
        let refused =
            r.w.core
                .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proven))
                .await;
        let Err(CoreError::Invalid(why)) = refused else {
            panic!("{refused:?}")
        };
        assert!(why.contains("landing"), "{why}");
    }
    // The repository cannot be read, so git cannot say: the entry waits for a person and no longer holds the withdraw.
    assert_eq!(r.w.core.recover_crew_merging(unix_now()).await.unwrap(), 1);
    assert_eq!(
        r.w.core.store.merge_entry(&task).unwrap().unwrap().state,
        "waiting"
    );
    let done =
        r.w.core
            .crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proven))
            .await
            .unwrap();
    assert_eq!(done.check.state, EscapeCheckState::Withdrawn);
}

/// Every act of the check's life, each clicked twice, with no pause between them (so the briefs of two attempts share a clock second):
/// no error reaches the person, one worker starts per accept, `attempts` counts accepts and the trail is complete and in order.
#[tokio::test]
async fn a_check_that_is_proposed_accepted_failed_and_retried_in_one_tick_with_double_clicks_never_errors()
 {
    use EscapeEventKind::*;
    let r = rig().await;
    r.plan.lock().unwrap().pattern = "value".into();
    let core = &r.w.core;
    let tasks = r.tasks();

    // Propose twice; withdraw twice; propose again twice.
    r.propose().await;
    r.propose().await;
    core.crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proposed))
        .await
        .unwrap();
    core.crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proposed))
        .await
        .unwrap();
    let again = wire::EscapeCheckPropose {
        seen: seen(Some(EscapeCheckState::Withdrawn)),
        ..r.propose_body()
    };
    for _ in 0..2 {
        core.crew_escape_check_propose(&r.escape, &again)
            .await
            .unwrap();
    }
    assert_eq!(r.tasks(), tasks, "a proposal starts nothing");

    // Accept twice: one worker.
    let first = r.accept().await.check.task_id.unwrap();
    let twice = r.accept().await.check;
    assert_eq!(twice.task_id.as_deref(), Some(first.as_str()));
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    assert_eq!(r.tasks(), tasks + 1);
    assert_eq!(r.check().attempts, 1);

    // Retry twice, and a new worker for the next accept, which is its own task even within the same second.
    r.reset();
    for _ in 0..2 {
        core.crew_escape_check_retry(&r.escape, &r.act(EscapeCheckState::Failed))
            .unwrap();
    }
    let second = r.accept().await.check.task_id.unwrap();
    assert_ne!(first, second);
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    assert_eq!((r.tasks(), r.check().attempts), (tasks + 2, 2));

    // And a third, after a withdraw and a new proposal that restart `attempts`.
    r.reset();
    core.crew_escape_check_retry(&r.escape, &r.act(EscapeCheckState::Failed))
        .unwrap();
    core.crew_escape_check_withdraw(&r.escape, &r.act(EscapeCheckState::Proposed))
        .await
        .unwrap();
    core.crew_escape_check_propose(&r.escape, &again)
        .await
        .unwrap();
    let third = r.accept().await.check.task_id.unwrap();
    assert!(third != first && third != second);
    assert_eq!(r.finish().await, EscapeCheckState::Failed);
    assert_eq!((r.tasks(), r.check().attempts), (tasks + 3, 1));

    assert_eq!(
        r.kinds(),
        [
            Detected,
            Confirmed,
            CheckProposed,
            CheckWithdrawn,
            CheckProposed,
            CheckFailed,
            CheckProposed,
            CheckFailed,
            CheckProposed,
            CheckWithdrawn,
            CheckProposed,
            CheckFailed
        ]
    );
}
