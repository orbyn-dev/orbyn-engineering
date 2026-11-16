//! Background crews, their state at rest (docs/crew.md, "Background crews"). A background crew is a crew the person defines once for a
//! repository: a template, what would start it, a run budget set at creation and an on/off switch. It lives in the service's data
//! folder, never in the repository (architecture rule 4), so nothing here writes a file into a target.
//!
//! This module validates and redacts what the person sends, then hands it to `store::crew_background`, which audits every change
//! in the same transaction and holds the rules that must stand whatever the caller does. **No trigger fires and no engine starts
//! here:** the run functions are the seam slice 4h will call, and today only tests do. A run that reaches its budget stops with the
//! reason `budget` and waits for the person's review; it has no merge authority (there is nothing to merge until a later slice
//! brings a result through the Autopilot merge queue). With the Crew Preview flag off every function answers "not found".

use orbyn_contracts as wire;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::{Page, redact, valid_id, wire_name};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{BackgroundRow, BackgroundRunRow, NewBackground, RunEnd};

/// The longest name, in characters.
pub const NAME_MAX: usize = 80;
/// The longest text of a trigger setting, in characters.
pub const SETTING_MAX: usize = 100;
/// The widest gap of a schedule, in hours: 30 days.
pub const EVERY_HOURS_MAX: u32 = 720;
/// The largest budget of turns and of minutes a run may be given.
pub const TURNS_MAX: u32 = 1000;
pub const MINUTES_MAX: u32 = 1440;

/// How a run that is running ends, other than by its budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundEnd {
    Stopped(wire::BackgroundStopReason),
    NeedsReview,
}

fn parse_wire<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_value(Value::String(text.to_owned()))
        .map_err(|_| CoreError::Internal(format!("a stored background value is unknown: {text}")))
}

/// Characters that hide text from a person, a wider set than the Autopilot tripwire's: zero-width, bidirectional controls and the tag
/// block, plus soft hyphen, combining grapheme joiner, Arabic letter mark, Hangul and Braille fillers, Mongolian free variation
/// selectors, line and paragraph separators, the deprecated format controls and interlinear annotation marks. A name or trigger text
/// the person types has no use for any of them; the tripwire reads agent text, where some are legitimate, so it keeps its own list.
/// Emoji variation selectors are allowed.
fn invisible(c: char) -> bool {
    matches!(c, '\u{00ad}' | '\u{034f}' | '\u{061c}' | '\u{115f}'..='\u{1160}' | '\u{17b4}'..='\u{17b5}' | '\u{180b}'..='\u{180f}'
        | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{2800}' | '\u{3164}' | '\u{feff}' | '\u{ffa0}'
        | '\u{fff9}'..='\u{fffb}' | '\u{e0000}'..='\u{e007f}')
}

