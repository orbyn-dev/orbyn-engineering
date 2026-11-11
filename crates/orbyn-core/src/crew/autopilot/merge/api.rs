//! What the HTTP routes read and the answers they relay (docs/crew.md, "Autopilot"): views of the modes, trust, risk, the queues,
//! the watch, the digest, the ledger and the guards, built from what the core stores. Nothing here decides anything. A read builds
//! a contract type; an answer calls the function that already enforces the rules (`crew_answer_review`,
//! `set_crew_autopilot_mode`) after checking the shape of the request. With the Crew Preview flag off every function answers
//! "not found", as if there were no such thing.
//!
//! Nothing a view returns holds an answer key or a secret (H1, H2): each free-text field and each stored JSON value is
//! redacted and has the key's text withheld, and a value that still fails the scan is replaced.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use orbyn_contracts as wire;
use serde_json::Value;

use super::queue::MergeTarget;
use super::review_queue::ReviewAnswer;
use super::risk::{
    HIGH_REVERT_PERMILLE, LARGE_FILES, LARGE_LINES, MIN_SAMPLE, Rate, Risk, WINDOW_DAYS,
};
use crate::app::Core;
use crate::crew::autopilot::ledger;
use crate::crew::autopilot::{IN_A_ROW, IN_A_RUN, Mode, ModeScope, RULES, review_window};
use crate::crew::{key_needles, redact, withhold};
use crate::error::{CoreError, Result};
use crate::store::{AutopilotEvent, EVENT_KINDS, MergeEntry, MergeRecord};

/// The most a page holds, and what it holds when the caller says nothing.
pub const PAGE_MAX: u32 = 200;
pub const PAGE_DEFAULT: u32 = 50;
/// How far into a list a caller may page.
pub const OFFSET_MAX: u32 = 100_000;
/// The longest comment on a merge, in characters.
pub const COMMENT_MAX: usize = 2000;
/// The widest window of the digest, in hours.
pub const DIGEST_HOURS_MAX: u32 = 720;
const MODES_MAX: u32 = 500;
const ROWS_MAX: usize = 100;
const RECENT: u32 = 20;
const ACTIVE_MAX: u32 = 1000;
const LATER_MAX: usize = 50;
const ACTIVE: [&str; 4] = ["queued", "blocked", "waiting", "merging"];

/// One page of a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub limit: u32,
    pub offset: u32,
}

impl Page {
    /// A limit of 1 to 200 (50 when absent) and an offset up to 100 000; anything else is refused.
    pub fn new(limit: Option<u32>, offset: Option<u32>) -> Result<Self> {
        let limit = limit.unwrap_or(PAGE_DEFAULT);
        let offset = offset.unwrap_or(0);
        if !(1..=PAGE_MAX).contains(&limit) {
            return Err(CoreError::Invalid(format!("limit is 1 to {PAGE_MAX} rows")));
        }
        if offset > OFFSET_MAX {
            return Err(CoreError::Invalid("offset is too far".into()));
        }
        Ok(Self { limit, offset })
    }

    fn next(self, total: usize) -> Option<u32> {
        let next = u64::from(self.offset) + u64::from(self.limit);
        u32::try_from(next)
            .ok()
            .filter(|n| usize::try_from(*n).is_ok_and(|n| n < total))
    }
}

/// An id as the routes take it: plain characters and a sane length, never a path.
pub fn valid_id(id: &str) -> Result<()> {
    let fine = !id.is_empty()
        && id.len() <= 96
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '-' | '.'))
        && !id.starts_with('.');
    if fine {
        Ok(())
    } else {
        Err(CoreError::Invalid("that id is not valid".into()))
    }
}

/// A commit as a client names it: 7 to 64 hex digits.
fn valid_commit(commit: &str) -> Result<()> {
    if (7..=64).contains(&commit.len()) && commit.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(CoreError::Invalid("that commit is not valid".into()))
    }
}

fn mode_wire(mode: Mode) -> wire::AutopilotMode {
    match mode {
        Mode::Off => wire::AutopilotMode::Off,
        Mode::On => wire::AutopilotMode::On,
        Mode::Full => wire::AutopilotMode::Full,
    }
}

