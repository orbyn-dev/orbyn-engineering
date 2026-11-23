//! The background events that start a run without a person pressing anything (docs/crew/background-engine-start.md,
//! PR 3: the `schedule` timer and the `ci_failed` watch, and KC6's `watch` timer). One owned future, like
//! `run_crew_merge_watch`: the service opts in behind the Crew Preview flag and awaits it on shutdown; there is no spawned child
//! and no OS schedule. Every firing goes through the same start as Run now (`crew_background_start`), with the event in the
//! start's `PausedTrigger` key instead of `:manual`, and is deduplicated by an event key that is kept in the store, so a repeat
//! of the same firing starts nothing new and nothing is retried on a timer. At most one start is attempted per crew per
//! pass, only enabled crews are read from the forge or claimed, and a pass runs only when the service sets an interval.
//!
//! **What is watched.** A stored crew is the authority: the schedule timer reads each `schedule` crew's `every_hours`,
//! the CI watch reads each `ci_failure` crew's branch (its setting, else the project's default) and its optional
//! workflow filter, and the watch timer reads each `watch` crew's `every_hours` and runs its stored command
//! (`crew::watch`). `issue_label` and `dependency_update` crews have no listener and are never listed here; the start seam
//! still refuses them with a plain reason. A trigger declared in a project's `.orbyn/crew.json` that has no
//! stored crew is ignored: nothing starts, nothing is adopted, and the most it earns is a diagnostic line
//! ("found in .orbyn/crew.json, no matching crew") that no screen shows.
//!
//! **Event text never reaches a prompt (decision 6).** The only fields read from outside are: a check's name and state
//! at a head commit, the head commit's sha, and a `.orbyn/crew.json` trigger's name and kind. Each of them only
//! *selects* what to start: a name is compared with the crew's stored workflow filter, a sha decides which head is
//! watched, a file trigger's name decides only whether a diagnostic line is worth writing. They appear in the dedupe
//! key, the `PausedTrigger` key (`<crew>:ci:<head>:<hash of the name>`, one event per head and failing check) and a diagnostic log,
//! and never in the brief, the worker's prompt or any other text a model reads, which `crew_background_start` keeps
//! code-owned. A check's output is never read.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::background::EVERY_HOURS_MAX;
use super::triggers::{self, TriggerDef, TriggerKind};
use super::watch::{CouldNotRun, WatchCheck};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::{
    CrewCheckState, CrewRepository, crew_base, crew_head_checks,
};
use crate::source_control::git::{git, git_hardened, text};
use crate::store::BackgroundRow;

/// The most CI crews one tick observes, so a store full of crews cannot turn a tick into unbounded forge traffic.
/// Each pass watches this many enabled crews from a rotating start, so every crew is reached over a few passes.
const CI_CREWS_MAX: usize = 64;
/// The most minutes a schedule may be told apart, as hours: the same bound the stored crew already has.
const HOURS_MAX: u32 = EVERY_HOURS_MAX;

