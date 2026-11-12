//! What the service does with the settings: read the view, change one thing at a time, try a
//! pattern. Every change is one transaction with its audit row.

use orbyn_contracts::{
    RedactionFormat, RedactionLimits, RedactionPattern, RedactionPatternInput,
    RedactionPatternProblem, RedactionPatternTest, RedactionPatternTestInput, RedactionProject,
    RedactionSettings, RedactionSpan, RedactionSurfaceChange,
};
use serde_json::json;

use super::builtin::{self, BUILTINS};
use super::custom::{self, Problem};
use super::redactor::{Custom, Redactor};
use super::settings::{Stored, StoredPattern, Surface, check_new};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::AuditRecord;
use crate::util::new_id;

/// A pattern that hides this share of the lines it is tried on hides nearly everything.
const BROAD_PERCENT: usize = 90;
/// Fewer lines than this say nothing about a pattern; the ordinary text below is used instead.
const MIN_CONTEXT_LINES: usize = 10;

/// Ordinary text that holds no secret: prose, paths, logs and code. A pattern that hides most of
/// it hides what a person reads all day.
const ORDINARY: &[&str] = &[
    "Compiling orbyn-core v0.1.0 (/work/orbyn/crates/orbyn-core)",
    "Finished dev profile in 12.4s",
    "src/redaction/mod.rs:42: fn build() -> Result<()> {",
    "The quick brown fox jumps over the lazy dog.",
    "$ git status --short",
    " M web/src/features/settings/SettingsPage.tsx",
    "running 14 tests",
    "test result: ok. 14 passed; 0 failed; 0 ignored",
    "npm run check",
    "Listening on http://127.0.0.1:5391",
    "const total = items.reduce((sum, item) => sum + item.price, 0);",
    "Please review the change and tell me what you think.",
];

fn problem(found: Problem) -> CoreError {
    CoreError::PatternRefused {
        code: "invalid_pattern",
        message: found.message,
        detail: json!({ "position": found.position }),
    }
}

fn audit(actor: &str, action: &str, target: &str, widens: bool) -> AuditRecord {
    AuditRecord {
        actor: actor.to_owned(),
        action: action.to_owned(),
        target: target.to_owned(),
        decision: None,
        source: "user".to_owned(),
        thread_id: None,
        run_id: None,
        // Ids and names only: a pattern's text is not copied into the log.
        detail: json!({ "widens": widens }),
    }
}

fn is_broad(hit: usize, total: usize) -> bool {
    total > 0 && hit * 100 >= total * BROAD_PERCENT
}

/// The share of ordinary text a pattern hides, as (lines hit, lines tried).
fn ordinary_hits(redactor: &Redactor) -> (usize, usize) {
    let lines: Vec<String> = ORDINARY.iter().map(|line| (*line).to_owned()).collect();
    (redactor.lines_hit(&lines), lines.len())
}

impl Core {
    fn stored_redaction(&self) -> Result<Stored> {
        Ok(Stored::parse(self.store.redaction_json()?.as_deref()))
    }

    /// The detectors for a surface, from the settings as saved. Crew does not come through here:
    /// it uses `Redactor::locked`, which no setting reaches.
    pub fn redactor(&self, surface: Surface, project_id: Option<&str>) -> Result<Redactor> {
        Ok(self.stored_redaction()?.redactor(surface, project_id))
    }

    pub fn redaction_settings(&self) -> Result<RedactionSettings> {
        Ok(self.view(&self.stored_redaction()?))
    }

    fn view(&self, stored: &Stored) -> RedactionSettings {
        let off = stored.known_formats_off();
        RedactionSettings {
            terminal: stored.terminal,
            agents: stored.agents,
            crew_locked: true,
            formats: BUILTINS
                .iter()
                .map(|b| RedactionFormat {
                    id: b.id.to_owned(),
                    label: b.label.to_owned(),
                    example: b.example.to_owned(),
                    enabled: !off.contains(b.id),
                })
                .collect(),
            patterns: stored.patterns.iter().map(pattern_view).collect(),
            projects_off: stored
                .projects_off
                .iter()
                .filter_map(|id| self.store.project(id).ok())
                .map(|project| RedactionProject {
                    id: project.id,
                    name: project.name,
                })
                .collect(),
            limits: RedactionLimits {
                max_patterns: custom::MAX_PATTERNS as u32,
                max_name_chars: custom::MAX_NAME_CHARS as u32,
                max_pattern_chars: custom::MAX_PATTERN_CHARS as u32,
                max_sample_chars: custom::MAX_SAMPLE_CHARS as u32,
                max_context_lines: custom::MAX_CONTEXT_LINES as u32,
            },
        }
    }

    /// Applies `edit` to the stored settings and records it. `target` and `action` name the row.
    fn change_redaction(
        &self,
        actor: &str,
        action: &str,
        target: &str,
        edit: impl FnOnce(&mut Stored) -> Result<()>,
    ) -> Result<RedactionSettings> {
        let stored = self.store.change_redaction(|old| {
            let before = Stored::parse(old.as_deref());
            let mut after = before.clone();
            edit(&mut after)?;
            let widens = after.widens(&before);
            Ok((
                serde_json::to_string(&after)?,
                Some(audit(actor, action, target, widens)),
                after,
            ))
        })?;
        Ok(self.view(&stored))
    }

