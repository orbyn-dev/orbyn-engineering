//! Watches (docs/crew.md, "Kiro Crew features" KC6, slice 2): the runner, the state machine and the watch tick, with fake
//! engines and real git. A check is a fixed command run in the project's checkout; the first check that runs is a baseline and
//! raises nothing, only a turn between passing and failing is a change, three checks in a row that could not run raise one
//! Needs you row, and a check that runs again resolves it. Nothing calls a model while the result stays the same.

use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts as wire;
use serde_json::json;

use super::harness::{Script, Turn, World, report, world};
use super::review_support::commit_all;
use crate::crew::Page;
use crate::crew::checks::tail_lines_bytes;
use crate::crew::fixtures::audit_actions;
use crate::crew::watch::{CouldNotRun, WatchCheck, WatchFound, plan};
use crate::store::{NewBackground, WatchStateRow};
use crate::testing::run_git;

const NOW: i64 = 1_800_000_000;
const HOURS: u32 = 6;
const BUCKET: i64 = HOURS as i64 * 3600;

fn project(w: &World) -> String {
    w.core.store.thread(&w.lead).unwrap().project_id
}

fn page() -> Page {
    Page::new(None, None).unwrap()
}

fn watch_body(
    w: &World,
    name: &str,
    command: &str,
    on_failing: &str,
) -> wire::BackgroundCrewCreate {
    serde_json::from_value(json!({
        "project_id": project(w), "name": name, "template": "fix_failing_ci",
        "trigger": { "kind": "watch", "command": command, "every_hours": HOURS, "on_failing": on_failing },
        "budget": { "turns": 10, "minutes": 30 },
        "daily": { "runs": 100, "turns": 1000 }, "utc_offset_minutes": 600
    }))
    .unwrap()
}

/// A watch created through the core (so its command is screened) and turned on.
async fn watch(w: &World, name: &str, command: &str, on_failing: &str) -> String {
    let crew = w
        .core
        .background_create(&watch_body(w, name, command, on_failing))
        .await
        .unwrap();
    w.core.background_set_enabled(&crew.id, true).unwrap();
    crew.id
}

fn status(w: &World, id: &str) -> wire::BackgroundWatchStatus {
    w.core.background_watch(id).unwrap()
}

fn changes(w: &World, id: &str) -> Vec<wire::BackgroundWatchChange> {
    w.core.background_watch_changes(id, page()).unwrap().changes
}

fn runs(w: &World, id: &str) -> Vec<wire::BackgroundRun> {
    w.core.background_runs(id, page()).unwrap().runs
}

fn threads(w: &World) -> i64 {
    w.core
        .store
        .reader()
        .query_row("SELECT COUNT(*) FROM threads", [], |row| row.get(0))
        .unwrap()
}

fn claims(w: &World) -> Vec<String> {
    let reader = w.core.store.reader();
    let mut statement = reader
        .prepare("SELECT identity FROM crew_background_events ORDER BY identity")
        .unwrap();
    statement
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<std::result::Result<Vec<String>, _>>()
        .unwrap()
}

/// The checkout is clean after the harness's first commit; a changed tracked file makes `git diff --exit-code` fail, and
/// committing it makes it pass again. Every call writes different text, so making the checkout dirty again after a commit is a
/// real change.
fn make_dirty(w: &World) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static WROTE: AtomicUsize = AtomicUsize::new(0);
    let n = WROTE.fetch_add(1, Ordering::SeqCst);
    std::fs::write(w.repo.join("src/base.txt"), format!("changed {n}\n")).unwrap();
}

fn make_clean(w: &World) {
    run_git(&w.repo, &["add", "."]);
    commit_all(&w.repo, "change");
}

/// A worker that never ends, so a run it started stays running.
fn hung() -> Script {
    Arc::new(|_, _| Turn::Hang)
}

/// A worker that changes a file it may change and reports, as a real engine does.
fn good() -> Script {
    Arc::new(|n, root| {
        std::fs::write(root.join(format!("src/fix-{n}.txt")), "fixed\n").unwrap();
        Turn::Say(report("done"))
    })
}

async fn tick(w: &World, now: i64) {
    w.core.crew_background_listeners_tick(now).await.unwrap();
}

fn blank(crew: &str) -> WatchStateRow {
    WatchStateRow {
        background_id: crew.to_owned(),
        last_result: None,
        baseline_done: false,
        error_streak: 0,
        last_checked_unix: None,
        last_exit_code: None,
        last_state: "not_checked".to_owned(),
        last_reason: None,
        tail: None,
    }
}