/// Text a person typed: trimmed, 1 to `max` characters, no control characters and none that hide text (zero-width, bidirectional,
/// tag-block and other invisible format characters, see `invisible`), then redacted by the locked redactor. A redaction
/// that makes it too long is refused rather than cut.
pub(super) fn text(label: &str, value: &str, max: usize) -> Result<String> {
    let value = value.trim();
    let count = value.chars().count();
    if !(1..=max).contains(&count) {
        return Err(CoreError::Invalid(format!(
            "{label} is 1 to {max} characters"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(CoreError::Invalid(format!(
            "{label} has a character that cannot be shown"
        )));
    }
    if value.chars().any(invisible) {
        return Err(CoreError::Invalid(format!(
            "{label} has a hidden character"
        )));
    }
    let redacted = redact(value);
    if redacted.chars().count() > max {
        return Err(CoreError::Invalid(format!("{label} is too long")));
    }
    Ok(redacted)
}

/// Which settings each trigger takes, and which it needs.
fn allowed(
    kind: wire::BackgroundTriggerKind,
) -> (&'static [&'static str], &'static [&'static str]) {
    use wire::BackgroundTriggerKind::*;
    match kind {
        CiFailure => (&["branch", "workflow"], &[]),
        IssueLabel => (&["label"], &["label"]),
        Schedule => (&["every_hours"], &["every_hours"]),
        DependencyUpdate => (&["ecosystem"], &[]),
    }
}

/// A trigger checked and redacted: its kind and the settings it stores, only the ones that are set.
fn trigger(trigger: &wire::BackgroundTrigger) -> Result<(String, Value)> {
    let mut settings = Map::new();
    let set = [
        ("branch", trigger.branch.as_deref()),
        ("workflow", trigger.workflow.as_deref()),
        ("label", trigger.label.as_deref()),
        ("ecosystem", trigger.ecosystem.as_deref()),
    ];
    for (name, value) in set {
        if let Some(value) = value {
            settings.insert(
                name.to_owned(),
                Value::String(text(name, value, SETTING_MAX)?),
            );
        }
    }
    if let Some(hours) = trigger.every_hours {
        if !(1..=EVERY_HOURS_MAX).contains(&hours) {
            return Err(CoreError::Invalid(format!(
                "every_hours is 1 to {EVERY_HOURS_MAX}"
            )));
        }
        settings.insert("every_hours".into(), json!(hours));
    }
    let kind = trigger.kind;
    let (takes, needs) = allowed(kind);
    let kind_name = wire_name(&kind);
    for name in settings.keys() {
        if !takes.contains(&name.as_str()) {
            return Err(CoreError::Invalid(format!(
                "a {kind_name} trigger has no {name} setting"
            )));
        }
    }
    for name in needs {
        if !settings.contains_key(*name) {
            return Err(CoreError::Invalid(format!(
                "a {kind_name} trigger needs {name}"
            )));
        }
    }
    Ok((kind_name, Value::Object(settings)))
}

fn trigger_wire(kind: &str, settings: &Value) -> Result<wire::BackgroundTrigger> {
    let get = |name: &str| settings[name].as_str().map(str::to_owned);
    Ok(wire::BackgroundTrigger {
        kind: parse_wire(kind)?,
        branch: get("branch"),
        workflow: get("workflow"),
        label: get("label"),
        every_hours: settings["every_hours"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok()),
        ecosystem: get("ecosystem"),
    })
}

fn budget_wire(turns: u32, minutes: u32) -> wire::BackgroundBudget {
    wire::BackgroundBudget { turns, minutes }
}

fn run_wire(run: BackgroundRunRow) -> Result<wire::BackgroundRun> {
    Ok(wire::BackgroundRun {
        state: parse_wire(&run.state)?,
        stop_reason: run.stop_reason.as_deref().map(parse_wire).transpose()?,
        budget: budget_wire(run.budget_turns, run.budget_minutes),
        id: run.id,
        crew_id: run.background_id,
        turns_spent: run.turns_spent,
        minutes_spent: run.minutes_spent,
        review_waiting: run.review_waiting,
        review_reason: run.review_reason,
        started_at: run.started_at,
        updated_at: run.updated_at,
        ended_at: run.ended_at,
    })
}

fn next_offset(page: Page, total: u32) -> Option<u32> {
    let next = page.offset.checked_add(page.limit)?;
    (next < total).then_some(next)
}

impl Core {
    fn background_wire(&self, crew: BackgroundRow) -> Result<wire::BackgroundCrew> {
        let (newest, waiting) = self.store.background_standing(&crew.id)?;
        // `idle`: nothing is running, nothing waits for the person, and the newest run did not stop for a reason that still needs them.
        // A run stopped by the person's own switch (`disabled`) needs nothing; a stop for the budget or a failure does.
        let state = match newest
            .as_ref()
            .map(|run| (run.state.as_str(), run.stop_reason.as_deref()))
        {
            Some(("running", _)) => wire::BackgroundState::Running,
            Some(("needs_review", _)) => wire::BackgroundState::NeedsReview,
            Some(("stopped", Some("disabled"))) | None if waiting == 0 => {
                wire::BackgroundState::Idle
            }
            Some(("stopped", Some("disabled"))) | None => wire::BackgroundState::NeedsReview,
            Some(("stopped", _)) => wire::BackgroundState::Stopped,
            Some((other, _)) => {
                return Err(CoreError::Internal(format!(
                    "a stored background run state is unknown: {other}"
                )));
            }
        };
        Ok(wire::BackgroundCrew {
            trigger: trigger_wire(&crew.trigger_kind, &crew.trigger_settings)?,
            budget: budget_wire(crew.budget_turns, crew.budget_minutes),
            template: parse_wire(&crew.template)?,
            id: crew.id,
            project_id: crew.project_id,
            name: crew.name,
            enabled: crew.enabled,
            state,
            reviews_waiting: waiting,
            created_at: crew.created_at,
            updated_at: crew.updated_at,
        })
    }

    fn background_row(&self, id: &str) -> Result<BackgroundRow> {
        valid_id(id)?;
        self.store
            .background_crew(id)?
            .ok_or_else(|| CoreError::NotFound("background crew".into()))
    }

    /// The background crews, by name, one page. `project` narrows the list to one project.
    pub fn background_list(
        &self,
        project: Option<&str>,
        page: Page,
    ) -> Result<wire::BackgroundCrewList> {
        self.crew_preview_only()?;
        if let Some(project) = project {
            valid_id(project)?;
        }
        let (rows, total) = self
            .store
            .background_crews_page(project, page.limit, page.offset)?;
        Ok(wire::BackgroundCrewList {
            crews: rows
                .into_iter()
                .map(|row| self.background_wire(row))
                .collect::<Result<_>>()?,
            total,
            next_offset: next_offset(page, total),
        })
    }

    pub fn background_get(&self, id: &str) -> Result<wire::BackgroundCrew> {
        self.crew_preview_only()?;
        self.background_wire(self.background_row(id)?)
    }

    /// Creates a background crew, off. The project must be a live project; every bound is checked here and again by the table.
    pub fn background_create(
        &self,
        create: &wire::BackgroundCrewCreate,
    ) -> Result<wire::BackgroundCrew> {
        self.crew_preview_only()?;
        valid_id(&create.project_id)?;
        let name = text("name", &create.name, NAME_MAX)?;
        let (kind, settings) = trigger(&create.trigger)?;
        if !(1..=TURNS_MAX).contains(&create.budget.turns) {
            return Err(CoreError::Invalid(format!(
                "the budget is 1 to {TURNS_MAX} turns"
            )));
        }
        if !(1..=MINUTES_MAX).contains(&create.budget.minutes) {
            return Err(CoreError::Invalid(format!(
                "the budget is 1 to {MINUTES_MAX} minutes"
            )));
        }
        self.store.project(&create.project_id)?;
        let row = self.store.create_background_crew(
            &NewBackground {
                project_id: &create.project_id,
                name: &name,
                template: &wire_name(&create.template),
                trigger_kind: &kind,
                trigger_settings: &settings,
                budget_turns: create.budget.turns,
                budget_minutes: create.budget.minutes,
            },
            "user",
        )?;
        self.background_wire(row)
    }

    /// Changes a crew's name and, or, trigger. The project, template and budget are fixed at creation.
    pub fn background_update(
        &self,
        id: &str,
        update: &wire::BackgroundCrewUpdate,
    ) -> Result<wire::BackgroundCrew> {
        self.crew_preview_only()?;
        self.background_row(id)?;
        if update.name.is_none() && update.trigger.is_none() {
            return Err(CoreError::Invalid(
                "send a name or a trigger to change".into(),
            ));
        }
        let name = update
            .name
            .as_deref()
            .map(|name| text("name", name, NAME_MAX))
            .transpose()?;
        let trigger = update.trigger.as_ref().map(trigger).transpose()?;
        let row = self.store.update_background_crew(
            id,
            name.as_deref(),
            trigger
                .as_ref()
                .map(|(kind, settings)| (kind.as_str(), settings)),
            "user",
        )?;
        self.background_wire(row)
    }

    /// Turns a crew on or off. Turning it off stops a run in progress with the reason `disabled`, logged as the person.
    pub fn background_set_enabled(&self, id: &str, enabled: bool) -> Result<wire::BackgroundCrew> {
        self.crew_preview_only()?;
        self.background_row(id)?;
        let row = self.store.set_background_enabled(id, enabled, "user")?;
        self.background_wire(row)
    }

    /// Deletes a crew and its runs. Refused only while a run is in progress; waiting reviews go with the crew.
    pub fn background_delete(&self, id: &str) -> Result<()> {
        self.crew_preview_only()?;
        self.background_row(id)?;
        self.store.delete_background_crew(id, "user")
    }

    /// A crew's runs, newest first, one page.
    pub fn background_runs(&self, id: &str, page: Page) -> Result<wire::BackgroundRunList> {
        self.crew_preview_only()?;
        self.background_row(id)?;
        let (rows, total) = self
            .store
            .background_runs_page(id, page.limit, page.offset)?;
        Ok(wire::BackgroundRunList {
            runs: rows.into_iter().map(run_wire).collect::<Result<_>>()?,
            total,
            next_offset: next_offset(page, total),
        })
    }

    /// Opens a run of an enabled crew with the crew's budget. Not reachable over HTTP and fired by nothing yet: slice 4h's triggers
    /// call it.
    pub fn crew_background_start_run(&self, id: &str) -> Result<wire::BackgroundRun> {
        self.crew_preview_only()?;
        self.background_row(id)?;
        run_wire(self.store.start_background_run(id)?)
    }

    /// Adds what a run spent. Reaching its budget stops it with the reason `budget` and queues it for the person's review. One call
    /// adds at most one run's whole budget, and something.
    pub fn crew_background_spend(
        &self,
        run_id: &str,
        turns: u32,
        minutes: u32,
    ) -> Result<wire::BackgroundRun> {
        self.crew_preview_only()?;
        valid_id(run_id)?;
        if turns == 0 && minutes == 0 {
            return Err(CoreError::Invalid("spend something".into()));
        }
        if turns > TURNS_MAX || minutes > MINUTES_MAX {
            return Err(CoreError::Invalid(
                "that is more than a run may spend".into(),
            ));
        }
        run_wire(self.store.add_background_spend(run_id, turns, minutes)?)
    }

    /// Ends a run that is running: stopped for a reason other than the budget (that is `crew_background_spend`'s), or finished with
    /// work for review.
    pub fn crew_background_end_run(
        &self,
        run_id: &str,
        end: BackgroundEnd,
    ) -> Result<wire::BackgroundRun> {
        self.crew_preview_only()?;
        valid_id(run_id)?;
        let reason;
        let end = match end {
            BackgroundEnd::Stopped(why) => {
                reason = wire_name(&why);
                RunEnd::Stopped(&reason)
            }
            BackgroundEnd::NeedsReview => {
                RunEnd::NeedsReview("The run finished and has work to review.")
            }
        };
        run_wire(self.store.end_background_run(run_id, end)?)
    }
}
