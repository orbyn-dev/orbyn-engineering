//! The Crew "needs you" inbox (Crew Preview, KC3 PR1): one read, across every crew, of what waits for the person, and the
//! person's device-local dismissal of the stops they do not want to see again. It adds no state: every item is a view of what
//! Crew already keeps (a task's state, a merge's `needs_person`, a queued merge's waiting state, an open hold in the
//! Autopilot's memory, a background run's wait, the Autopilot ledger, a manual check). Nothing here moves a task, answers a
//! merge or settles a run; the dismissal is a row of its own and touches nothing behind the item.
//!
//! A task appears once, unless a command held for its worker waits beside it: the hold is an item of its own. The first rule
//! that matches wins, in this order: a task whose last cancel attempt could not stop its
//! worker (its worker may still run, so it comes before the same task waiting on a person), then a task waiting on a person,
//! then a merge waiting on
//! the person (after it landed, for review, and before it lands, at the merge gate), then a command Autopilot held for the
//! person, then a background run waiting for review, then a stopped task with a trip wire, then a stopped task whose worker
//! stopped twice. Other stopped and cancelled tasks are not items. Every text is one line, redacted and capped at 160
//! characters; a task carries its own scrubbed reason when its latest `needs_person` result has one (the question, the refusal
//! that kept it from starting, a manual check), and every other kind carries fixed core words.
//!
//! Since KC3 PR2 this list is the one field: the Taskboard's Needs you column, the Review queue's `waiting_elsewhere`, the
//! dashboard's `live` row, the Autopilot digest's `needs_person` count and the Flow `you` cell read it instead of deriving
//! their own rule, so a dismissal hides the wait wherever the field is read. The waits the frames draw and the old field had
//! no kind for are items too: a command Autopilot held for the person (`CommandApproval`, answered in Approvals) and a merge
//! the queue holds at the gate before it lands (`MergeGateWaiting`, released by the person); a merge that already landed and
//! waits for review stays `MergeWaiting`. They read the same list, not necessarily equal loads: the board cards only work
//! tasks and the dashboard narrows to one repository. The Taskboard and the dashboard are refreshed while their screen is
//! open, so those reads pass a crew or repository scope the store narrows in SQL and never load every crew's records; the
//! Inbox, the Review queue and the digest read the whole device.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use orbyn_contracts as wire;
use orbyn_contracts::ResultState;

use super::flow::facts::Facts;
use super::flow::scrub::Scrub;
use super::{valid_id, wire_name};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::util::{format_unix, parse_time};

/// The most items one request may ask to announce.
pub const NOTIFY_MAX: usize = 50;

/// The typed stop detail a worker that stopped twice carries. Its `stop_reason` stays `engine_error`; another
/// engineer writes the row, and this module only reads it.
pub const WORKER_STOPPED_TWICE: &str = "worker_stopped_twice";

/// The most items one read answers with.
const CAP: usize = 200;

/// How wide a needs-you read reaches. The Inbox and the Review queue read every crew on the device; the Taskboard reads one
/// crew and the dashboard one repository. A narrower read passes its crew or project to the store reads too, so the other
/// crews' rows are never loaded on a screen's 30-second refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scope<'a> {
    /// Every crew on this device.
    Device,
    /// One crew: the Taskboard's read.
    Crew(&'a str),
    /// One repository: the dashboard's read.
    Project(&'a str),
}

impl<'a> Scope<'a> {
    /// Whether an item of this project and crew is in scope. The store reads are already narrowed to a crew or a project;
    /// this keeps the rule in one place for the reads that still see other rows.
    fn keeps(self, project: Option<&str>, crew: &str) -> bool {
        match self {
            Self::Device => true,
            Self::Crew(id) => crew == id,
            Self::Project(id) => project == Some(id),
        }
    }

    /// The crew the store reads narrow to, when this read is for one crew.
    fn crew(self) -> Option<&'a str> {
        match self {
            Self::Crew(id) => Some(id),
            Self::Device | Self::Project(_) => None,
        }
    }

    /// The repository the store reads narrow to, when this read is for one repository.
    fn project(self) -> Option<&'a str> {
        match self {
            Self::Project(id) => Some(id),
            Self::Device | Self::Crew(_) => None,
        }
    }
}