fn ran(pass: bool, exit_code: Option<u32>, tail: Option<&str>) -> WatchCheck {
    WatchCheck {
        found: WatchFound::Ran { pass, exit_code },
        tail: tail.map(str::to_owned),
    }
}

fn missing(why: CouldNotRun) -> WatchCheck {
    WatchCheck::could_not_run(why)
}

#[test]
fn the_kept_tail_is_the_last_twenty_lines_and_two_kib_and_never_splits_a_character() {
    let long: String = (1..=25).map(|n| format!("line {n}\n")).collect();
    let kept = tail_lines_bytes(&long, 20, 2 * 1024);
    assert_eq!(kept.lines().count(), 20);
    assert!(kept.starts_with("line 6"), "{kept}");
    assert!(kept.ends_with("line 25"), "{kept}");

    let wide: String = "é".repeat(2000);
    let kept = tail_lines_bytes(&wide, 20, 2 * 1024);
    assert!(kept.len() <= 2 * 1024, "{} bytes", kept.len());
    assert!(kept.chars().all(|c| c == 'é'), "a whole character is kept");
    assert_eq!(
        kept.chars().count(),
        1024,
        "2 KiB is 1024 two-byte characters"
    );

    // A cut inside a line drops that line; a last line longer than the cap keeps its own end.
    let mixed = format!("{}\n{}", "x".repeat(4000), "tail");
    assert_eq!(tail_lines_bytes(&mixed, 20, 2 * 1024), "tail");
    assert_eq!(tail_lines_bytes("short", 20, 2 * 1024), "short");
    assert_eq!(tail_lines_bytes("", 20, 2 * 1024), "");
}

#[test]
fn the_state_machine_raises_nothing_on_a_baseline_and_only_pass_fail_changes_are_rows() {
    // The first check that runs is the baseline, whichever way it ends: a failing baseline raises nothing.
    for pass in [true, false] {
        let step = plan(
            &blank("crew"),
            &ran(pass, Some(if pass { 0 } else { 1 }), None),
            true,
            10,
        );
        assert!(step.next.baseline_done);
        assert_eq!(
            step.next.last_result.as_deref(),
            Some(if pass { "pass" } else { "fail" })
        );
        assert_eq!(
            step.change.expect("a baseline is a row"),
            ("baseline", Some(if pass { 0 } else { 1 }), None)
        );
        assert_eq!(step.action, "none");
        assert!(!step.start_template);
        assert_eq!(step.next.last_checked_unix, Some(10));
    }

    let passing = plan(&blank("crew"), &ran(true, Some(0), None), false, 10).next;

    // The same result again: the latest exit code and tail replace the last ones, and no row is written.
    let same = plan(&passing, &ran(true, Some(0), Some("again")), false, 20);
    assert!(same.change.is_none());
    assert_eq!(same.next.last_state, "passing");
    assert_eq!(same.next.tail.as_deref(), Some("again"));
    let same_fail = plan(
        &plan(&passing, &ran(false, Some(2), None), false, 20).next,
        &ran(false, Some(3), Some("another failure")),
        false,
        30,
    );
    assert!(
        same_fail.change.is_none(),
        "a different failure while it stays failing is not a change"
    );
    assert_eq!(same_fail.next.last_exit_code, Some(3));
    assert_eq!(same_fail.next.tail.as_deref(), Some("another failure"));

    // Passing to failing is the one change that acts: needs_you by itself, or the template when the trigger says so.
    let failing = plan(&passing, &ran(false, Some(2), Some("boom")), false, 20);
    assert_eq!(
        failing.change,
        Some(("failing", Some(2), Some("boom".to_owned())))
    );
    assert_eq!(failing.action, "needs_you");
    assert!(!failing.start_template);
    assert_eq!(failing.next.last_state, "failing");
    let template = plan(&passing, &ran(false, Some(2), None), true, 20);
    assert_eq!(template.action, "started_template");
    assert!(template.start_template);

    // Failing to passing is the recovery, and its row is "Passing again".
    let failing = failing.next;
    let recovered = plan(&failing, &ran(true, Some(0), None), false, 30);
    assert_eq!(recovered.change, Some(("passing", Some(0), None)));
    assert_eq!(recovered.action, "resolved");
    assert_eq!(recovered.next.last_state, "passing");
    assert!(!recovered.start_template);
}