impl Core {
    /// One owned future, with no spawned children or OS schedule: the schedule timer and the CI-failure watch
    /// together. The service opts in with an interval (the Crew Preview flag and an interval of 60s to 24h) and awaits
    /// it on shutdown. With no interval there is no pass at all, not even at startup: the future returns at once. With
    /// one, a pass runs at startup so a schedule that is already due is not missed, then once per interval.
    ///
    /// A stop is only looked at between firings, never inside one: a firing is a claim followed by a start, and a start
    /// that was cut short could leave a run with no task. A run a crash leaves like that is settled by the recovery that
    /// runs when the core opens (`recover_background_starts`).
    pub async fn run_crew_background_listeners(
        self: &Arc<Self>,
        stop: CancellationToken,
    ) -> Result<()> {
        if !self.crew_enabled() {
            return Ok(());
        }
        let Some(every) = self.config.crew_background_watch_interval else {
            return Ok(());
        };
        if !(Duration::from_secs(60)..=Duration::from_secs(86400)).contains(&every) {
            return Err(CoreError::Invalid(
                "Background watch interval must be 60 seconds to 24 hours".into(),
            ));
        }
        if stop.is_cancelled() {
            return Ok(());
        }
        let mut rotation = 0usize;
        self.crew_background_listeners_pass(crate::util::unix_now(), rotation, &stop)
            .await?;
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                _ = timer.tick() => {}
            }
            rotation = rotation.wrapping_add(CI_CREWS_MAX);
            self.crew_background_listeners_pass(crate::util::unix_now(), rotation, &stop)
                .await?;
        }
    }

    /// One pass of both listeners with nothing to stop it and no rotation: the schedules that are due, the CI failures
    /// at the heads the crews watch, and the file triggers that have no crew.
    #[cfg(test)]
    pub(crate) async fn crew_background_listeners_tick(self: &Arc<Self>, now: i64) -> Result<()> {
        self.crew_background_listeners_pass(now, 0, &CancellationToken::new())
            .await
    }

    /// One pass of both listeners. A pass never fails on one crew's trouble: each item is logged and the pass goes on,
    /// so one unreadable forge or project cannot stop the other events. `rotation` moves which CI crews are watched
    /// when there are more than `CI_CREWS_MAX`. `stop` is checked between firings only.
    pub(crate) async fn crew_background_listeners_pass(
        self: &Arc<Self>,
        now: i64,
        rotation: usize,
        stop: &CancellationToken,
    ) -> Result<()> {
        self.crew_background_schedule_tick(now, stop).await;
        if stop.is_cancelled() {
            return Ok(());
        }
        self.crew_background_watch_tick(now, stop).await;
        if stop.is_cancelled() {
            return Ok(());
        }
        self.crew_background_ci_tick(rotation, stop).await;
        if stop.is_cancelled() {
            return Ok(());
        }
        self.crew_background_file_tick().await;
        Ok(())
    }

    /// The schedule timer: each enabled `schedule` crew fires once per `every_hours` bucket of `now`. The bucket is
    /// counted from the Unix epoch, so it is stable across a restart, and the claims say which buckets have fired: a
    /// bucket at or before the newest one claimed for the crew never fires, so a clock that moves back starts nothing.
    /// A crew that is off is not looked at, so a bucket is not spent on it.
    async fn crew_background_schedule_tick(self: &Arc<Self>, now: i64, stop: &CancellationToken) {
        let crews = self
            .store
            .background_crews_of_kind("schedule")
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the schedule crews could not be read");
                Vec::new()
            });
        for crew in crews.into_iter().filter(|crew| crew.enabled) {
            if stop.is_cancelled() {
                return;
            }
            let Some(hours) = every_hours(&crew) else {
                tracing::warn!(crew = %crew.id, "a schedule crew has no usable every_hours; nothing fired");
                continue;
            };
            let bucket = now.div_euclid(i64::from(hours) * 3600);
            match self.store.latest_schedule_bucket(&crew.id) {
                Ok(Some(newest)) if bucket <= newest => continue,
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, crew = %crew.id, "a schedule's newest bucket could not be read");
                    continue;
                }
            }
            let identity = format!("schedule:{}:{bucket}", crew.id);
            match self.store.claim_background_event(&identity) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::warn!(%error, crew = %crew.id, "a schedule firing could not be claimed");
                    continue;
                }
            }
            self.fire_background_event(&crew.id, "schedule", &format!("schedule:{bucket}"))
                .await;
        }
    }

    /// The CI watch: a failed check run at the head of the branch each enabled `ci_failure` crew watches, narrowed by
    /// the crew's optional workflow filter. One read per project and branch, however many crews watch it. A crew that
    /// is off is dropped before any forge read or claim. At most one start is attempted per crew per pass, so a head
    /// with many failing checks does not fire many starts at once (the others stay unclaimed for a later pass). The
    /// check's name only selects: it is compared with the stored filter and a short hash of it goes into the claim
    /// and the firing's key; the name itself never reaches the brief, a prompt, a held row or the audit.
    async fn crew_background_ci_tick(self: &Arc<Self>, rotation: usize, stop: &CancellationToken) {
        let crews = self
            .store
            .background_crews_of_kind("ci_failure")
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the CI crews could not be read");
                Vec::new()
            });
        let crews: Vec<BackgroundRow> = crews.into_iter().filter(|crew| crew.enabled).collect();
        if crews.len() > CI_CREWS_MAX {
            tracing::warn!(
                crews = crews.len(),
                watched = CI_CREWS_MAX,
                "more CI crews than one pass watches; this pass watches {CI_CREWS_MAX} and the rest come around on the next passes"
            );
        }
        let mut branches: BTreeMap<(String, String), Vec<BackgroundRow>> = BTreeMap::new();
        for crew in window(crews, rotation, CI_CREWS_MAX) {
            let Ok(project) = self.store.project(&crew.project_id) else {
                tracing::warn!(crew = %crew.id, "the CI crew's project is gone; nothing fired");
                continue;
            };
            if !project.trusted {
                tracing::debug!(crew = %crew.id, "the CI crew's project is not trusted; nothing fired");
                continue;
            }
            let branch = match crew.trigger_settings["branch"].as_str().map(str::to_owned) {
                Some(branch) => branch,
                None => match default_branch(Path::new(&project.path)).await {
                    Some(branch) => branch,
                    None => {
                        tracing::debug!(crew = %crew.id, "the CI crew's branch could not be told; nothing fired");
                        continue;
                    }
                },
            };
            branches
                .entry((crew.project_id.clone(), branch))
                .or_default()
                .push(crew);
        }
        for ((project_id, branch), watching) in branches {
            if stop.is_cancelled() {
                return;
            }
            let Ok(project) = self.store.project(&project_id) else {
                continue;
            };
            let root = Path::new(&project.path);
            let remote = match git(root, &["remote", "get-url", "origin"]).await {
                Ok(remote) => text(&remote),
                Err(error) => {
                    tracing::debug!(%error, project = %project.name, "no origin remote to watch CI on");
                    continue;
                }
            };
            let Ok(context) = CrewRepository::from_remote(&remote) else {
                tracing::debug!(project = %project.name, "the origin remote is not a forge Orbyn can read CI from");
                continue;
            };
            // The forge reads come before any claim, so a stop that cuts one short leaves nothing claimed.
            let head = tokio::select! {
                biased;
                _ = stop.cancelled() => return,
                head = crew_base(&context, root, &branch) => match head {
                    Ok(head) => head,
                    Err(error) => {
                        tracing::debug!(%error, project = %project.name, branch = %branch, "the watched branch could not be read");
                        continue;
                    }
                },
            };
            let checks = tokio::select! {
                biased;
                _ = stop.cancelled() => return,
                checks = crew_head_checks(&context, root, &head) => match checks {
                    Ok(checks) => checks,
                    Err(error) => {
                        tracing::debug!(%error, project = %project.name, "the head's checks could not be read");
                        continue;
                    }
                },
            };
            for crew in watching {
                let Some(failed) = failed_for(&crew, &checks, &head) else {
                    continue;
                };
                for name in failed {
                    // Between firings only: nothing is claimed that is not then started.
                    if stop.is_cancelled() {
                        return;
                    }
                    // One event per head and failing check (docs/crew/background-engine-start.md, section 2), so two
                    // different failed checks at one head are two events, and the same one twice is one. The check's
                    // name is outside text: only a short hash of it is kept, in the claim and the key.
                    let tag = event_tag(&name);
                    let identity = format!("ci:{}:{head}:{tag}", crew.id);
                    match self.store.claim_background_event(&identity) {
                        Ok(true) => {}
                        Ok(false) => continue,
                        Err(error) => {
                            tracing::warn!(%error, crew = %crew.id, "a CI firing could not be claimed");
                            continue;
                        }
                    }
                    self.fire_background_event(&crew.id, "ci", &format!("ci:{head}:{tag}"))
                        .await;
                    // One start attempt per crew per pass.
                    break;
                }
            }
        }
    }

    /// The file triggers that have no crew (decision 5): nothing starts, nothing is adopted, and the most one earns is
    /// one diagnostic line that no screen shows. Only projects that already hold a background crew are read: a project
    /// with no crew is never observed, so its file starts nothing on its own. The file is read from the project's
    /// committed HEAD, its names are parser-checked data, and they never leave this diagnostic.
    async fn crew_background_file_tick(self: &Arc<Self>) {
        let projects = self.store.list_projects().unwrap_or_else(|error| {
            tracing::warn!(%error, "the projects could not be read for the background file pass");
            Vec::new()
        });
        for project in projects {
            if !self
                .store
                .background_crews_page(Some(&project.id), 1, 0)
                .is_ok_and(|(_, total)| total > 0)
            {
                continue;
            }
            let root = Path::new(&project.path);
            let Ok(policy) = git_hardened(root, &["show", "HEAD:.orbyn/crew.json"]).await else {
                continue;
            };
            let policy = String::from_utf8_lossy(&policy).trim().to_owned();
            let defs = match triggers::parse_and_validate(&policy) {
                Ok(defs) => defs,
                // A file with problems is not policy: nothing starts from it either way, and its problems are the
                // parser's to report, not this listener's.
                Err(_) => continue,
            };
            let (crews, _) = self
                .store
                .background_crews_page(
                    Some(&project.id),
                    crate::store::BACKGROUND_PER_PROJECT_MAX,
                    0,
                )
                .unwrap_or((Vec::new(), 0));
            for def in defs {
                if !crews.iter().any(|crew| matches(&def, crew)) {
                    tracing::debug!(
                        project = %project.name,
                        trigger = %def.name,
                        "found in .orbyn/crew.json, no matching crew"
                    );
                }
            }
        }
    }

    /// Starts one run for a firing whose claim is already kept, and says in one line how it went. A refusal is the
    /// start seam's plain reason (a crew that is off, a run already running, no engine, an untrusted folder, review
    /// capacity, the daily budget) and is never retried by this listener: the claim stays, and the next firing is a
    /// new event. A refusal also writes one code-owned audit row (`crew.background.listener.refused`, the crew, the
    /// listener and a fixed category), so it reads beside the crew rather than only in a log. The start's actor is
    /// the constant `LISTENER_ACTOR`, never event text, and the start is never cancelled once it has begun. Answers
    /// whether a run started, which the watch timer records on the change it fired for.
    pub(super) async fn fire_background_event(
        self: &Arc<Self>,
        crew: &str,
        listener: &str,
        event: &str,
    ) -> bool {
        match self
            .crew_background_start(crew, event, LISTENER_ACTOR)
            .await
        {
            Ok(run) => {
                tracing::info!(crew = %crew, run = %run.id, "a background event started a run");
                true
            }
            Err(error) => {
                tracing::warn!(
                    crew = %crew,
                    "a background event was refused: {}",
                    error.public_message()
                );
                if let Err(audit) = self.store.audit_background_listener_refusal(
                    crew,
                    listener,
                    refusal_reason(&error),
                ) {
                    tracing::warn!(%audit, crew = %crew, "a refused background firing could not be audited");
                }
                false
            }
        }
    }

    /// The watch timer: each enabled `watch` crew's stored command runs once per `every_hours` bucket, in the project's
    /// checkout, for at most five minutes (`crew::watch`). The bucket is claimed before the command runs, so a repeat of the
    /// same bucket checks nothing again and a crash mid-check does not repeat it; a bucket at or before the newest claimed one
    /// never runs, so a clock that moves back checks nothing. A crew that is off is never looked at and spends no claim. A check
    /// writes its own state and history and only starts a run (through `crew_background_start`, with the event in the held
    /// row's key) when a result turns failing with `on_failing` = `template`; a start that is refused is audited like the other
    /// listeners' refusals and leaves the Needs you row raised. Nothing here calls a model.
    async fn crew_background_watch_tick(self: &Arc<Self>, now: i64, stop: &CancellationToken) {
        let crews = self
            .store
            .background_crews_of_kind("watch")
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the watch crews could not be read");
                Vec::new()
            });
        for crew in crews.into_iter().filter(|crew| crew.enabled) {
            if stop.is_cancelled() {
                return;
            }
            let Some(hours) = every_hours(&crew) else {
                tracing::warn!(crew = %crew.id, "a watch crew has no usable every_hours; nothing checked");
                continue;
            };
            let Some(command) = crew.trigger_settings["command"].as_str() else {
                tracing::warn!(crew = %crew.id, "a watch crew has no command; nothing checked");
                continue;
            };
            let project = match self.store.project(&crew.project_id) {
                Ok(project) => project,
                Err(error) => {
                    tracing::warn!(%error, crew = %crew.id, "the watch crew's project is gone; nothing checked");
                    continue;
                }
            };
            let bucket = crate::crew::watch::bucket_of(now, hours);
            match self.store.latest_watch_bucket(&crew.id) {
                Ok(Some(newest)) if bucket <= newest => continue,
                Ok(_) => {}
                Err(error) => {
                    tracing::warn!(%error, crew = %crew.id, "a watch's newest bucket could not be read");
                    continue;
                }
            }
            let identity = format!("watch:{}:{bucket}", crew.id);
            match self.store.claim_background_event(&identity) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::warn!(%error, crew = %crew.id, "a watch check could not be claimed");
                    continue;
                }
            }
            let check = if project.trusted {
                // Between firings only: a check that has begun is not cut short, so its claim and its row cannot disagree.
                tokio::select! {
                    biased;
                    _ = stop.cancelled() => return,
                    check = self.watch_check(
                        Path::new(&project.path),
                        command,
                        crate::crew::watch::CHECK_LIMIT,
                    ) => check,
                }
            } else {
                WatchCheck::could_not_run(CouldNotRun::Untrusted)
            };
            let at = now;
            if let Err(error) = self
                .record_watch_check(&crew, &crate::crew::watch::event_key(bucket), check, at)
                .await
            {
                tracing::warn!(%error, crew = %crew.id, "a watch check could not be recorded");
            }
        }
    }
}

