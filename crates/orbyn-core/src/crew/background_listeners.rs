//! The background events that start a run without a person pressing anything (docs/crew/background-engine-start.md,
//! PR 3: the `schedule` timer and the `ci_failed` watch; KC7: the `issue_label` listener and the `dependency_update`
//! tick). One owned future, like `run_crew_merge_watch`: the service opts in behind the Crew Preview flag and awaits
//! it on shutdown; there is no spawned child and no OS schedule. Every firing goes through the same start as Run now
//! (`crew_background_start`), with the event in the start's `PausedTrigger` key instead of `:manual`, and is
//! deduplicated by an event key that is kept in the store, so a repeat of the same firing starts nothing new and
//! nothing is retried on a timer. At most one start is attempted per crew per pass, only enabled crews are read from
//! the forge or the tracker or claimed, and a pass runs only when the service sets an interval.
//!
//! **What is watched.** A stored crew is the authority: the schedule timer reads each `schedule` crew's `every_hours`,
//! the CI watch reads each `ci_failure` crew's branch (its setting, else the project's default) and its optional
//! workflow filter, and the issue-label listener reads the open issues in the project's tracker scope that carry the
//! crew's stored `label`. An issue-label crew starts with a baseline: its first pass claims the labelled issues
//! already there and starts nothing, so turning a crew on never runs on old labels; only a later label-set change
//! (while the crew's label is present) is a new event, and a removal and re-add is not (the readers give no per-label
//! event time; a label feed is a reader follow-up). A project is read only through the one tracker reader that can answer for it (the connected
//! tracker its origin remote names; Linear names no project and answers none), so a project with no reader, an
//! untrusted folder and a crew that is off spend no event and start nothing. A `dependency_update` crew has its tick
//! and the same claim, start and refusal path, but no reader can report a dependency or advisory update yet, so that
//! tick answers [`DependencyFeed::Cannot`] and fires nothing (follow-up #1360). `watch` crews have no listener (KC6);
//! the start seam still refuses what Run now cannot start with a plain reason. A trigger declared in a project's
//! `.orbyn/crew.json` that has no stored crew is ignored: nothing starts, nothing is adopted, and the most it earns is
//! a diagnostic line ("found in .orbyn/crew.json, no matching crew") that no screen shows.
//!
//! **Event text never reaches a prompt (decision 6).** The only fields read from outside are: a check's name and state
//! at a head commit, the head commit's sha, a tracker issue's key and whether it carries the stored label, the key of a
//! dependency update once a reader has that feed, and a `.orbyn/crew.json` trigger's name and kind. Each of them only
//! *selects* what to start: a name is compared with the crew's stored workflow filter, a sha decides which head is
//! watched, a label decides which issue a crew watches, a file trigger's name decides only whether a diagnostic line
//! is worth writing. They appear in the dedupe key, the `PausedTrigger` key (a hash of the outside fields, one event
//! per issue, label and label set, or per head and failing check) and a diagnostic log, and never in the brief, the worker's
//! prompt or any other text a model reads, which `crew_background_start` keeps code-owned. A check's output, an
//! issue's title and body and a tracker's update text are never read.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use orbyn_contracts::IssueLabel;
use tokio_util::sync::CancellationToken;

use super::background::EVERY_HOURS_MAX;
use super::triggers::{self, TriggerDef, TriggerKind};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::issues::{DependencyFeed, IssueQuery, MAX_LIMIT, ProjectReader};
use crate::source_control::forge::crew::{
    CrewCheckState, CrewRepository, crew_base, crew_head_checks,
};
use crate::source_control::git::{git, git_hardened, text};
use crate::store::{BackgroundRow, tracker_name};

/// The most CI crews one tick observes, so a store full of crews cannot turn a tick into unbounded forge traffic.
/// Each pass watches this many enabled crews from a rotating start, so every crew is reached over a few passes.
const CI_CREWS_MAX: usize = 64;
/// The most minutes a schedule may be told apart, as hours: the same bound the stored crew already has.
const HOURS_MAX: u32 = EVERY_HOURS_MAX;