fn mode_core(mode: wire::AutopilotMode) -> Mode {
    match mode {
        wire::AutopilotMode::Off => Mode::Off,
        wire::AutopilotMode::On => Mode::On,
        wire::AutopilotMode::Full => Mode::Full,
    }
}

fn risk_wire(text: &str) -> wire::AutopilotRisk {
    match Risk::parse(text).unwrap_or(Risk::High) {
        Risk::Low => wire::AutopilotRisk::Low,
        Risk::Medium => wire::AutopilotRisk::Medium,
        Risk::High => wire::AutopilotRisk::High,
    }
}

fn rate_wire(rate: Rate) -> wire::AutopilotRate {
    wire::AutopilotRate {
        merges: rate.merges,
        reverts: rate.reverts,
        revert_permille: if rate.merges == 0 {
            0
        } else {
            u32::try_from(u64::from(rate.reverts) * 1000 / u64::from(rate.merges))
                .unwrap_or(u32::MAX)
        },
        counts: rate.merges >= MIN_SAMPLE,
    }
}

fn cap<T>(mut rows: Vec<T>, max: usize) -> (Vec<T>, bool) {
    let over = rows.len() > max;
    rows.truncate(max);
    (rows, over)
}

fn slice<T: Clone>(rows: &[T], page: Page) -> Vec<T> {
    rows.iter()
        .skip(page.offset as usize)
        .take(page.limit as usize)
        .cloned()
        .collect()
}

fn to_u32(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// Cleans text and stored JSON for one task: redacted, the task's key withheld, and scanned.
struct Scrub {
    needles: Vec<String>,
}

impl Scrub {
    fn text(&self, text: &str) -> String {
        ledger::shown(text, &self.needles)
    }

    fn texts(&self, items: &[String]) -> Vec<String> {
        items.iter().map(|t| self.text(t)).collect()
    }

    fn json(&self, value: &Value) -> Value {
        ledger::safe(value, &self.needles)
    }
}

/// The scrubs of the tasks one call touches, read once each.
struct Scrubs<'a> {
    core: &'a Core,
    seen: HashMap<String, Arc<Scrub>>,
}

impl<'a> Scrubs<'a> {
    fn new(core: &'a Core) -> Self {
        Self {
            core,
            seen: HashMap::new(),
        }
    }

    fn of(&mut self, task_id: &str) -> Arc<Scrub> {
        let core = self.core;
        Arc::clone(self.seen.entry(task_id.to_owned()).or_insert_with(|| {
            let needles = core
                .store
                .crew_answer_key(task_id)
                .ok()
                .flatten()
                .map(|key| key_needles(&key))
                .unwrap_or_default();
            Arc::new(Scrub { needles })
        }))
    }
}

fn str_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn answer_of(value: &Value) -> Option<wire::AutopilotAnswer> {
    Some(wire::AutopilotAnswer {
        approve: value.get("approve")?.as_bool()?,
        reason: str_of(value, "reason").unwrap_or_default(),
    })
}

fn log_row(event: &AutopilotEvent, scrub: &Scrub) -> wire::AutopilotLogRow {
    let body = scrub.json(&event.body);
    let action = body.get("action").cloned().unwrap_or(Value::Null);
    let second = body.get("second").and_then(|s| {
        let answer = answer_of(s.get("answer")?)?;
        Some(wire::AutopilotSecondKey {
            engine: str_of(s, "engine")?,
            approve: answer.approve,
            reason: answer.reason,
        })
    });
    wire::AutopilotLogRow {
        id: event.id,
        task_id: event.task_id.clone(),
        crew_id: event.crew_id.clone(),
        kind: event.kind.clone(),
        at: event.at.clone(),
        decided_by: str_of(&body, "decided_by"),
        tier: str_of(&body, "tier"),
        rule: str_of(&body, "rule"),
        mode: str_of(&body, "mode"),
        action_kind: str_of(&action, "kind"),
        target: str_of(&action, "target"),
        paths: action
            .get("paths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(str::to_owned))
            .take(20)
            .collect(),
        lead: body.get("lead").and_then(answer_of),
        second,
        needs_second_engine: body["needs_second_engine"].as_bool() == Some(true),
        body,
    }
}

/// How an answer to a merge in the Review queue is shaped. The core's own rules decide the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answering {
    LooksGood,
    Comment,
    Revert,
}