#[test]
fn three_checks_that_could_not_run_count_a_streak_and_a_check_that_runs_again_resolves_it() {
    let passing = plan(&blank("crew"), &ran(true, Some(0), None), false, 10).next;
    let mut state = passing.clone();
    for (n, at) in (1..=3).zip([20, 30, 40]) {
        let step = plan(&state, &missing(CouldNotRun::TimedOut), false, at);
        assert!(
            step.change.is_none(),
            "a check that could not run is not a change"
        );
        assert_eq!(step.next.error_streak, n);
        assert_eq!(step.next.last_state, "could_not_run");
        assert_eq!(step.next.last_reason.as_deref(), Some("timed_out"));
        assert_eq!(
            step.next.last_result.as_deref(),
            Some("pass"),
            "the last result that ran is kept"
        );
        state = step.next;
    }
    assert_eq!(state.error_streak, 3);
    // A refusal now says so instead, and still counts.
    let refused = plan(&state, &missing(CouldNotRun::Refused), false, 50);
    assert_eq!(refused.next.error_streak, 4);
    assert_eq!(refused.next.last_reason.as_deref(), Some("refused"));

    // A check that runs again clears the streak; the same result it had is no change, so it only resolves.
    let recovered = plan(&refused.next, &ran(true, Some(0), None), false, 60);
    assert!(recovered.change.is_none());
    assert_eq!(recovered.next.error_streak, 0);
    assert_eq!(recovered.next.last_state, "passing");
    assert_eq!(recovered.next.last_reason, None);

    // Before the first check that ran, a check that runs is the baseline even after failures.
    let early = plan(
        &blank("crew"),
        &missing(CouldNotRun::CouldNotStart),
        false,
        70,
    )
    .next;
    let baseline = plan(&early, &ran(true, Some(0), None), false, 80);
    assert_eq!(
        baseline.change.map(|(kind, _, _)| kind),
        Some("baseline"),
        "the first check that runs is still the baseline"
    );
}

#[tokio::test]
async fn a_watch_checks_once_per_bucket_and_keeps_the_watch_claim_key() {
    let w = world(true, hung()).await;
    let id = watch(&w, "Diff", "git diff --exit-code", "needs_you").await;
    let bucket = NOW.div_euclid(BUCKET);

    tick(&w, NOW).await;
    assert_eq!(status(&w, &id).state, wire::BackgroundWatchState::Passing);
    assert_eq!(claims(&w), [format!("watch:{id}:{bucket}")]);
    let checked = status(&w, &id).last_checked_at;

    // The same bucket again: the claim the first check kept checks nothing new.
    tick(&w, NOW).await;
    assert_eq!(claims(&w).len(), 1);
    assert_eq!(status(&w, &id).last_checked_at, checked);

    // The next bucket is a new check.
    tick(&w, NOW + BUCKET).await;
    assert_eq!(claims(&w).len(), 2);
    assert_eq!(
        claims(&w),
        [
            format!("watch:{id}:{bucket}"),
            format!("watch:{id}:{}", bucket + 1)
        ]
    );

    // A clock that moves back checks nothing again, and a crew that is off is never looked at or claimed.
    tick(&w, NOW).await;
    assert_eq!(claims(&w).len(), 2);
    w.core.background_set_enabled(&id, false).unwrap();
    tick(&w, NOW + 2 * BUCKET).await;
    assert_eq!(claims(&w).len(), 2, "a crew that is off spends no claim");
}