/// The longest reason, in characters.
const REASON_MAX: usize = 160;
/// The longest question, in characters.
const QUESTION_MAX: usize = 600;

/// The fixed reason of a kind, used when the task behind it has no scrubbed reason of its own.
fn fixed_reason(kind: wire::NeedsYouKind) -> Option<&'static str> {
    match kind {
        wire::NeedsYouKind::TaskNeedsPerson => None,
        wire::NeedsYouKind::CouldNotStart => Some("The worker never ran."),
        wire::NeedsYouKind::ManualCheck => Some("A check only a person can make is waiting."),
        wire::NeedsYouKind::MergeWaiting => Some("A merge needs your decision."),
        wire::NeedsYouKind::MergeGateWaiting => Some("A merge waits for your release."),
        wire::NeedsYouKind::CommandApproval => Some("A command waits for your approval."),
        wire::NeedsYouKind::BackgroundReview => {
            Some("A background run is waiting for your review.")
        }
        wire::NeedsYouKind::WatchFailing => Some("The watch's command is failing."),
        // The streak and the fixed reason word say it, so it is built per item.
        wire::NeedsYouKind::WatchCouldNotRun => None,
        wire::NeedsYouKind::TripwireStop => Some("A trip wire stopped the task."),
        wire::NeedsYouKind::WorkerStoppedTwice => Some("The worker stopped twice."),
        wire::NeedsYouKind::CouldNotStop => Some("The worker could not be stopped. Stop it again."),
    }
}

/// Whether the person may dismiss this kind. A wait that is answered rather than hidden is not dismissible.
fn dismissible(kind: wire::NeedsYouKind) -> bool {
    matches!(
        kind,
        wire::NeedsYouKind::CouldNotStart
            | wire::NeedsYouKind::TripwireStop
            | wire::NeedsYouKind::WorkerStoppedTwice
    )
}

/// Whether `one` is a later RFC 3339 time than `other`. A pair that does not parse falls back to the stored strings.
fn later(one: &str, other: &str) -> bool {
    parse_time(one)
        .zip(parse_time(other))
        .map_or_else(|| one > other, |(one, other)| one > other)
}

/// One line: whitespace collapsed, empty dropped. The text was already redacted and capped.
fn line(text: String) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

impl Core {
    /// A task's own scrubbed reason: its latest `needs_person` result. A task that could not start has the refusal there, a
    /// manual check has its own words, and a question has the briefer's sentence; the kind's fixed words stand in when this is
    /// empty, so the CouldNotStart item carries the task's refusal rather than "The worker never ran." alone. The second part
    /// is the question for the item, and whether the scan hid it: when it did, the reason is not given either, because the
    /// text around a hidden secret is still part of it.
    fn needs_you_reason(
        &self,
        facts: &Facts,
        scrub: &Scrub,
    ) -> Result<(Option<String>, Option<String>, bool)> {
        let Some(result) = self
            .store
            .crew_results(&facts.task.id)?
            .into_iter()
            .rev()
            .find(|result| result.state == ResultState::NeedsPerson)
        else {
            return Ok((None, None, false));
        };
        let (question, hidden) = scrub.marked(&result.summary, QUESTION_MAX);
        if hidden {
            return Ok((None, None, true));
        }
        Ok((
            line(scrub.text(&result.summary, REASON_MAX)),
            line(question),
            false,
        ))
    }

