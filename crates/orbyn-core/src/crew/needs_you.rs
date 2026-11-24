//! The Crew "needs you" inbox (Crew Preview, KC3 PR1): one read, across every crew, of what waits for the person, and the
//! person's device-local dismissal of the stops they do not want to see again. It adds no state: every item is a view of what
//! Crew already stores (a task's state, a merge's `needs_person`, a background run's wait, the Autopilot ledger, a manual
//! check). Nothing here moves a task, answers a merge or settles a run; the dismissal is a row of its own and touches nothing
//! behind the item.
//!
//! A task appears once. The first rule that matches wins, in this order: a task whose last cancel attempt could not stop its
//! worker (its worker may still run, so it comes before the same task waiting on a person), then a task waiting on a person, then a merge waiting on the person, then a background run waiting for review, then
//! a stopped task with a trip wire, then a stopped task whose worker stopped twice. Other stopped and cancelled tasks are not
//! items. Every text is one line, redacted and capped at 160 characters; only a task that waits on a person carries its own
//! scrubbed reason, every other kind carries fixed core words.

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

/// The stop reason a worker that stopped twice writes. Another engineer writes the row; this module only reads it.
pub const WORKER_STOPPED_TWICE: &str = "worker_stopped_twice";

/// The most items one read answers with.
const CAP: usize = 200;
/// The longest reason, in characters.
const REASON_MAX: usize = 160;

/// The fixed reason of a kind. A task waiting on a person has its own scrubbed reason instead.
fn fixed_reason(kind: wire::NeedsYouKind) -> Option<&'static str> {
    match kind {
        wire::NeedsYouKind::TaskNeedsPerson => None,
        wire::NeedsYouKind::CouldNotStart => Some("The worker never ran."),
        wire::NeedsYouKind::ManualCheck => Some("A check only a person can make is waiting."),
        wire::NeedsYouKind::MergeWaiting => Some("A merge needs your decision."),
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
    /// The scrubbed reason of a task that waits on a person: the latest `needs_person` result, as the Taskboard reads it.
    fn needs_you_reason(&self, facts: &Facts, scrub: &Scrub) -> Result<Option<String>> {
        Ok(self
            .store
            .crew_results(&facts.task.id)?
            .into_iter()
            .rev()
            .find(|result| result.state == ResultState::NeedsPerson)
            .and_then(|result| line(scrub.text(&result.summary, REASON_MAX))))
    }

    /// Every item, dismissed or not, before the cap and the order. The first rule for an id wins.
    fn needs_you_items(&self) -> Result<Vec<wire::NeedsYouItem>> {
        let person = self.display_redactor()?;
        let mut scrubs: BTreeMap<String, Scrub> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        let mut items = Vec::new();
        let mut stopped = Vec::new();

        for waiting in self.store.crew_could_not_stop()? {
            if !seen.insert(waiting.task.id.clone()) {
                continue;
            }
            let kind = wire::NeedsYouKind::CouldNotStop;
            items.push(wire::NeedsYouItem {
                id: waiting.task.id.clone(),
                kind,
                project_id: waiting.project_id,
                crew_id: Some(waiting.task.crew_id),
                since: waiting.since,
                dismissible: dismissible(kind),
                reason: fixed_reason(kind).map(str::to_owned),
            });
        }

        for place in self
            .store
            .crew_tasks_in_states(&["needs_person", "stopped"])?
        {
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
            let reason = if kind == wire::NeedsYouKind::TaskNeedsPerson {
                let scrub = match scrubs.entry(place.task.crew_id.clone()) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        entry.insert(Scrub::of_crew(self, &person, &place.task.crew_id)?)
                    }
                };
                self.needs_you_reason(&facts, scrub)?
            } else {
                fixed_reason(kind).map(str::to_owned)
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
                });
            }
        }

        let merges: Vec<_> = self
            .store
            .crew_all_merges()?
            .into_iter()
            .filter(|merge| merge.review_state == "waiting" && merge.needs_person.is_some())
            .collect();
        let task_ids: Vec<String> = merges.iter().map(|merge| merge.task_id.clone()).collect();
        let projects = self.store.crew_projects_of_tasks(&task_ids)?;
        for merge in merges {
            if !seen.insert(merge.task_id.clone()) {
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
            });
        }

        for review in self.store.background_reviews()? {
            if !review.run.review_waiting || !seen.insert(review.run.id.clone()) {
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
            });
        }

        // A watch beside them: one that turned failing and asked for the person, and one whose last three checks in a row could
        // not run. Both resolve themselves on a check that runs again, so neither is dismissible.
        for watch in self.store.watch_raised()? {
            if !seen.insert(watch.crew_id.clone()) {
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
            });
        }

        let tripwire = self.store.crew_tasks_with_autopilot_event("tripwire")?;
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
            });
        }
        for place in &stopped {
            if place.task.stop_reason.as_deref() != Some(WORKER_STOPPED_TWICE)
                || !seen.insert(place.task.id.clone())
            {
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
            });
        }

        Ok(items)
    }

    /// The items a person sees: dismissed waits removed, newest first, ties by id, at most 200.
    fn needs_you_list(&self) -> Result<Vec<wire::NeedsYouItem>> {
        let mut items = self.needs_you_items()?;
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
            .needs_you_items()?
            .into_iter()
            .find(|item| item.kind == kind && item.id == id)
            .ok_or_else(|| CoreError::NotFound(format!("needs you item {id}")))?;
        self.store
            .dismiss_crew_needs_you(&wire_name(&kind), id, &item.since, actor)?;
        Ok(wire::NeedsYouDismissed { dismissed: true })
    }
}