impl Core {
    /// With the Crew Preview flag off, nothing here exists.
    fn crew_preview_only(&self) -> Result<()> {
        if self.config.crew_preview {
            Ok(())
        } else {
            Err(CoreError::NotFound("route".into()))
        }
    }

    /// The Preview flag as a route reads it: on, or not found.
    pub fn crew_preview_state(&self) -> Result<wire::CrewPreviewState> {
        self.crew_preview_only()?;
        Ok(wire::CrewPreviewState { enabled: true })
    }

    fn saved_default(&self) -> Option<Mode> {
        self.store
            .crew_autopilot_mode_of("default")
            .ok()
            .flatten()
            .and_then(|text| Mode::parse(&text))
    }

    /// The modes the person has saved.
    pub fn autopilot_modes(&self) -> Result<wire::AutopilotModes> {
        self.crew_preview_only()?;
        let (rows, truncated) = cap(
            self.store.crew_autopilot_modes_all(MODES_MAX + 1)?,
            MODES_MAX as usize,
        );
        let entries = rows
            .into_iter()
            .filter_map(|(scope, mode, updated_at)| {
                let mode = mode_wire(Mode::parse(&mode)?);
                let (scope, id) = match scope.split_once(':') {
                    Some(("repo", id)) => (wire::AutopilotScopeKind::Repo, Some(id.to_owned())),
                    Some(("crew", id)) => (wire::AutopilotScopeKind::Crew, Some(id.to_owned())),
                    None if scope == "default" => (wire::AutopilotScopeKind::Default, None),
                    _ => return None,
                };
                Some(wire::AutopilotModeEntry {
                    scope,
                    id,
                    mode,
                    updated_at,
                })
            })
            .collect();
        Ok(wire::AutopilotModes {
            confirmed: self.saved_default().is_some(),
            effective_default: mode_wire(self.saved_default().unwrap_or_default()),
            entries,
            truncated,
        })
    }

    /// Saves the person's mode for a scope, through the one setter the core has. A repository must be a project and a crew must
    /// have a task, so a mode cannot be saved for a name that is nothing.
    pub fn autopilot_set_mode(&self, change: &wire::AutopilotModeChange) -> Result<()> {
        self.crew_preview_only()?;
        let scope = match (change.scope, change.id.as_deref()) {
            (wire::AutopilotScopeKind::Default, None) => ModeScope::Default,
            (wire::AutopilotScopeKind::Default, Some(_)) => {
                return Err(CoreError::Invalid("the default mode has no id".into()));
            }
            (wire::AutopilotScopeKind::Repo, Some(id)) => {
                valid_id(id)?;
                self.store.project(id)?;
                ModeScope::Repo(id.to_owned())
            }
            (wire::AutopilotScopeKind::Crew, Some(id)) => {
                valid_id(id)?;
                if self.store.crew_tasks_in_crew(id)?.is_empty() {
                    return Err(CoreError::NotFound("crew".into()));
                }
                ModeScope::Crew(id.to_owned())
            }
            _ => return Err(CoreError::Invalid("name the repository or crew".into())),
        };
        self.set_crew_autopilot_mode(&scope, mode_core(change.mode))
    }

    /// The person keeps the default as it is: it is saved, so the first-run banner is done. It changes no mode.
    pub fn autopilot_confirm(&self) -> Result<()> {
        self.crew_preview_only()?;
        let mode = self.saved_default().unwrap_or_default();
        self.set_crew_autopilot_mode(&ModeScope::Default, mode)
    }