    /// Every item in `scope`, dismissed or not, before the cap and the order. The first rule for an id wins. The scope narrows
    /// each store read to its crew or repository, and is asked again per item before any facts, results or scrubs are loaded,
    /// so a read for one crew never pays for another's.
    fn needs_you_items(&self, scope: Scope<'_>) -> Result<Vec<wire::NeedsYouItem>> {
        let person = self.display_redactor()?;
        let mut scrubs: BTreeMap<String, Scrub> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut items = Vec::new();
        let mut stopped = Vec::new();

        for waiting in self
            .store
            .crew_could_not_stop(scope.crew(), scope.project())?
        {
            if !scope.keeps(waiting.project_id.as_deref(), &waiting.task.crew_id) {
                continue;
            }
            if !seen.insert(waiting.task.id.clone()) {
                continue;
            }
            let kind = wire::NeedsYouKind::CouldNotStop;
            let brief = self.store.crew_brief(&waiting.task.id)?;
            let route = self.store.crew_route_decisions(&waiting.task.id)?.pop();
            let scrub = match scrubs.entry(waiting.task.crew_id.clone()) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => {
                    entry.insert(Scrub::of_crew(self, &person, &waiting.task.crew_id)?)
                }
            };
            let name = brief
                .as_ref()
                .and_then(|brief| line(scrub.text(&brief.outcome, REASON_MAX)));
            let engine = route.as_ref().map(|route| route.engine.as_str().to_owned());
            let model = route
                .as_ref()
                .map(|route| scrub.text(route.model.as_str(), 80));
            let task_kind = brief.as_ref().map(|brief| wire_name(&brief.kind));
            items.push(wire::NeedsYouItem {
                id: waiting.task.id.clone(),
                kind,
                project_id: waiting.project_id,
                crew_id: Some(waiting.task.crew_id),
                since: waiting.since,
                dismissible: dismissible(kind),
                reason: fixed_reason(kind).map(str::to_owned),
                name,
                engine,
                model,
                task_kind,
                question: None,
                question_hidden: false,
            });
        }

