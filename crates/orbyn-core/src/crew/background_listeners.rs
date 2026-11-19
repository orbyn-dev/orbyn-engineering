//! The background events that start a run without a person pressing anything (docs/crew/background-engine-start.md,
//! PR 3: the `schedule` timer and the `ci_failed` watch). One owned future, like `run_crew_merge_watch`: the service
//! opts in behind the Crew Preview flag and awaits it on shutdown; there is no spawned child and no OS schedule. Every
//! firing goes through the same start as Run now (`crew_background_start`), with the event in the start's
//! `PausedTrigger` key instead of `:manual`, and is deduplicated by an event key that is kept in the store, so a repeat
//! of the same firing starts nothing new and nothing is retried on a timer.
//!
//! **What is watched.** A stored crew is the authority: the schedule timer reads each `schedule` crew's `every_hours`,
//! and the CI watch reads each `ci_failure` crew's branch (its setting, else the project's default) and its optional
//! workflow filter. `issue_label` and `dependency_update` crews have no listener and are never listed here; the start
//! seam still refuses them with a plain reason. A trigger declared in a project's `.orbyn/crew.json` that has no
//! stored crew is ignored: nothing starts, nothing is adopted, and the most it earns is a diagnostic line
//! ("found in .orbyn/crew.json, no matching crew") that no screen shows.
//!
//! **Event text never reaches a prompt (decision 6).** The only fields read from outside are: a check's name and state
//! at a head commit, the head commit's sha, and a `.orbyn/crew.json` trigger's name and kind. Each of them only
//! *selects* what to start: a name is compared with the crew's stored workflow filter, a sha decides which head is
//! watched, a file trigger's name decides only whether a diagnostic line is worth writing. They appear in the dedupe
//! key, the `PausedTrigger` key (ids and keys only) and a diagnostic log, and never in the brief, the worker's prompt
//! or any other text a model reads, which `crew_background_start` keeps code-owned. A check's output is never read.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::background::EVERY_HOURS_MAX;
use super::triggers::{self, TriggerDef, TriggerKind};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew::{
    CrewCheckState, CrewRepository, crew_base, crew_head_checks,
};
use crate::source_control::git::{git, git_hardened, text};
use crate::store::BackgroundRow;

/// The most CI crews one tick observes, so a store full of crews cannot turn a tick into unbounded forge traffic.
/// The first by name are watched and the rest are named in one line.
const CI_CREWS_MAX: usize = 64;
/// The most minutes a schedule may be told apart, as hours: the same bound the stored crew already has.
const HOURS_MAX: u32 = EVERY_HOURS_MAX;