    /// Revert rates by area and engine over the window the risk score reads.
    pub fn autopilot_trust(&self, now: i64) -> Result<wire::AutopilotTrust> {
        self.crew_preview_only()?;
        let since = now - WINDOW_DAYS * 86_400;
        let mut total = Rate::default();
        let mut areas: BTreeMap<String, Rate> = BTreeMap::new();
        let mut engines: BTreeMap<String, Rate> = BTreeMap::new();
        for merge in self
            .store
            .crew_all_merges()?
            .iter()
            .filter(|m| m.merged_unix >= since)
        {
            let reverted = u32::from(merge.review_state == "reverted");
            let bump = |rate: &mut Rate| {
                rate.merges += 1;
                rate.reverts += reverted;
            };
            bump(&mut total);
            for area in &merge.areas {
                bump(areas.entry(area.clone()).or_default());
            }
            bump(engines.entry(merge.engine.clone()).or_default());
        }
        let rows = |map: BTreeMap<String, Rate>| {
            let mut rows: Vec<wire::AutopilotRateRow> = map
                .into_iter()
                .map(|(name, rate)| wire::AutopilotRateRow {
                    name: redact(&name),
                    rate: rate_wire(rate),
                })
                .collect();
            rows.sort_by(|a, b| b.rate.merges.cmp(&a.rate.merges).then(a.name.cmp(&b.name)));
            cap(rows, ROWS_MAX)
        };
        let (areas, more_areas) = rows(areas);
        let (engines, more_engines) = rows(engines);
        Ok(wire::AutopilotTrust {
            window_days: u32::try_from(WINDOW_DAYS).unwrap_or(60),
            min_sample: MIN_SAMPLE,
            high_revert_permille: HIGH_REVERT_PERMILLE,
            total: rate_wire(total),
            areas,
            engines,
            autonomy_by_area: false,
            truncated: more_areas || more_engines,
        })
    }

    /// Why a task's merge has its risk. Reads the queue row, else the merge that reached the branch.
    pub fn autopilot_risk(&self, task_id: &str) -> Result<wire::AutopilotRiskReport> {
        self.crew_preview_only()?;
        valid_id(task_id)?;
        let mut scrubs = Scrubs::new(self);
        let scrub = scrubs.of(task_id);
        let not_measured = vec!["test coverage of the touched code".to_owned()];
        if let Some(entry) = self.store.merge_entry(task_id)? {
            return Ok(wire::AutopilotRiskReport {
                task_id: entry.task_id.clone(),
                state: entry.state.clone(),
                risk: risk_wire(&entry.risk),
                reasons: scrub.texts(&strings(&entry.reasons)),
                samples: scrub.texts(&entry.samples),
                areas: scrub.texts(&entry.areas),
                engine: scrub.text(&entry.engine),
                files: entry.files,
                lines: entry.lines,
                mode: entry.mode.clone(),
                note: entry.note.as_deref().map(|n| scrub.text(n)),
                not_measured,
            });
        }
        let record = self
            .store
            .crew_merge(task_id)?
            .ok_or_else(|| CoreError::NotFound("merge".into()))?;
        Ok(wire::AutopilotRiskReport {
            task_id: record.task_id.clone(),
            state: "merged".to_owned(),
            risk: risk_wire(&record.risk),
            reasons: scrub.texts(&strings(&record.reasons)),
            samples: scrub.texts(&strings(&record.proof["risk_samples"])),
            areas: scrub.texts(&record.areas),
            engine: scrub.text(&record.engine),
            files: record.files,
            lines: record.lines,
            mode: record.mode.clone(),
            note: None,
            not_measured,
        })
    }

    /// The after-merge Review queue, most urgent first, one page.
    pub fn autopilot_review_queue(
        &self,
        now: i64,
        page: Page,
    ) -> Result<wire::AutopilotReviewQueue> {
        self.crew_preview_only()?;
        let all = self.crew_review_queue(now)?;
        let mut scrubs = Scrubs::new(self);
        let items = slice(&all, page)
            .into_iter()
            .map(|item| {
                let scrub = scrubs.of(&item.task_id);
                let watch = self
                    .store
                    .crew_merge(&item.task_id)
                    .ok()
                    .flatten()
                    .map_or_else(|| "unknown".to_owned(), |m| m.watch);
                wire::AutopilotReviewItem {
                    risk: risk_wire(item.risk.name()),
                    reasons: scrub.texts(&item.reasons),
                    merge_commit: item.merge_commit,
                    before_commit: item.before_commit,
                    candidate: item.candidate,
                    files: item.files,
                    lines: item.lines,
                    merged_unix: item.merged_unix,
                    due_unix: item.due_unix,
                    overdue: item.overdue,
                    proof: scrub.json(&item.proof),
                    not_verified: scrub.texts(&item.not_verified),
                    cost: scrub.json(&item.cost),
                    watch,
                    needs_person: item.needs_person.as_deref().map(|n| scrub.text(n)),
                    task_id: item.task_id,
                }
            })
            .collect();
        Ok(wire::AutopilotReviewQueue {
            items,
            total: to_u32(all.len()),
            next_offset: page.next(all.len()),
            now_unix: now,
        })
    }