    pub fn change_redaction_surfaces(
        &self,
        actor: &str,
        change: &RedactionSurfaceChange,
    ) -> Result<RedactionSettings> {
        self.change_redaction(actor, "redaction.surface", "surfaces", |stored| {
            stored.terminal = change.terminal.unwrap_or(stored.terminal);
            stored.agents = change.agents.unwrap_or(stored.agents);
            Ok(())
        })
    }

    pub fn change_redaction_format(
        &self,
        actor: &str,
        id: &str,
        enabled: bool,
    ) -> Result<RedactionSettings> {
        if builtin::find(id).is_none() {
            return Err(CoreError::NotFound("recognised format".into()));
        }
        self.change_redaction(actor, "redaction.format", id, |stored| {
            if enabled {
                stored.formats_off.remove(id);
            } else {
                stored.formats_off.insert(id.to_owned());
            }
            Ok(())
        })
    }

    pub fn change_redaction_project(
        &self,
        actor: &str,
        project_id: &str,
        redaction: bool,
    ) -> Result<RedactionSettings> {
        let project = self.store.project(project_id)?;
        self.change_redaction(actor, "redaction.project", &project.id, |stored| {
            if redaction {
                stored.projects_off.remove(&project.id);
            } else {
                stored.projects_off.insert(project.id.clone());
            }
            Ok(())
        })
    }

    /// Saves a new pattern, or replaces `editing`. A pattern that hides nearly everything in
    /// ordinary text is refused with `pattern_too_broad` unless `accept_broad` is set.
    pub fn save_redaction_pattern(
        &self,
        actor: &str,
        editing: Option<&str>,
        input: &RedactionPatternInput,
    ) -> Result<RedactionSettings> {
        let regex = custom::compile(input.kind, &input.pattern).map_err(problem)?;
        if input.accept_broad != Some(true) {
            let only = Redactor::only(vec![Custom {
                name: &input.name,
                regex,
                applies_to: input.applies_to,
            }]);
            let (hit, total) = ordinary_hits(&only);
            if is_broad(hit, total) {
                return Err(CoreError::PatternRefused {
                    code: "pattern_too_broad",
                    message: format!(
                        "This pattern is very broad. It would hide {hit} of {total} lines of ordinary text."
                    ),
                    detail: json!({ "lines_matched": hit, "lines_total": total }),
                });
            }
        }
        let id = editing.map_or_else(new_id, str::to_owned);
        self.change_redaction(actor, "redaction.pattern", &id, |stored| {
            let name = check_new(stored, editing, &input.name, input.kind, &input.pattern)
                .map_err(problem)?;
            let entry = StoredPattern {
                id: id.clone(),
                name,
                pattern: input.pattern.clone(),
                kind: input.kind,
                applies_to: input.applies_to,
            };
            match editing {
                Some(editing) => {
                    let slot = stored
                        .patterns
                        .iter_mut()
                        .find(|p| p.id == editing)
                        .ok_or_else(|| CoreError::NotFound("pattern".into()))?;
                    *slot = entry;
                }
                None => stored.patterns.push(entry),
            }
            Ok(())
        })
    }

    pub fn delete_redaction_pattern(&self, actor: &str, id: &str) -> Result<RedactionSettings> {
        self.change_redaction(actor, "redaction.pattern", id, |stored| {
            let before = stored.patterns.len();
            stored.patterns.retain(|p| p.id != id);
            if stored.patterns.len() == before {
                return Err(CoreError::NotFound("pattern".into()));
            }
            Ok(())
        })
    }

    /// Tries a pattern on a sample, and on recent terminal lines when there are enough of them
    /// (the ordinary text above otherwise). Nothing is saved.
    pub fn test_redaction_pattern(
        &self,
        input: &RedactionPatternTestInput,
    ) -> RedactionPatternTest {
        let regex = match custom::compile(input.kind, &input.pattern) {
            Ok(regex) => regex,
            Err(found) => {
                return RedactionPatternTest {
                    problem: Some(RedactionPatternProblem {
                        message: found.message,
                        position: found.position.map(|at| at as u32),
                    }),
                    matches: Vec::new(),
                    masked: String::new(),
                    lines_matched: 0,
                    lines_total: 0,
                    broad: false,
                };
            }
        };
        let only = Redactor::only(vec![Custom {
            name: "pattern",
            regex,
            applies_to: input.applies_to,
        }]);
        let sample: String = input
            .sample
            .chars()
            .take(custom::MAX_SAMPLE_CHARS)
            .collect();
        let matches = only
            .spans(&sample)
            .iter()
            .map(|span| RedactionSpan {
                start: sample[..span.start].chars().count() as u32,
                end: sample[..span.end].chars().count() as u32,
            })
            .collect();
        let context: Vec<String> = input
            .lines
            .iter()
            .flatten()
            .take(custom::MAX_CONTEXT_LINES)
            .cloned()
            .collect();
        let (hit, total) = if context.len() >= MIN_CONTEXT_LINES {
            (only.lines_hit(&context), context.len())
        } else {
            ordinary_hits(&only)
        };
        RedactionPatternTest {
            problem: None,
            matches,
            masked: only.dotted(&sample),
            lines_matched: hit as u32,
            lines_total: total as u32,
            broad: is_broad(hit, total),
        }
    }
}

fn pattern_view(stored: &StoredPattern) -> RedactionPattern {
    RedactionPattern {
        id: stored.id.clone(),
        name: stored.name.clone(),
        pattern: stored.pattern.clone(),
        kind: stored.kind,
        applies_to: stored.applies_to,
    }
}