impl Core {
    /// One owned future, with no spawned children or OS schedule: the schedule timer, the CI-failure watch and the
    /// issue-reader listeners together. The service opts in with an interval (the Crew Preview flag and an interval of
    /// 60s to 24h) and awaits it on shutdown. With no interval there is no pass at all, not even at startup: the future
    /// returns at once. With one, a pass runs at startup so a schedule that is already due is not missed, then once per
    /// interval.
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

    /// One pass of every listener with nothing to stop it and no rotation: the schedules that are due, the CI failures
    /// at the heads the crews watch, the labelled issues and dependency updates the readers report, and the file
    /// triggers that have no crew.
    #[cfg(test)]
    pub(crate) async fn crew_background_listeners_tick(self: &Arc<Self>, now: i64) -> Result<()> {
        self.crew_background_listeners_pass(now, 0, &CancellationToken::new())
            .await
    }

    /// One pass of every listener. A pass never fails on one crew's trouble: each item is logged and the pass goes on,
    /// so one unreadable forge, tracker or project cannot stop the other events. `rotation` moves which CI crews are
    /// watched when there are more than `CI_CREWS_MAX`. `stop` is checked between firings only.
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
        self.crew_background_ci_tick(rotation, stop).await;
        if stop.is_cancelled() {
            return Ok(());
        }
        self.crew_background_issue_label_tick(stop).await;
        if stop.is_cancelled() {
            return Ok(());
        }
        self.crew_background_dependency_update_tick(stop).await;
        if stop.is_cancelled() {
            return Ok(());
        }
        // The file pass stays last: it reads committed files of projects that already hold a crew and starts nothing.
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