    /// The merges after this one on its branch, which a revert has to leave alone or ask about. A read: the check that decides is
    /// the revert's own.
    pub fn autopilot_revert_check(&self, task_id: &str) -> Result<wire::AutopilotRevertCheck> {
        self.crew_preview_only()?;
        valid_id(task_id)?;
        let record = self
            .store
            .crew_merge(task_id)?
            .ok_or_else(|| CoreError::NotFound("merge".into()))?;
        let later: Vec<wire::AutopilotLaterMerge> = self
            .store
            .crew_merges(&record.repo, &record.branch)?
            .into_iter()
            .filter(|m| m.id > record.id && m.review_state != "reverted")
            .map(|m| wire::AutopilotLaterMerge {
                risk: risk_wire(&m.risk),
                task_id: m.task_id,
                merge_commit: m.merge_commit,
            })
            .collect();
        let (later_merges, truncated) = cap(later, LATER_MAX);
        Ok(wire::AutopilotRevertCheck {
            task_id: record.task_id,
            merge_commit: record.merge_commit,
            already_reverted: record.review_state == "reverted",
            later_merges,
            truncated,
        })
    }

    /// A person's answer to a merge. The merge's repository and branch come from the stored merge, never from the request, and the
    /// request must name the merge commit it was shown. The rules (a revert is made and checked in a scratch checkout and refused
    /// when a later merge needs it, a branch that is checked out is never moved) are `crew_answer_review`'s.
    pub async fn autopilot_answer(
        self: &Arc<Self>,
        task_id: &str,
        body: &wire::AutopilotAnswerBody,
        answering: Answering,
        now: i64,
    ) -> Result<wire::AutopilotAnswered> {
        self.crew_preview_only()?;
        valid_id(task_id)?;
        valid_commit(&body.merge_commit)?;
        let record = self
            .store
            .crew_merge(task_id)?
            .ok_or_else(|| CoreError::NotFound("merge".into()))?;
        if !record
            .merge_commit
            .starts_with(&body.merge_commit.to_ascii_lowercase())
        {
            return Err(CoreError::Invalid(
                "This merge changed since it was shown. Reload the Review queue.".into(),
            ));
        }
        let answer = match answering {
            Answering::LooksGood => ReviewAnswer::LooksGood,
            Answering::Revert => ReviewAnswer::Revert,
            Answering::Comment => {
                let text = body.text.as_deref().map(str::trim).unwrap_or_default();
                if text.is_empty() || text.chars().count() > COMMENT_MAX {
                    return Err(CoreError::Invalid(format!(
                        "A comment is 1 to {COMMENT_MAX} characters."
                    )));
                }
                let needles = Scrubs::new(self).of(task_id).needles.clone();
                ReviewAnswer::Comment(withhold(&redact(text), &needles))
            }
        };
        let target = MergeTarget {
            repo: PathBuf::from(&record.repo),
            branch: record.branch.clone(),
            owed: None,
        };
        let commit = self
            .crew_answer_review(task_id, answer, &target, now)
            .await?;
        Ok(wire::AutopilotAnswered {
            task_id: task_id.to_owned(),
            review_state: match answering {
                Answering::LooksGood => "looks_good",
                Answering::Comment => "commented",
                Answering::Revert => "reverted",
            }
            .to_owned(),
            revert_commit: commit,
        })
    }