impl Core {
    /// One owned future, with no spawned children or OS schedule: the schedule timer and the CI-failure watch
    /// together. The service must opt in (the Crew Preview flag, and an interval for the periodic pass) and await it
    /// on shutdown. A startup tick runs once even with the interval off, so a schedule that is already due is not
    /// missed; with no interval there is no periodic pass after it.
    pub async fn run_crew_background_listeners(
        self: &Arc<Self>,
        stop: CancellationToken,
    ) -> Result<()> {
        if !self.crew_enabled() {
            return Ok(());
        }
        if let Some(every) = self.config.crew_background_watch_interval
            && !(Duration::from_secs(60)..=Duration::from_secs(86400)).contains(&every)
        {
            return Err(CoreError::Invalid(
                "Background watch interval must be 60 seconds to 24 hours".into(),
            ));
        }
        let now = crate::util::unix_now();
        tokio::select! { biased; _ = stop.cancelled() => return Ok(()), result = self.crew_background_listeners_tick(now) => result? }
        let Some(every) = self.config.crew_background_watch_interval else {
            return Ok(());
        };
        let mut timer = tokio::time::interval_at(tokio::time::Instant::now() + every, every);
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                _ = timer.tick() => {}
            }
            let now = crate::util::unix_now();
            tokio::select! {
                biased;
                _ = stop.cancelled() => return Ok(()),
                result = self.crew_background_listeners_tick(now) => result?,
            }
        }
    }

    /// One pass of both listeners: the schedules that are due, the CI failures at the heads the crews watch, and the
    /// file triggers that have no crew. A pass never fails on one crew's trouble: each item is logged and the pass
    /// goes on, so one unreadable forge or project cannot stop the other events.
    pub(crate) async fn crew_background_listeners_tick(self: &Arc<Self>, now: i64) -> Result<()> {
        self.crew_background_schedule_tick(now).await;
        self.crew_background_ci_tick().await;
        self.crew_background_file_tick().await;
        Ok(())
    }

    /// The schedule timer: each `schedule` crew fires once per `every_hours` bucket of `now`. The bucket is counted from
    /// the Unix epoch, so it is stable across a restart and the row the claim keeps says which bucket has fired. A
    /// crew that is off is fired and refused by the start seam, which burns the bucket's claim: turning a crew on
    /// mid-bucket starts nothing until the next bucket.
    async fn crew_background_schedule_tick(self: &Arc<Self>, now: i64) {
        let crews = self
            .store
            .background_crews_of_kind("schedule")
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the schedule crews could not be read");
                Vec::new()
            });
        for crew in crews {
            let Some(hours) = every_hours(&crew) else {
                tracing::warn!(crew = %crew.id, "a schedule crew has no usable every_hours; nothing fired");
                continue;
            };
            let bucket = now.div_euclid(i64::from(hours) * 3600);
            let identity = format!("schedule:{}:{bucket}", crew.id);
            match self.store.claim_background_event(&identity) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    tracing::warn!(%error, crew = %crew.id, "a schedule firing could not be claimed");
                    continue;
                }
            }
            self.fire_background_event(&crew.id, &format!("schedule:{bucket}"))
                .await;
        }
    }

    /// The CI watch: a failed check run at the head of the branch each `ci_failure` crew watches, narrowed by the
    /// crew's optional workflow filter. One read per project and branch, however many crews watch it. The check's
    /// name only selects: it is compared with the stored filter and written into the dedupe key, and it never reaches
    /// the run the firing starts.
    async fn crew_background_ci_tick(self: &Arc<Self>) {
        let crews = self
            .store
            .background_crews_of_kind("ci_failure")
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the CI crews could not be read");
                Vec::new()
            });
        if crews.len() > CI_CREWS_MAX {
            tracing::warn!(
                crews = crews.len(),
                watched = CI_CREWS_MAX,
                "more CI crews than one pass watches; the rest wait for the next pass"
            );
        }
        let mut branches: BTreeMap<(String, String), Vec<BackgroundRow>> = BTreeMap::new();
        for crew in crews.into_iter().take(CI_CREWS_MAX) {
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
            let head = match crew_base(&context, root, &branch).await {
                Ok(head) => head,
                Err(error) => {
                    tracing::debug!(%error, project = %project.name, branch = %branch, "the watched branch could not be read");
                    continue;
                }
            };
            let checks = match crew_head_checks(&context, root, &head).await {
                Ok(checks) => checks,
                Err(error) => {
                    tracing::debug!(%error, project = %project.name, "the head's checks could not be read");
                    continue;
                }
            };
            for crew in watching {
                let Some(failed) = failed_for(&crew, &checks, &head) else {
                    continue;
                };
                for name in failed {
                    let identity = format!("ci:{}:{head}:{name}", crew.id);
                    match self.store.claim_background_event(&identity) {
                        Ok(true) => {}
                        Ok(false) => continue,
                        Err(error) => {
                            tracing::warn!(%error, crew = %crew.id, "a CI firing could not be claimed");
                            continue;
                        }
                    }
                    // The event key in the start's trigger is the head alone: ids and keys, never the check's name.
                    self.fire_background_event(&crew.id, &format!("ci:{head}"))
                        .await;
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
    /// start seam's plain reason (a crew that is off, a run already running, no engine, review capacity) and is never
    /// retried by this listener: the claim stays, and the next firing is a new event.
    async fn fire_background_event(self: &Arc<Self>, crew: &str, event: &str) {
        match self.crew_background_start(crew, event).await {
            Ok(run) => {
                tracing::info!(crew = %crew, run = %run.id, "a background event started a run")
            }
            Err(error) => {
                tracing::warn!(
                    crew = %crew,
                    "a background event was refused: {}",
                    error.public_message()
                )
            }
        }
    }
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
