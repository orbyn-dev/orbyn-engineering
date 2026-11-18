//! A crash while a local queue entry is `merging` (issue 703): recovery tells from git whether the merge landed, never touches a live
//! merge or a forge entry, and the entry's state is only ever moved from the state its writer expected.

use super::autopilot_merge::{Merging, gitq, merging, passed};
use super::harness::World;
use crate::crew::fixtures::{forge_needs_person, git as fgit};
use crate::crew::{MergeOutcome, Mode};
use crate::store::{ForgePhase, MergeStateMove, OPEN_MERGE_STATES};
use crate::testing::{TempDir, run_git};

const NOW: i64 = 1_800_000_000;

/// A queued low-risk task on a branch no one has checked out.
async fn queued() -> (Merging, String) {
    let m = merging(Mode::Full, |_| vec![("docs/note.md", "A note.\n")]).await;
    let id = passed(&m.w, &["docs/**"], "Add a note.").await;
    let entry =
        m.w.core
            .crew_queue_merge(&id, &m.target, NOW)
            .await
            .unwrap();
    assert_eq!(entry.state, "queued", "{entry:?}");
    run_git(&m.w.repo, &["checkout", "-q", "-b", "parking"]);
    (m, id)
}

fn crash(w: &World, id: &str) {
    w.core
        .store
        .set_merge_entry_state(id, &["queued"], "merging", None)
        .unwrap()
        .expect_moved();
}

/// What a crash after the branch moved leaves: the queue's own merge commit on the branch, and the entry still `merging`.
async fn land_by_hand(m: &Merging, id: &str) -> (String, String) {
    let candidate = m.w.core.store.merge_entry(id).unwrap().unwrap().candidate;
    let before = m.tip().await;
    let dir = TempDir::new();
    let tree = dir.path().join("scratch");
    run_git(
        &m.w.repo,
        &[
            "worktree",
            "add",
            "--detach",
            &tree.to_string_lossy(),
            &before,
        ],
    );
    let subject = crate::Core::merge_subject(id);
    fgit(&tree, &["merge", "--no-ff", "-m", &subject, &candidate]);
    let merge = fgit(&tree, &["rev-parse", "HEAD"]);
    run_git(
        &m.w.repo,
        &["worktree", "remove", "--force", &tree.to_string_lossy()],
    );
    run_git(
        &m.w.repo,
        &[
            "update-ref",
            &format!("refs/heads/{}", m.main),
            &merge,
            &before,
        ],
    );
    (before, merge)
}

#[tokio::test]
async fn a_crash_after_the_branch_moved_is_recorded_as_merged_and_flagged_for_a_person() {
    let (m, id) = queued().await;
    crash(&m.w, &id);
    let (before, merge) = land_by_hand(&m, &id).await;
    assert_eq!(m.w.core.recover_crew_merging(NOW + 5).await.unwrap(), 1);
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "merged"
    );
    let record = m.w.core.store.crew_merge(&id).unwrap().unwrap();
    assert_eq!((record.merge_commit, record.before_commit), (merge, before));
    assert!(
        record.needs_person.is_some(),
        "a recovered merge is looked at by a person"
    );
    // A second pass has nothing to do.
    assert_eq!(m.w.core.recover_crew_merging(NOW + 6).await.unwrap(), 0);
}

#[tokio::test]
async fn a_crash_before_the_branch_moved_queues_the_entry_again_and_it_then_merges_once() {
    let (m, id) = queued().await;
    crash(&m.w, &id);
    let tip = m.tip().await;
    assert_eq!(m.w.core.recover_crew_merging(NOW + 5).await.unwrap(), 1);
    let entry = m.w.core.store.merge_entry(&id).unwrap().unwrap();
    assert_eq!(entry.state, "queued");
    assert!(entry.note.unwrap().contains("nothing landed"));
    assert_eq!(m.tip().await, tip, "recovery never moves the branch");
    // Only the lane's own pass runs it again.
    let outcomes = m.process(NOW + 10).await;
    assert!(
        matches!(&outcomes[..], [MergeOutcome::Merged { .. }]),
        "{outcomes:?}"
    );
    assert_eq!(
        gitq(&m.w.repo, &["rev-list", "--count", "--merges", &m.main]).await,
        "1"
    );
}