    /// What waits to merge, in the order queued, one page, with what the queue will do next.
    pub fn autopilot_merge_queue(&self, page: Page) -> Result<wire::AutopilotMergeQueue> {
        self.crew_preview_only()?;
        let (all, _) = self.store.merge_entries_page(&ACTIVE, ACTIVE_MAX, 0)?;
        let mut scrubs = Scrubs::new(self);
        let mut ahead: HashMap<(String, String), u32> = HashMap::new();
        let mut rows = Vec::with_capacity(all.len());
        for entry in &all {
            let key = (entry.repo.clone(), entry.branch.clone());
            let before = ahead.get(&key).copied().unwrap_or(0);
            if matches!(entry.state.as_str(), "queued" | "blocked") {
                *ahead.entry(key).or_default() += 1;
            }
            rows.push(queue_entry(entry, before, &scrubs.of(&entry.task_id)));
        }
        let count = |state: &str| to_u32(all.iter().filter(|e| e.state == state).count());
        let forecast = wire::AutopilotForecast {
            next_task_id: all
                .iter()
                .find(|e| e.state == "queued")
                .map(|e| e.task_id.clone()),
            queued: count("queued"),
            blocked: count("blocked"),
            waiting_for_person: count("waiting"),
            conflict_forecast: false,
        };
        Ok(wire::AutopilotMergeQueue {
            entries: slice(&rows, page),
            total: to_u32(rows.len()),
            next_offset: page.next(rows.len()),
            forecast,
        })
    }

    /// The watch on main: what it found for each merge, newest first, and the latest breaks and reverts.
    pub fn autopilot_watch(&self, page: Page) -> Result<wire::AutopilotWatch> {
        self.crew_preview_only()?;
        let (records, total) = self.store.crew_merges_page(page.limit, page.offset)?;
        let mut scrubs = Scrubs::new(self);
        let rows = records
            .iter()
            .map(|m| watch_row(m, &scrubs.of(&m.task_id)))
            .collect();
        let counts = self.store.crew_watch_counts()?;
        let of = |watch: &str| {
            counts
                .iter()
                .find(|(w, _)| w == watch)
                .map_or(0, |(_, n)| *n)
        };
        let recent = self
            .log_page(None, &["main_broken", "reverted", "person_told"], 0, RECENT)?
            .0;
        Ok(wire::AutopilotWatch {
            rows,
            total,
            next_offset: page.next(total as usize),
            broken: of("broken"),
            unwatched: of("unknown"),
            recent,
            bisect_progress: false,
        })
    }

    fn log_page(
        &self,
        task_id: Option<&str>,
        kinds: &[&str],
        offset: u32,
        limit: u32,
    ) -> Result<(Vec<wire::AutopilotLogRow>, u32)> {
        let (events, total) = self
            .store
            .crew_autopilot_events_page(task_id, kinds, limit, offset)?;
        let mut scrubs = Scrubs::new(self);
        let rows = events
            .iter()
            .map(|e| log_row(e, &scrubs.of(&e.task_id)))
            .collect();
        Ok((rows, total))
    }

    /// The approval ledger, newest first, for a task or for all. `kind` limits it to one kind of row.
    pub fn autopilot_log(
        &self,
        task_id: Option<&str>,
        kind: Option<&str>,
        page: Page,
    ) -> Result<wire::AutopilotLog> {
        self.crew_preview_only()?;
        if let Some(task) = task_id {
            valid_id(task)?;
        }
        let kinds: Vec<&str> =
            match kind {
                None => Vec::new(),
                Some(kind) => vec![*EVENT_KINDS.iter().find(|k| **k == kind).ok_or_else(|| {
                    CoreError::Invalid("that is not a kind of ledger row".into())
                })?],
            };
        let (rows, total) = self.log_page(task_id, &kinds, page.offset, page.limit)?;
        Ok(wire::AutopilotLog {
            rows,
            total,
            next_offset: page.next(total as usize),
        })
    }