/// The audit actor of every listener start: code-owned, never event text.
const LISTENER_ACTOR: &str = "crew";

/// A fixed word for why a start was refused. Never the refusal's own text, which may name a crew or a path.
fn refusal_reason(error: &CoreError) -> &'static str {
    match error {
        CoreError::FolderUntrusted | CoreError::FolderUntrustedInit => "folder_untrusted",
        CoreError::EngineUnavailable(_) => "no_engine",
        CoreError::RunsActive => "run_in_progress",
        CoreError::PolicyDenied(_) => "not_allowed",
        CoreError::NotFound(_) => "crew_gone",
        _ => "refused",
    }
}

/// A short hash of a check's name: 16 hex characters of its SHA-256. The name is outside text and can be any length, so
/// the claim and the held row's key carry this instead, which keeps the key inside the held row's 128 characters.
pub(super) fn event_tag(name: &str) -> String {
    crate::util::sha256_hex(name.as_bytes())[..16].to_owned()
}

/// The crews one pass watches when there are more than `max`: `max` of them from `rotation` on, wrapping, so the crews
/// beyond the first `max` by name are reached on later passes instead of never. With `max` or fewer, all of them.
fn window<T>(mut items: Vec<T>, rotation: usize, max: usize) -> Vec<T> {
    let len = items.len();
    if len <= max {
        return items;
    }
    items.rotate_left(rotation % len);
    items.truncate(max);
    items
}