    /// The issue-label listener: an enabled `issue_label` crew fires on an open issue in its project's tracker scope
    /// that carries the crew's stored `label` and that it has not seen before. On its first pass for a crew (no
    /// `issue_label:<crew>:baseline` claim yet) it claims every matching issue the tracker reports right now and
    /// starts nothing, so turning a crew on never starts runs for labels that were already there; it then keeps the
    /// baseline claim for that label. A later pass fires only on an issue whose claim it does not have. The claim keeps
    /// a hash of the issue's whole label set, because the readers give no per-label event time: an edit that leaves the
    /// labels alone is inert, adding the label or changing the set while it is present is a new event, and a label
    /// removed and added again returns to a set already claimed and does not fire (a reader-side label feed is the
    /// follow-up). Only a
    /// project with a reader is read at all (see [`Core::project_issue_reader`]); a tracker that cannot answer spends
    /// no event. The newest unseen issue wins, so one pass attempts at most one start per crew and the rest wait for a
    /// later pass. The issue's key only selects: the claim keeps it, the shorter start key keeps hashes, and the
    /// issue's title, body and comments are never read (decision 6).
    async fn crew_background_issue_label_tick(self: &Arc<Self>, stop: &CancellationToken) {
        let crews = self
            .store
            .background_crews_of_kind("issue_label")
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the issue label crews could not be read");
                Vec::new()
            });
        for crew in crews.into_iter().filter(|crew| crew.enabled) {
            if stop.is_cancelled() {
                return;
            }
            let Some(reader) = self.background_project_reader(&crew).await else {
                continue;
            };
            let Some(label) = crew.trigger_settings["label"].as_str() else {
                tracing::warn!(crew = %crew.id, "an issue label crew has no usable label; nothing fired");
                continue;
            };
            // The first sight of a (crew, label) is a baseline: the labels already on its issues are not events. The
            // marker is per stored label, so changing a crew's label baselines the issues that already carry the new one,
            // and it is kept after every matching issue is claimed, so a pass cut short is a baseline again next time.
            let marker = format!("issue_label:{}:baseline:{}", crew.id, event_tag(label));
            let baseline = match self.store.background_event_claimed(&marker) {
                Ok(kept) => !kept,
                Err(error) => {
                    tracing::warn!(%error, crew = %crew.id, "an issue label crew's baseline could not be read");
                    continue;
                }
            };
            let query = IssueQuery {
                tracker: Some(reader.tracker),
                labels: vec![label.to_owned()],
                limit: MAX_LIMIT,
                ..IssueQuery::default()
            };
            let mut page = match self
                .scope_issues(reader.tracker, &reader.scope, &query)
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    tracing::debug!(%error, crew = %crew.id, "the project's issues could not be read; nothing fired");
                    continue;
                }
            };
            // Newest update first, and a stable order for a tie, so which issue wins is not the tracker's page order.
            page.issues.sort_by(|a, b| {
                b.updated_at
                    .cmp(&a.updated_at)
                    .then_with(|| a.key.cmp(&b.key))
            });
            let mut baseline_failed = false;
            for issue in page.issues {
                if stop.is_cancelled() {
                    return;
                }
                // The listing asks for the label; a tracker that ignores its own filter must not fire on an unlabelled
                // issue, so the stored label is checked once more against what the reader reported.
                if !issue
                    .labels
                    .iter()
                    .any(|name| name.name.eq_ignore_ascii_case(label))
                {
                    continue;
                }
                let identity = issue_label_claim(
                    &crew.id,
                    tracker_name(reader.tracker),
                    &reader.scope,
                    &issue.key,
                    label,
                    &label_set_tag(&issue.labels),
                );
                match self.store.claim_background_event(&identity) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        tracing::warn!(%error, crew = %crew.id, "an issue label firing could not be claimed");
                        baseline_failed = true;
                        continue;
                    }
                }
                if baseline {
                    // First sight of this crew: the issue is claimed so it can never fire, and nothing starts.
                    continue;
                }
                self.fire_background_event(
                    &crew.id,
                    "issue_label",
                    &issue_label_event(&reader.scope, &issue.key, label),
                )
                .await;
                // One start attempt per crew per pass.
                break;
            }
            if baseline {
                // The marker comes last: a pass that could not claim every issue is a baseline again next time, rather
                // than letting labels that were already there fire.
                if !baseline_failed && let Err(error) = self.store.claim_background_event(&marker) {
                    tracing::warn!(%error, crew = %crew.id, "an issue label crew's baseline could not be kept");
                }
            }
        }
    }

    /// The dependency-update listener: an enabled `dependency_update` crew fires on a dependency or advisory update
    /// its project's tracker reader reports, narrowed by the crew's optional `ecosystem` setting. No reader has such a
    /// feed yet ([`IssueSource::dependency_updates`]), so today every tracker answers [`DependencyFeed::Cannot`]:
    /// the tick spends no event, starts nothing and writes no refusal for a tracker that cannot answer. The one
    /// trace line per pass says so once, never once per update.
    async fn crew_background_dependency_update_tick(self: &Arc<Self>, stop: &CancellationToken) {
        let crews = self
            .store
            .background_crews_of_kind("dependency_update")
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "the dependency update crews could not be read");
                Vec::new()
            });
        let mut observed = false;
        let mut answered = false;
        for crew in crews.into_iter().filter(|crew| crew.enabled) {
            observed = true;
            if stop.is_cancelled() {
                return;
            }
            let Some(reader) = self.background_project_reader(&crew).await else {
                continue;
            };
            let feed = match self
                .tracker_dependency_updates(reader.tracker, &reader.scope)
                .await
            {
                Ok(feed) => feed,
                Err(error) => {
                    tracing::debug!(%error, crew = %crew.id, "the project's dependency feed could not be read; nothing fired");
                    continue;
                }
            };
            let DependencyFeed::Updates(mut updates) = feed else {
                continue;
            };
            answered = true;
            let wanted = crew.trigger_settings["ecosystem"].as_str();
            updates.sort_by(|a, b| a.key.cmp(&b.key));
            for update in updates {
                if stop.is_cancelled() {
                    return;
                }
                // The crew names an ecosystem: an update the feed does not place in it is not this crew's event.
                if wanted.is_some_and(|wanted| update.ecosystem.as_deref() != Some(wanted)) {
                    continue;
                }
                let tag = event_tag(&update.key);
                let identity = format!("dependency_update:{}:{tag}", crew.id);
                match self.store.claim_background_event(&identity) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        tracing::warn!(%error, crew = %crew.id, "a dependency update firing could not be claimed");
                        continue;
                    }
                }
                self.fire_background_event(
                    &crew.id,
                    "dependency_update",
                    &format!("dependency_update:{tag}"),
                )
                .await;
                // One start attempt per crew per pass.
                break;
            }
        }
        if observed && !answered {
            tracing::trace!(
                "no tracker reader reports dependency or advisory updates yet; the dependency update listeners fired nothing"
            );
        }
    }

    /// The project behind a crew and the issue reader that can answer for it, or none. A crew whose project is gone
    /// or untrusted, or whose checkout no connected tracker can answer for, is never read: no event is spent and no
    /// refusal is written, because a tracker that cannot answer is not a refusal.
    async fn background_project_reader(&self, crew: &BackgroundRow) -> Option<ProjectReader> {
        let project = match self.store.project(&crew.project_id) {
            Ok(project) => project,
            Err(error) => {
                tracing::debug!(%error, crew = %crew.id, "a background crew's project could not be read; nothing fired");
                return None;
            }
        };
        if !project.trusted {
            tracing::debug!(crew = %crew.id, "a background crew's project is not trusted; nothing fired");
            return None;
        }
        match self.project_issue_reader(&project).await {
            Ok(reader) => reader,
            Err(error) => {
                tracing::debug!(%error, crew = %crew.id, "a background crew's tracker could not be read; nothing fired");
                None
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
    /// the constant `LISTENER_ACTOR`, never event text, and the start is never cancelled once it has begun.
    async fn fire_background_event(self: &Arc<Self>, crew: &str, listener: &str, event: &str) {
        match self
            .crew_background_start(crew, event, LISTENER_ACTOR)
            .await
        {
            Ok(run) => {
                tracing::info!(crew = %crew, run = %run.id, "a background event started a run")
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

/// The claim of one issue label event: the crew, the tracker and scope, the issue's key, a short hash of the label
/// and a short hash of the issue's whole label set. The readers give no per-label event time, so the label set is the
/// event: an edit that leaves the labels alone is inert, adding the label (or any label-set change while it is
/// present) is a new claim, and a removal and re-add returns to a set already claimed and is not a new event. A claim
/// row has no length limit, so the scope and key are kept whole here; the shorter start key below hashes them. The key
/// and label set are untrusted outside text and are only an identity: neither reaches a prompt.
fn issue_label_claim(
    crew: &str,
    tracker: &str,
    scope: &str,
    key: &str,
    label: &str,
    label_set: &str,
) -> String {
    format!(
        "issue_label:{crew}:{tracker}:{scope}:{key}:{}:{label_set}",
        event_tag(label)
    )
}

/// A short hash of an issue's whole label set, for the claim identity. Names are sorted and de-duplicated and each is
/// framed by its character count, so two different sets cannot hash alike by running two names together.
pub(super) fn label_set_tag(labels: &[IssueLabel]) -> String {
    let mut names: Vec<&str> = labels.iter().map(|label| label.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    let mut framed = String::new();
    for name in names {
        framed.push_str(&name.chars().count().to_string());
        framed.push(':');
        framed.push_str(name);
    }
    crate::util::sha256_hex(framed.as_bytes())[..16].to_owned()
}

/// The start key's event part for one issue label: hashes of the scope, the issue's key and the stored label, never
/// their text. It keeps the held row's `<crew>:<event>` key inside its 128 characters whatever the tracker's scope
/// and key look like, and the label is the person's own stored setting, not event text.
fn issue_label_event(scope: &str, key: &str, label: &str) -> String {
    format!(
        "issue_label:{}:{}:{}",
        event_tag(scope),
        event_tag(key),
        event_tag(label)
    )
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