    /// How close each running task is to a trip wire, and the wires that fired lately.
    pub fn autopilot_tripwires(&self) -> Result<wire::AutopilotTripwires> {
        self.crew_preview_only()?;
        let mut running = Vec::new();
        for (task, _) in self
            .store
            .crew_tasks_in_flight()?
            .into_iter()
            .take(ROWS_MAX)
        {
            let (in_a_row, in_a_run) = self.store.crew_autopilot_denials(&task.id)?;
            running.push(wire::AutopilotTripwireTask {
                task_id: task.id,
                crew_id: task.crew_id,
                state: task.state,
                in_a_row,
                in_a_run,
            });
        }
        Ok(wire::AutopilotTripwires {
            in_a_row_limit: IN_A_ROW,
            in_a_run_limit: IN_A_RUN,
            running,
            recent: self.log_page(None, &["tripwire"], 0, RECENT)?.0,
        })
    }

    /// The limits Autopilot works within, fixed in code, and who answers for the lead and as a second key.
    pub fn autopilot_guards(&self) -> Result<wire::AutopilotGuards> {
        self.crew_preview_only()?;
        let approvers = self
            .crew_autopilot
            .approvers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let window = |risk: Risk| wire::AutopilotReviewWindow {
            risk: risk_wire(risk.name()),
            hours: review_window(risk).and_then(|s| u32::try_from(s / 3600).ok()),
        };
        Ok(wire::AutopilotGuards {
            always_denied: RULES
                .iter()
                .map(|(id, rule)| wire::AutopilotRule {
                    id: (*id).to_owned(),
                    rule: (*rule).to_owned(),
                })
                .collect(),
            in_a_row_limit: IN_A_ROW,
            in_a_run_limit: IN_A_RUN,
            review_windows: vec![window(Risk::High), window(Risk::Medium), window(Risk::Low)],
            large_files: LARGE_FILES,
            large_lines: LARGE_LINES,
            high_revert_permille: HIGH_REVERT_PERMILLE,
            min_sample: MIN_SAMPLE,
            window_days: u32::try_from(WINDOW_DAYS).unwrap_or(60),
            default_mode: mode_wire(self.saved_default().unwrap_or_default()),
            lead_engine: approvers.lead.as_ref().map(|a| redact(a.engine())),
            second_key_engines: approvers
                .others
                .iter()
                .take(10)
                .map(|a| redact(a.engine()))
                .collect(),
        })
    }

    /// What merged, what was reverted and what waits for review in the last `hours` (1 to 720; 24 when absent).
    pub fn autopilot_digest(&self, now: i64, hours: Option<u32>) -> Result<wire::AutopilotDigest> {
        self.crew_preview_only()?;
        let hours = hours.unwrap_or(24);
        if !(1..=DIGEST_HOURS_MAX).contains(&hours) {
            return Err(CoreError::Invalid(format!(
                "hours is 1 to {DIGEST_HOURS_MAX}"
            )));
        }
        let since = now - i64::from(hours) * 3600;
        let merges = self.store.crew_all_merges()?;
        let digest_merge = |m: &MergeRecord| wire::AutopilotDigestMerge {
            task_id: m.task_id.clone(),
            risk: risk_wire(&m.risk),
            merge_commit: m.merge_commit.clone(),
            merged_unix: m.merged_unix,
        };
        let mut merged: Vec<&MergeRecord> =
            merges.iter().filter(|m| m.merged_unix >= since).collect();
        merged.sort_by_key(|m| std::cmp::Reverse(m.merged_unix));
        let waiting: Vec<&MergeRecord> = merges
            .iter()
            .filter(|m| m.review_state == "waiting")
            .collect();
        let since_text = time::OffsetDateTime::from_unix_timestamp(since)
            .ok()
            .and_then(|t| {
                t.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            })
            .ok_or_else(|| CoreError::Invalid("that window is not valid".into()))?;
        let counts = self.store.crew_autopilot_counts_since(&since_text)?;
        let of = |kind: &str| {
            counts
                .iter()
                .find(|(k, _)| k == kind)
                .map_or(0, |(_, n)| *n)
        };
        let (reverted_events, _) =
            self.store
                .crew_autopilot_events_page(None, &["reverted"], to_u32(ROWS_MAX), 0)?;
        let reverted: Vec<wire::AutopilotDigestMerge> = reverted_events
            .iter()
            .filter(|e| crate::util::parse_time(&e.at).is_some_and(|t| t >= since))
            .filter_map(|e| merges.iter().find(|m| m.task_id == e.task_id))
            .map(digest_merge)
            .collect();
        let overdue = waiting
            .iter()
            .filter(|m| m.due_unix.is_some_and(|due| now > due))
            .count();
        let needs_person = merges
            .iter()
            .filter(|m| m.needs_person.is_some() && m.review_state != "reverted")
            .count();
        let mut digest = wire::AutopilotDigest {
            since_unix: since,
            until_unix: now,
            merged_count: to_u32(merged.len()),
            merged: merged
                .iter()
                .take(ROWS_MAX)
                .map(|m| digest_merge(m))
                .collect(),
            reverted_count: of("reverted"),
            reverted,
            waiting_review: to_u32(waiting.len()),
            overdue: to_u32(overdue),
            needs_person: to_u32(needs_person),
            denied: of("denied"),
            tripwires: of("tripwire"),
            quiet: false,
        };
        digest.quiet = digest.merged_count == 0
            && digest.reverted_count == 0
            && digest.waiting_review == 0
            && digest.needs_person == 0
            && digest.denied == 0
            && digest.tripwires == 0;
        Ok(digest)
    }
}