/// The window, for the crew tests.
#[cfg(test)]
pub(super) fn window_for_test<T>(items: Vec<T>, rotation: usize, max: usize) -> Vec<T> {
    window(items, rotation, max)
}

/// A schedule crew's `every_hours`, within the bounds the stored crew already promises.
fn every_hours(crew: &BackgroundRow) -> Option<u32> {
    crew.trigger_settings["every_hours"]
        .as_u64()
        .and_then(|hours| u32::try_from(hours).ok())
        .filter(|hours| (1..=HOURS_MAX).contains(hours))
}

/// The names of the failed checks at `head` this crew fires on: every failure when its settings name no workflow,
/// only the named one when they do. The names are outside data and are returned only to be claimed, never to be said.
fn failed_for(
    crew: &BackgroundRow,
    checks: &[crate::source_control::forge::crew::CrewCheckReceipt],
    head: &str,
) -> Option<Vec<String>> {
    let watch = crew.trigger_settings["workflow"].as_str();
    let failed: Vec<String> = checks
        .iter()
        .filter(|check| {
            check.head == head
                && check.state == CrewCheckState::Failed
                && watch.is_none_or(|named| named == check.name)
        })
        .map(|check| check.name.clone())
        .collect();
    let mut failed = failed;
    failed.sort();
    failed.dedup();
    (!failed.is_empty()).then_some(failed)
}