#[tokio::test]
async fn a_failing_watch_raises_one_needs_you_row_and_a_pass_again_resolves_it() {
    let w = world(true, hung()).await;
    let id = watch(&w, "Diff", "git diff --exit-code", "needs_you").await;

    tick(&w, NOW).await;
    let baseline = changes(&w, &id);
    assert_eq!(baseline.len(), 1);
    assert_eq!(baseline[0].kind, wire::BackgroundWatchChangeKind::Baseline);
    assert_eq!(baseline[0].action, wire::BackgroundWatchChangeAction::None);
    assert!(w.core.crew_needs_you().unwrap().items.is_empty());

    make_dirty(&w);
    tick(&w, NOW + BUCKET).await;
    let failing = status(&w, &id);
    assert_eq!(failing.state, wire::BackgroundWatchState::Failing);
    assert_eq!(failing.last_exit_code, Some(1));
    assert!(
        failing
            .tail
            .as_deref()
            .is_some_and(|tail| tail.contains("changed")),
        "{failing:?}"
    );
    let rows = changes(&w, &id);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind, wire::BackgroundWatchChangeKind::Failing);
    assert_eq!(rows[0].action, wire::BackgroundWatchChangeAction::NeedsYou);
    let items = w.core.crew_needs_you().unwrap().items;
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].kind, wire::NeedsYouKind::WatchFailing);
    assert_eq!(items[0].id, id);
    assert_eq!(items[0].crew_id.as_deref(), Some(id.as_str()));
    assert_eq!(
        items[0].reason.as_deref(),
        Some("The watch's command is failing.")
    );

    // Still failing with a different exit code: the last code and tail update and no second row or row-repeat appears.
    std::fs::write(w.repo.join("src/base.txt"), "changed again\n").unwrap();
    tick(&w, NOW + 2 * BUCKET).await;
    assert_eq!(changes(&w, &id).len(), 2, "a repeat is not a row");
    assert_eq!(w.core.crew_needs_you().unwrap().items.len(), 1);

    make_clean(&w);
    tick(&w, NOW + 3 * BUCKET).await;
    let recovered = status(&w, &id);
    assert_eq!(recovered.state, wire::BackgroundWatchState::Passing);
    assert_eq!(recovered.error_streak, 0);
    let rows = changes(&w, &id);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].kind, wire::BackgroundWatchChangeKind::Passing);
    assert_eq!(rows[0].action, wire::BackgroundWatchChangeAction::Resolved);
    assert!(
        w.core.crew_needs_you().unwrap().items.is_empty(),
        "Passing again resolves it"
    );
}

#[tokio::test]
async fn a_template_watch_starts_one_run_and_nothing_while_nothing_changes() {
    let w = world(true, hung()).await;
    let id = watch(&w, "Template", "git diff --exit-code", "template").await;
    let before = threads(&w);

    // The baseline passes: no run, no thread and no prompt, because nothing changed.
    tick(&w, NOW).await;
    assert_eq!(threads(&w), before);
    assert!(runs(&w, &id).is_empty());
    assert!(
        w.prompts.lock().unwrap().is_empty(),
        "no model call while nothing changes"
    );

    // Turning failing starts the crew's template once, through the same start as Run now.
    make_dirty(&w);
    tick(&w, NOW + BUCKET).await;
    let started = runs(&w, &id);
    assert_eq!(started.len(), 1, "{started:?}");
    let rows = changes(&w, &id);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].kind, wire::BackgroundWatchChangeKind::Failing);
    assert_eq!(
        rows[0].action,
        wire::BackgroundWatchChangeAction::StartedTemplate
    );
    assert!(w.core.crew_needs_you().unwrap().items.is_empty());
    let tasks = w.core.store.crew_tasks_in_crew(&started[0].id).unwrap();
    assert_eq!(tasks.len(), 1, "one task per started run");
    let brief = w.core.store.crew_brief(&tasks[0].0.id).unwrap().unwrap();
    assert!(
        brief.outcome.contains("Find why the latest CI run"),
        "the run is the crew's template, never event text: {}",
        brief.outcome
    );

    // Still failing: no second run and no second row.
    tick(&w, NOW + 2 * BUCKET).await;
    assert_eq!(runs(&w, &id).len(), 1);
    assert_eq!(changes(&w, &id).len(), 2);

    // A pass resolves the row; failing again while the first run still holds the crew is a refused start, and the Needs you
    // row stays raised with one audit row beside it.
    make_clean(&w);
    tick(&w, NOW + 3 * BUCKET).await;
    assert_eq!(
        changes(&w, &id)[0].action,
        wire::BackgroundWatchChangeAction::Resolved
    );
    make_dirty(&w);
    tick(&w, NOW + 4 * BUCKET).await;
    let rows = changes(&w, &id);
    assert_eq!(rows[0].action, wire::BackgroundWatchChangeAction::NeedsYou);
    assert_eq!(
        runs(&w, &id).len(),
        1,
        "the run in progress refuses a second start"
    );
    assert_eq!(
        audit_actions(&w.core, "crew.background.listener.refused").len(),
        1,
        "a refused template start is audited like the other listeners'"
    );
    let items = w.core.crew_needs_you().unwrap().items;
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(items[0].kind, wire::NeedsYouKind::WatchFailing);
}