fn queue_entry(entry: &MergeEntry, ahead: u32, scrub: &Scrub) -> wire::AutopilotQueueEntry {
    wire::AutopilotQueueEntry {
        task_id: entry.task_id.clone(),
        crew_id: entry.crew_id.clone(),
        state: entry.state.clone(),
        risk: risk_wire(&entry.risk),
        reasons: scrub.texts(&strings(&entry.reasons)),
        areas: scrub.texts(&entry.areas),
        samples: scrub.texts(&entry.samples),
        engine: scrub.text(&entry.engine),
        files: entry.files,
        lines: entry.lines,
        mode: entry.mode.clone(),
        note: entry.note.as_deref().map(|n| scrub.text(n)),
        released: entry.released,
        queued_unix: entry.queued_unix,
        ahead,
    }
}

fn watch_row(record: &MergeRecord, scrub: &Scrub) -> wire::AutopilotWatchRow {
    wire::AutopilotWatchRow {
        task_id: record.task_id.clone(),
        merge_commit: record.merge_commit.clone(),
        risk: risk_wire(&record.risk),
        watch: record.watch.clone(),
        review_state: record.review_state.clone(),
        needs_person: record.needs_person.as_deref().map(|n| scrub.text(n)),
        reverted_commit: record.reverted_commit.clone(),
        merged_unix: record.merged_unix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_bounded() {
        assert_eq!(
            Page::new(None, None).unwrap(),
            Page {
                limit: 50,
                offset: 0
            }
        );
        assert!(Page::new(Some(0), None).is_err());
        assert!(Page::new(Some(201), None).is_err());
        assert!(Page::new(Some(200), Some(100_000)).is_ok());
        assert!(Page::new(None, Some(100_001)).is_err());
        let page = Page {
            limit: 2,
            offset: 0,
        };
        assert_eq!(page.next(5), Some(2));
        assert_eq!(page.next(2), None);
    }

    #[test]
    fn an_id_is_plain_characters_and_never_a_path() {
        for good in ["task-1", "sha256:abc123", "crew_1.x"] {
            assert!(valid_id(good).is_ok(), "{good}");
        }
        for bad in ["", "../x", "a/b", "a b", ".hidden", "a\\b", &"x".repeat(97)] {
            assert!(valid_id(bad).is_err(), "{bad}");
        }
        assert!(valid_commit("abc1234").is_ok());
        assert!(valid_commit("abc").is_err());
        assert!(valid_commit("xyz1234").is_err());
    }

    #[test]
    fn a_rate_counts_from_the_minimum_sample() {
        let few = rate_wire(Rate {
            merges: 9,
            reverts: 9,
        });
        assert!(!few.counts);
        let many = rate_wire(Rate {
            merges: 10,
            reverts: 3,
        });
        assert!(many.counts);
        assert_eq!(many.revert_permille, 300);
        assert_eq!(rate_wire(Rate::default()).revert_permille, 0);
    }
}