        for place in self.store.crew_tasks_in_states(
            &["needs_person", "stopped"],
            scope.crew(),
            scope.project(),
        )? {
            if !scope.keeps(place.project_id.as_deref(), &place.task.crew_id) {
                continue;
            }
            if place.task.state == "stopped" {
                stopped.push(place);
                continue;
            }
            let facts = self.flow_facts(place.task.clone())?;
            let kind = if facts.could_not_start() {
                wire::NeedsYouKind::CouldNotStart
            } else if !self
                .store
                .crew_manual_checks_waiting(&place.task.id)?
                .is_empty()
            {
                wire::NeedsYouKind::ManualCheck
            } else {
                wire::NeedsYouKind::TaskNeedsPerson
            };
            // The task's own scrubbed result first: for a task that could not start, that result is the refusal the person
            // acts on, and taking a kind's fixed words instead was hiding it from the Board card and the Inbox row.
            let (reason, question, question_hidden) = {
                let scrub = match scrubs.entry(place.task.crew_id.clone()) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        entry.insert(Scrub::of_crew(self, &person, &place.task.crew_id)?)
                    }
                };
                let (reason, question, hidden) = self.needs_you_reason(&facts, scrub)?;
                let reason = match reason {
                    Some(reason) => Some(reason),
                    None => fixed_reason(kind).map(str::to_owned),
                };
                let asked = (kind == wire::NeedsYouKind::TaskNeedsPerson).then_some(question);
                (
                    reason,
                    asked.flatten(),
                    hidden && kind == wire::NeedsYouKind::TaskNeedsPerson,
                )
            };
            if seen.insert(place.task.id.clone()) {
                items.push(wire::NeedsYouItem {
                    id: place.task.id.clone(),
                    kind,
                    project_id: place.project_id,
                    crew_id: Some(place.task.crew_id),
                    since: place.task.updated_at,
                    dismissible: dismissible(kind),
                    reason,
                    name: None,
                    engine: None,
                    model: None,
                    task_kind: None,
                    question,
                    question_hidden,
                });
            }
        }

        let merges = self
            .store
            .crew_waiting_merges(scope.crew(), scope.project())?;
        let task_ids: Vec<String> = merges.iter().map(|merge| merge.task_id.clone()).collect();
        let projects = self.store.crew_projects_of_tasks(&task_ids)?;
        for merge in merges {
            if !scope.keeps(
                projects.get(&merge.task_id).map(String::as_str),
                &merge.crew_id,
            ) || !seen.insert(merge.task_id.clone())
            {
                continue;
            }
            items.push(wire::NeedsYouItem {
                id: merge.task_id.clone(),
                kind: wire::NeedsYouKind::MergeWaiting,
                project_id: projects.get(&merge.task_id).cloned(),
                crew_id: Some(merge.crew_id),
                since: format_unix(merge.merged_unix),
                dismissible: false,
                reason: fixed_reason(wire::NeedsYouKind::MergeWaiting).map(str::to_owned),
                name: None,
                engine: None,
                model: None,
                task_kind: None,
                question: None,
                question_hidden: false,
            });
        }

        // A command Autopilot held for the person. The holds live in memory until decided, so this is a snapshot of that
        // book and not a store read; a command held for a worker's task is a wait of its own (the task may keep working
        // beside it, so it is not a task wait and does not take the task's place in the list).
        let holds = self.held_waiting(crate::util::unix_now());
        if !holds.is_empty() {
            let task_ids: Vec<String> = holds.iter().map(|hold| hold.task_id.clone()).collect();
            let projects = self.store.crew_projects_of_tasks(&task_ids)?;
            for hold in holds {
                let project = projects.get(&hold.task_id).cloned();
                if !scope.keeps(project.as_deref(), &hold.crew_id) || !seen.insert(hold.id.clone())
                {
                    continue;
                }
                items.push(wire::NeedsYouItem {
                    kind: wire::NeedsYouKind::CommandApproval,
                    id: hold.id,
                    project_id: project,
                    crew_id: Some(hold.crew_id),
                    since: format_unix(hold.held_at),
                    dismissible: dismissible(wire::NeedsYouKind::CommandApproval),
                    reason: fixed_reason(wire::NeedsYouKind::CommandApproval).map(str::to_owned),
                    name: None,
                    engine: None,
                    model: None,
                    task_kind: None,
                    question: None,
                    question_hidden: false,
                });
            }
        }

        // A merge the queue holds for the person before it lands (`waiting`), which is not a merge that already landed and
        // waits for review: the person's release is what moves it, and it is the queue's own row.
        let gate = self
            .store
            .crew_waiting_merge_entries(scope.crew(), scope.project())?;
        if !gate.is_empty() {
            let task_ids: Vec<String> = gate.iter().map(|entry| entry.task_id.clone()).collect();
            let projects = self.store.crew_projects_of_tasks(&task_ids)?;
            for entry in gate {
                let project = projects.get(&entry.task_id).cloned();
                if !scope.keeps(project.as_deref(), &entry.crew_id)
                    || !seen.insert(entry.task_id.clone())
                {
                    continue;
                }
                items.push(wire::NeedsYouItem {
                    kind: wire::NeedsYouKind::MergeGateWaiting,
                    id: entry.task_id,
                    project_id: project,
                    crew_id: Some(entry.crew_id),
                    since: format_unix(entry.queued_unix),
                    dismissible: dismissible(wire::NeedsYouKind::MergeGateWaiting),
                    reason: fixed_reason(wire::NeedsYouKind::MergeGateWaiting).map(str::to_owned),
                    name: None,
                    engine: None,
                    model: None,
                    task_kind: None,
                    question: None,
                    question_hidden: false,
                });
            }
        }

        for review in self
            .store
            .background_reviews(scope.crew(), scope.project())?
        {
            if !review.run.review_waiting
                || !scope.keeps(Some(&review.project_id), &review.run.background_id)
                || !seen.insert(review.run.id.clone())
            {
                continue;
            }
            let since = review
                .run
                .ended_at
                .clone()
                .unwrap_or_else(|| review.run.updated_at.clone());
            items.push(wire::NeedsYouItem {
                id: review.run.id,
                kind: wire::NeedsYouKind::BackgroundReview,
                project_id: Some(review.project_id),
                crew_id: Some(review.run.background_id),
                since,
                dismissible: false,
                reason: fixed_reason(wire::NeedsYouKind::BackgroundReview).map(str::to_owned),
                name: None,
                engine: None,
                model: None,
                task_kind: None,
                question: None,
                question_hidden: false,
            });
        }

        // A watch beside them: one that turned failing and asked for the person, and one whose last three checks in a row could
        // not run. Both resolve themselves on a check that runs again, so neither is dismissible.
        for watch in self.store.watch_raised(scope.crew(), scope.project())? {
            if !scope.keeps(Some(&watch.project_id), &watch.crew_id)
                || !seen.insert(watch.crew_id.clone())
            {
                continue;
            }
            // The could-not-run kind is the raised one only once the streak reaches three; before that a kept failing row is
            // still the failing kind.
            let could_not_run = watch.last_state == "could_not_run" && watch.error_streak >= 3;
            let kind = if could_not_run {
                wire::NeedsYouKind::WatchCouldNotRun
            } else {
                wire::NeedsYouKind::WatchFailing
            };
            let reason = if could_not_run {
                let why = watch
                    .last_reason
                    .as_deref()
                    .and_then(crate::crew::watch::CouldNotRun::of_word)
                    .map_or("could not run", crate::crew::watch::CouldNotRun::sentence);
                line(format!(
                    "{} checks in a row could not run: {why}.",
                    watch.error_streak
                ))
            } else {
                fixed_reason(kind).map(str::to_owned)
            };
            // A could-not-run row began with its latest check; a failing row began with the change that raised it.
            let at = if could_not_run {
                watch.last_checked_unix.or(watch.change_at)
            } else {
                watch.change_at.or(watch.last_checked_unix)
            }
            .unwrap_or_default();
            items.push(wire::NeedsYouItem {
                id: watch.crew_id.clone(),
                kind,
                project_id: Some(watch.project_id),
                crew_id: Some(watch.crew_id),
                since: format_unix(at),
                dismissible: false,
                reason,
                name: None,
                engine: None,
                model: None,
                task_kind: None,
                question: None,
                question_hidden: false,
            });
        }

        let tripwire = self.store.crew_tasks_with_autopilot_event(
            "tripwire",
            scope.crew(),
            scope.project(),
        )?;
        for place in &stopped {
            if !tripwire.contains(&place.task.id) || !seen.insert(place.task.id.clone()) {
                continue;
            }
            items.push(wire::NeedsYouItem {
                id: place.task.id.clone(),
                kind: wire::NeedsYouKind::TripwireStop,
                project_id: place.project_id.clone(),
                crew_id: Some(place.task.crew_id.clone()),
                since: place.task.updated_at.clone(),
                dismissible: true,
                reason: fixed_reason(wire::NeedsYouKind::TripwireStop).map(str::to_owned),
                name: None,
                engine: None,
                model: None,
                task_kind: None,
                question: None,
                question_hidden: false,
            });
        }
        for place in &stopped {
            // A second death keeps `stop_reason = 'engine_error'` for every old reader; the typed detail names it.
            let stopped_twice = place.task.stop_reason.as_deref() == Some("engine_error")
                && place.task.stop_detail.as_deref() == Some(WORKER_STOPPED_TWICE);
            if !stopped_twice || !seen.insert(place.task.id.clone()) {
                continue;
            }
            items.push(wire::NeedsYouItem {
                id: place.task.id.clone(),
                kind: wire::NeedsYouKind::WorkerStoppedTwice,
                project_id: place.project_id.clone(),
                crew_id: Some(place.task.crew_id.clone()),
                since: place.task.updated_at.clone(),
                dismissible: true,
                reason: fixed_reason(wire::NeedsYouKind::WorkerStoppedTwice).map(str::to_owned),
                name: None,
                engine: None,
                model: None,
                task_kind: None,
                question: None,
                question_hidden: false,
            });
        }

        Ok(items)
    }

    /// Every wait a person has not dismissed, newest first, ties by id, with no cap. The reads that scope the field to one
    /// crew or one repository (the Taskboard, the dashboard) take this, so another crew's newer waits can never push this
    /// one's out of its own count; a presentation cap belongs after that scoping. The Inbox and the Review queue read every
    /// crew (`Scope::Device`).
    pub(super) fn needs_you_all(&self, scope: Scope<'_>) -> Result<Vec<wire::NeedsYouItem>> {
        let mut items = self.needs_you_items(scope)?;
        let mut dismissed: BTreeMap<(String, String), String> = BTreeMap::new();
        for row in self.store.crew_needs_you_dismissals()? {
            dismissed.insert((row.kind, row.id), row.since);
        }
        items.retain(|item| {
            let key = (wire_name(&item.kind), item.id.clone());
            !dismissed
                .get(&key)
                .is_some_and(|since| !later(&item.since, since))
        });
        items.sort_by(|a, b| {
            let at = parse_time(&b.since)
                .unwrap_or(i64::MIN)
                .cmp(&parse_time(&a.since).unwrap_or(i64::MIN));
            at.then_with(|| a.id.cmp(&b.id))
        });
        Ok(items)
    }

    /// The items the Inbox shows: `needs_you_all` for the whole device, at most 200.
    fn needs_you_list(&self) -> Result<Vec<wire::NeedsYouItem>> {
        let mut items = self.needs_you_all(Scope::Device)?;
        items.truncate(CAP);
        Ok(items)
    }

    /// Everything that waits for the person, across every crew: newest first, ties by id, at most 200.
    pub fn crew_needs_you(&self) -> Result<wire::NeedsYouList> {
        self.crew_preview_only()?;
        let items = self.needs_you_list()?;
        Ok(wire::NeedsYouList {
            count: u32::try_from(items.len()).unwrap_or(u32::MAX),
            items,
        })
    }

    /// The person hides one dismissible item on this device. Only a dismissible kind that is currently an item of that kind is
    /// accepted; the audit row and the dismissal are written together, and a repeat at the same wait writes nothing. The item's
    /// state is never touched.
    pub fn crew_needs_you_dismiss(
        &self,
        kind: wire::NeedsYouKind,
        id: &str,
        actor: &str,
    ) -> Result<wire::NeedsYouDismissed> {
        self.crew_preview_only()?;
        if !dismissible(kind) {
            return Err(CoreError::Invalid(
                "this kind of item cannot be dismissed".into(),
            ));
        }
        valid_id(id)?;
        let item = self
            .needs_you_items(Scope::Device)?
            .into_iter()
            .find(|item| item.kind == kind && item.id == id)
            .ok_or_else(|| CoreError::NotFound(format!("needs you item {id}")))?;
        self.store
            .dismiss_crew_needs_you(&wire_name(&kind), id, &item.since, actor)?;
        Ok(wire::NeedsYouDismissed { dismissed: true })
    }

    /// A window asks to announce items with a desktop notification (KC10). Only current, undismissed items are considered, at
    /// most 50 refs are read, and an item is handed out once per wait: the first window to ask for it gets it and every later one
    /// (another window, a restart) gets nothing, until the item waits again with a later `since`. The answer keeps the order asked.
    /// Nothing behind an item is touched.
    pub fn crew_needs_you_notify(
        &self,
        items: &[wire::NeedsYouRef],
    ) -> Result<wire::NeedsYouNotified> {
        self.crew_preview_only()?;
        if items.len() > NOTIFY_MAX {
            return Err(CoreError::Invalid(format!(
                "at most {NOTIFY_MAX} items can be announced at once"
            )));
        }
        for item in items {
            valid_id(&item.id)?;
        }
        let current = self.needs_you_list()?;
        let mut claimed = Vec::new();
        for asked in items {
            let Some(item) = current
                .iter()
                .find(|item| item.kind == asked.kind && item.id == asked.id)
            else {
                continue;
            };
            if self.store.claim_crew_needs_you_notice(
                &wire_name(&item.kind),
                &item.id,
                &item.since,
            )? {
                claimed.push(asked.clone());
            }
        }
        Ok(wire::NeedsYouNotified { claimed })
    }
}