#[tokio::test]
async fn a_finished_template_run_leaves_one_change_row_and_the_watch_reads_nothing_after_it() {
    let w = world(true, good()).await;
    let id = watch(&w, "Good template", "git diff --exit-code", "template").await;
    tick(&w, NOW).await;
    make_dirty(&w);
    tick(&w, NOW + BUCKET).await;
    let run = runs(&w, &id).remove(0);
    let task = w.core.store.crew_tasks_in_crew(&run.id).unwrap()[0]
        .0
        .id
        .clone();
    w.until_state(&task, "passed").await;
    let run = w.core.background_runs(&id, page()).unwrap().runs.remove(0);
    assert_eq!(
        run.state,
        wire::BackgroundRunState::NeedsReview,
        "the started run is an ordinary background run, for the person to review"
    );
}

#[tokio::test]
async fn three_checks_that_could_not_run_raise_one_row_and_a_check_that_runs_resolves_it() {
    let w = world(true, hung()).await;
    // Stored the way a watch is, but with a command the screen refuses now: the check must be screened again before it runs.
    let settings = json!({
        "command": "git push --force origin main", "every_hours": HOURS, "on_failing": "needs_you"
    });
    let crew = w
        .core
        .store
        .create_background_crew(
            &NewBackground {
                project_id: &project(&w),
                name: "Unrunnable",
                template: "fix_failing_ci",
                trigger_kind: "watch",
                trigger_settings: &settings,
                budget_turns: 10,
                budget_minutes: 30,
                daily_runs: 100,
                daily_turns: 1000,
                utc_offset_minutes: 600,
            },
            "test",
        )
        .unwrap();
    w.core.background_set_enabled(&crew.id, true).unwrap();

    for (n, tick_at) in [NOW, NOW + BUCKET, NOW + 2 * BUCKET]
        .into_iter()
        .enumerate()
    {
        tick(&w, tick_at).await;
        let state = status(&w, &crew.id);
        assert_eq!(state.state, wire::BackgroundWatchState::CouldNotRun);
        assert_eq!(state.reason.as_deref(), Some("refused"));
        assert_eq!(state.error_streak, n as u32 + 1);
        assert_eq!(changes(&w, &crew.id).len(), 0, "not a change, no row");
        let items = w.core.crew_needs_you().unwrap().items;
        if n < 2 {
            assert!(items.is_empty(), "two in a row raise nothing: {items:?}");
        } else {
            assert_eq!(items.len(), 1, "{items:?}");
            assert_eq!(items[0].kind, wire::NeedsYouKind::WatchCouldNotRun);
            assert_eq!(
                items[0].reason.as_deref(),
                Some("3 checks in a row could not run: refused.")
            );
        }
    }

    // A fourth could not run updates the one row and does not repeat it.
    tick(&w, NOW + 3 * BUCKET).await;
    assert_eq!(status(&w, &crew.id).error_streak, 4);
    assert_eq!(w.core.crew_needs_you().unwrap().items.len(), 1);

    // A command that runs again is the baseline (no check ever ran), and it resolves the row.
    let safe = json!({
        "command": "git diff --exit-code", "every_hours": HOURS, "on_failing": "needs_you"
    });
    w.core
        .store
        .update_background_crew(&crew.id, None, Some(("watch", &safe)), "test")
        .unwrap();
    tick(&w, NOW + 4 * BUCKET).await;
    let state = status(&w, &crew.id);
    assert_eq!(state.state, wire::BackgroundWatchState::Passing);
    assert!(state.baseline_done);
    assert_eq!(state.error_streak, 0);
    assert_eq!(state.reason, None);
    assert_eq!(
        changes(&w, &crew.id).len(),
        1,
        "the first check that ran is the baseline"
    );
    assert!(w.core.crew_needs_you().unwrap().items.is_empty());
}