#[tokio::test]
async fn when_git_cannot_say_the_entry_waits_for_a_person_with_the_reason() {
    // The branch holds the candidate, but not as a merge the queue made (it was fast-forwarded by someone).
    let (m, id) = queued().await;
    crash(&m.w, &id);
    let candidate = m.w.core.store.merge_entry(&id).unwrap().unwrap().candidate;
    run_git(
        &m.w.repo,
        &["update-ref", &format!("refs/heads/{}", m.main), &candidate],
    );
    assert_eq!(m.w.core.recover_crew_merging(NOW + 5).await.unwrap(), 1);
    let entry = m.w.core.store.merge_entry(&id).unwrap().unwrap();
    assert_eq!(entry.state, "waiting");
    assert!(entry.note.unwrap().contains("git cannot say"));
    assert!(m.w.core.store.crew_merge(&id).unwrap().is_none());
    // Releasing it never records a merge: the change is already on the branch, so the queue rejects it and the tip stays the tip.
    m.w.core.crew_release_merge(&id).unwrap();
    let tip = m.tip().await;
    let outcomes = m.process(NOW + 10).await;
    assert!(
        matches!(&outcomes[..], [MergeOutcome::Rejected { why, .. }] if why.contains("already on")),
        "{outcomes:?}"
    );
    assert_eq!(m.tip().await, tip);
    assert!(m.w.core.store.crew_merge(&id).unwrap().is_none());
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "rejected"
    );

    // A repository git cannot read is unknown too, never a guess.
    let (m, id) = queued().await;
    let mut entry = m.w.core.store.merge_entry(&id).unwrap().unwrap();
    entry.repo = m.w.repo.join("gone").to_string_lossy().into_owned();
    m.w.core.store.add_merge_entry(&entry).unwrap();
    crash(&m.w, &id);
    assert_eq!(m.w.core.recover_crew_merging(NOW + 5).await.unwrap(), 1);
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "waiting"
    );
}

#[tokio::test]
async fn a_merge_that_is_live_in_this_process_is_not_touched() {
    let (m, id) = queued().await;
    crash(&m.w, &id);
    // The lane's lock is held for the whole of a live merge.
    let lane = m.w.core.crew_merges.lock().await;
    assert_eq!(m.w.core.recover_crew_merging(NOW + 5).await.unwrap(), 0);
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "merging"
    );
    drop(lane);
    assert_eq!(m.w.core.recover_crew_merging(NOW + 6).await.unwrap(), 1);
}

#[tokio::test]
async fn a_forge_entry_is_left_to_the_forge_reconcile() {
    let (m, id) = queued().await;
    m.w.core
        .store
        .set_merge_entry_state(&id, &OPEN_MERGE_STATES, "rejected", Some("x"))
        .unwrap()
        .expect_moved();
    // A fresh task whose intent is the forge's own: the entry says `merging`, the intent says needs_person.
    let forge = passed(&m.w, &["docs/**"], "A forge change.").await;
    forge_needs_person(&m.w.core, &forge, &m.w.repo.to_string_lossy(), "wait");
    m.w.core
        .store
        .set_merge_entry_state(&forge, &OPEN_MERGE_STATES, "merging", None)
        .unwrap()
        .expect_moved();
    assert_eq!(m.w.core.recover_crew_merging(NOW + 5).await.unwrap(), 0);
    let entry = m.w.core.store.merge_entry(&forge).unwrap().unwrap();
    assert_eq!(entry.state, "merging");
    assert_eq!(
        m.w.core
            .store
            .crew_forge_merge(&forge)
            .unwrap()
            .unwrap()
            .phase,
        ForgePhase::NeedsPerson
    );
}

#[tokio::test]
async fn a_state_write_never_overwrites_what_another_writer_decided() {
    let (m, id) = queued().await;
    let store = &m.w.core.store;
    // A person rejects the entry between the lane reading it and writing `merging`.
    m.w.core.crew_reject_merge(&id, "no").unwrap();
    assert_eq!(
        store
            .set_merge_entry_state(&id, &["queued", "blocked"], "merging", None)
            .unwrap(),
        MergeStateMove::Changed("rejected".into())
    );
    let entry = store.merge_entry(&id).unwrap().unwrap();
    assert_eq!(
        (entry.state.as_str(), entry.note.as_deref()),
        ("rejected", Some("no"))
    );
    // A task with no entry is an error, not a lost race; `merged` is not written here.
    assert!(
        store
            .set_merge_entry_state("nope", &["queued"], "blocked", None)
            .is_err()
    );
    assert!(
        store
            .set_merge_entry_state(&id, &["rejected"], "merged", None)
            .is_err()
    );
    // The lane's own pass leaves a rejected entry out.
    assert!(m.process(NOW + 1).await.is_empty());
    assert_eq!(store.merge_entry(&id).unwrap().unwrap().state, "rejected");
}

#[tokio::test]
async fn a_person_cannot_reject_a_merge_that_is_already_merging() {
    let (m, id) = queued().await;
    crash(&m.w, &id);
    assert!(m.w.core.crew_reject_merge(&id, "no").is_err());
    assert_eq!(
        m.w.core.store.merge_entry(&id).unwrap().unwrap().state,
        "merging"
    );
}