/// Whether a file trigger and a stored crew are the same crew: the names read equal once case and spacing are
/// ignored, and the crew's kind is one the trigger can start (a `manual` trigger matches any kind Run now can start).
fn matches(def: &TriggerDef, crew: &BackgroundRow) -> bool {
    triggers::normalized(&def.name) == triggers::normalized(&crew.name)
        && match def.kind {
            TriggerKind::CiFailed => crew.trigger_kind == "ci_failure",
            TriggerKind::Schedule => crew.trigger_kind == "schedule",
            TriggerKind::Manual => matches!(crew.trigger_kind.as_str(), "ci_failure" | "schedule"),
        }
}

/// The branch a CI crew watches when its settings name none: what the checkout's `origin` points at first
/// (`origin/HEAD`, else `origin/main`, `origin/master` or `origin/trunk`), else the checkout's own branch. `None`
/// when none can be told.
async fn default_branch(root: &Path) -> Option<String> {
    if let Ok(head) = git_hardened(
        root,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    )
    .await
    {
        let named = String::from_utf8_lossy(&head).trim().to_owned();
        if let Some(branch) = named.strip_prefix("origin/")
            && !branch.is_empty()
            && branch != "HEAD"
        {
            return Some(branch.to_owned());
        }
    }
    for guess in ["main", "master", "trunk"] {
        let remote = format!("origin/{guess}");
        if git_hardened(root, &["rev-parse", "--verify", "--quiet", &remote])
            .await
            .is_ok()
        {
            return Some(guess.to_owned());
        }
    }
    if let Ok(head) = git_hardened(root, &["symbolic-ref", "--short", "HEAD"]).await {
        let named = String::from_utf8_lossy(&head).trim().to_owned();
        if !named.is_empty() {
            return Some(named);
        }
    }
    None
}