#[tokio::test]
async fn the_runner_runs_in_the_checkout_and_keeps_a_redacted_tail() {
    let w = world(true, hung()).await;
    let secret = format!("ghp_{}", "q".repeat(36));

    let clean = w
        .core
        .watch_check(&w.repo, "git diff --exit-code", Duration::from_secs(30))
        .await;
    assert_eq!(
        clean.found,
        WatchFound::Ran {
            pass: true,
            exit_code: Some(0)
        },
        "{clean:?}"
    );
    assert_eq!(
        clean.tail, None,
        "a command that printed nothing keeps no tail"
    );

    make_dirty(&w);
    let failed = w
        .core
        .watch_check(&w.repo, "git diff --exit-code", Duration::from_secs(30))
        .await;
    assert_eq!(
        failed.found,
        WatchFound::Ran {
            pass: false,
            exit_code: Some(1)
        },
        "{failed:?}"
    );
    assert!(
        failed
            .tail
            .as_deref()
            .is_some_and(|tail| tail.contains("changed")),
        "{failed:?}"
    );

    // The output is redacted through the locked redactor before it is kept: a diff that prints a token it found in the file
    // stores `[redacted]` instead.
    std::fs::write(w.repo.join("src/base.txt"), format!("token {secret}\n")).unwrap();
    let leaked = w
        .core
        .watch_check(&w.repo, "git diff --exit-code", Duration::from_secs(30))
        .await;
    let tail = leaked.tail.expect("the diff printed the changed line");
    assert!(!tail.contains(&secret), "{tail}");
    assert!(tail.contains("[redacted]"), "{tail}");

    // The screen runs again before every check: a command the rules refuse now never runs.
    let refused = w
        .core
        .watch_check(
            &w.repo,
            "git push --force origin main",
            Duration::from_secs(30),
        )
        .await;
    assert_eq!(
        refused.found,
        WatchFound::CouldNotRun(CouldNotRun::Refused),
        "{refused:?}"
    );
    assert_eq!(refused.tail, None);
}

/// A command that runs longer than the time limit: a watch stops it at five minutes, and a test shortens the limit. Windows
/// gets its own sleeper, so the branch that times out is tested on both platforms.
#[cfg(unix)]
const SLOW: &str = "sleep 5";
#[cfg(windows)]
const SLOW: &str = "powershell -NoProfile -Command \"Start-Sleep -Seconds 5\"";

#[tokio::test]
async fn a_command_that_runs_too_long_is_stopped_and_is_could_not_run_timed_out() {
    let w = world(true, hung()).await;
    let check = w
        .core
        .watch_check(&w.repo, SLOW, Duration::from_millis(100))
        .await;
    assert_eq!(
        check.found,
        WatchFound::CouldNotRun(CouldNotRun::TimedOut),
        "{check:?}"
    );
}

#[tokio::test]
async fn the_watch_reads_page_the_history_and_refuse_a_crew_that_is_not_a_watch() {
    let off = world(false, hung()).await;
    assert!(matches!(
        off.core.background_watch("x").unwrap_err(),
        crate::error::CoreError::NotFound(_)
    ));

    let w = world(true, hung()).await;
    let id = watch(&w, "Diff", "git diff --exit-code", "needs_you").await;
    make_dirty(&w);
    tick(&w, NOW).await;
    make_clean(&w);
    tick(&w, NOW + BUCKET).await;

    let first = w
        .core
        .background_watch_changes(&id, crate::crew::Page::new(Some(1), None).unwrap())
        .unwrap();
    assert_eq!(first.total, 2, "the baseline and the recovery");
    assert_eq!(first.changes.len(), 1);
    assert_eq!(first.next_offset, Some(1));
    assert_eq!(
        first.changes[0].kind,
        wire::BackgroundWatchChangeKind::Passing,
        "newest first"
    );
    let second = w
        .core
        .background_watch_changes(&id, crate::crew::Page::new(Some(1), Some(1)).unwrap())
        .unwrap();
    assert_eq!(second.changes.len(), 1);
    assert_eq!(second.next_offset, None);

    // A crew that is not a watch has no watch state and no history.
    let plain = w
        .core
        .background_create(
            &serde_json::from_value(json!({
                "project_id": project(&w), "name": "Plain", "template": "fix_failing_ci",
                "trigger": { "kind": "schedule", "every_hours": 6 },
                "budget": { "turns": 1, "minutes": 1 },
                "daily": { "runs": 1, "turns": 1 }, "utc_offset_minutes": 0
            }))
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(matches!(
        w.core.background_watch(&plain.id).unwrap_err(),
        crate::error::CoreError::Invalid(_)
    ));
    assert!(matches!(
        w.core
            .background_watch_changes(&plain.id, page())
            .unwrap_err(),
        crate::error::CoreError::Invalid(_)
    ));
}

#[test]
fn a_watch_event_names_its_bucket_in_the_claim_and_the_held_row_key() {
    assert_eq!(
        crate::crew::watch::bucket_of(NOW, HOURS),
        NOW.div_euclid(BUCKET)
    );
    assert_eq!(
        crate::crew::watch::event_key(crate::crew::watch::bucket_of(NOW, HOURS)),
        format!("watch:{}", NOW.div_euclid(BUCKET))
    );
}
