//! What the service does with the settings: read the view, change one thing at a time, try a
//! pattern. Every change is one transaction with its audit row.

use orbyn_contracts::{
    RedactionFormat, RedactionLimits, RedactionPattern, RedactionPatternDetail,
    RedactionPatternInput, RedactionPatternProblem, RedactionPatternTest,
    RedactionPatternTestInput, RedactionProject, RedactionSettings, RedactionSpan,
    RedactionSurfaceChange,
};
use std::sync::{Arc, Mutex};

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

/// The compiled sets: bounded, and the least recently used set is evicted first.
const CACHE_MAX: usize = 32;

/// Compiled sets by the settings text they came from, so a new revision is a new key.
struct CompiledCache {
    /// Least recently used first.
    entries: Mutex<Vec<(String, Arc<Redactor>)>>,
}

impl CompiledCache {
    const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    fn get_or_build(&self, key: String, build: impl FnOnce() -> Redactor) -> Arc<Redactor> {
        {
            let mut entries = self.lock();
            if let Some(at) = entries.iter().position(|(known, _)| *known == key) {
                let hit = Arc::clone(&entries[at].1);
                // The hit is the most recently used one now.
                let entry = entries.remove(at);
                entries.push(entry);
                return hit;
            }
        }
        // Everything slow happens with no lock held, so one compile cannot hold up the others.
        let built = Arc::new(build());
        let mut entries = self.lock();
        if let Some((_, hit)) = entries.iter().find(|(known, _)| *known == key) {
            // Another read built it while this one did: its set wins.
            return Arc::clone(hit);
        }
        if entries.len() >= CACHE_MAX {
            entries.remove(0);
        }
        entries.push((key, Arc::clone(&built)));
        built
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(String, Arc<Redactor>)>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.lock().len()
    }

    #[cfg(test)]
    fn keys(&self) -> Vec<String> {
        self.lock().iter().map(|(key, _)| key.clone()).collect()
    }

    #[cfg(test)]
    fn lock_is_free(&self) -> bool {
        self.entries.try_lock().is_ok()
    }
}

static CACHE: CompiledCache = CompiledCache::new();

fn cached(key: String, build: impl FnOnce() -> Redactor) -> Arc<Redactor> {
    CACHE.get_or_build(key, build)
}

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
        // The route cannot tell the person from an agent holding the same token.
        source: "unknown".to_owned(),
        thread_id: None,
        run_id: None,
        // Ids and names only: a pattern's text is not copied into the log.
        detail: json!({ "widens": widens }),
    }
}

fn is_broad(hit: usize, total: usize) -> bool {
    total > 0 && hit * 100 >= total * BROAD_PERCENT
}

/// The share of ordinary text a pattern hides, as (characters hidden, characters tried).
fn ordinary_hits(redactor: &Redactor) -> (usize, usize) {
    let lines: Vec<String> = ORDINARY.iter().map(|line| (*line).to_owned()).collect();
    redactor.chars_hidden(&lines)
}

/// The detectors for a surface, read from the store. The run manager has no `Core`, so it comes
/// through here; `Core::redactor` is the same call.
pub(crate) fn redactor_from(
    store: &crate::store::Store,
    surface: Surface,
    project_id: Option<&str>,
) -> Result<Arc<Redactor>> {
    let text = store.redaction_json()?;
    let raw = text.as_deref().unwrap_or("");
    let key = match surface {
        // The terminal set reads no project, so every project shares its one entry.
        Surface::Terminal => format!("terminal|{raw}"),
        // The agents set is keyed by project: a project can be switched off on its own.
        Surface::Agents => format!("agents|{}|{raw}", project_id.unwrap_or("")),
    };
    Ok(cached(key, || {
        Stored::parse(text.as_deref()).redactor(surface, project_id)
    }))
}

impl Core {
    fn stored_redaction(&self) -> Result<Stored> {
        Ok(Stored::parse(self.store.redaction_json()?.as_deref()))
    }

    /// The detectors for a surface, from the settings as saved. Crew does not come through here:
    /// it uses `Redactor::locked`, which no setting reaches.
    pub fn redactor(&self, surface: Surface, project_id: Option<&str>) -> Result<Arc<Redactor>> {
        redactor_from(&self.store, surface, project_id)
    }

    /// The person's own patterns alone, for text shown back to them: the ledger and its rows are
    /// stored as they are and masked on the way out.
    ///
    /// Settings that cannot be read fail the read. Unlike the surfaces (where an unreadable document
    /// means everything is hidden), nothing here can stand in for patterns that are not known, and
    /// showing the text unmasked would drop what the person asked to hide: so the ledger read errors.
    pub fn display_redactor(&self) -> Result<Arc<Redactor>> {
        let text = self.store.redaction_json()?;
        if let Some(raw) = text.as_deref()
            && serde_json::from_str::<Stored>(raw).is_err()
        {
            tracing::error!("the redaction settings do not read; the ledger is not shown");
            return Err(CoreError::Internal(
                "the redaction settings do not read".into(),
            ));
        }
        let key = format!("display||{}", text.as_deref().unwrap_or(""));
        Ok(cached(key, || {
            Stored::parse(text.as_deref()).person_redactor()
        }))
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
                    message: format!("This would hide {hit} of {total} characters of the sample."),
                    detail: json!({ "hidden_chars": hit, "total_chars": total }),
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

    /// One pattern with its text, for the edit dialog.
    pub fn redaction_pattern(&self, id: &str) -> Result<RedactionPatternDetail> {
        let stored = self.stored_redaction()?;
        let found = stored
            .patterns
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| CoreError::NotFound("pattern".into()))?;
        Ok(RedactionPatternDetail {
            id: found.id.clone(),
            name: found.name.clone(),
            pattern: found.pattern.clone(),
            kind: found.kind,
            applies_to: found.applies_to,
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
                    hidden_chars: 0,
                    total_chars: 0,
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
            only.chars_hidden(&context)
        } else {
            ordinary_hits(&only)
        };
        RedactionPatternTest {
            problem: None,
            matches,
            masked: only.dotted(&sample),
            hidden_chars: hit as u32,
            total_chars: total as u32,
            broad: is_broad(hit, total),
        }
    }
}

fn pattern_view(stored: &StoredPattern) -> RedactionPattern {
    RedactionPattern {
        id: stored.id.clone(),
        name: stored.name.clone(),
        kind: stored.kind,
        applies_to: stored.applies_to,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    #[test]
    fn the_compiled_cache_stays_at_its_cap() {
        let cache = CompiledCache::new();
        for n in 0..CACHE_MAX + 8 {
            cache.get_or_build(format!("set-{n}"), Redactor::default);
        }
        assert_eq!(cache.len(), CACHE_MAX, "the cache grew past its cap");
        let keys = cache.keys();
        assert!(!keys.contains(&"set-0".to_owned()), "the oldest went first");
        assert!(keys.contains(&format!("set-{}", CACHE_MAX + 7)));
    }

    #[test]
    fn the_compiled_cache_evicts_the_least_recently_used_set() {
        let cache = CompiledCache::new();
        for n in 0..CACHE_MAX {
            cache.get_or_build(format!("set-{n}"), Redactor::default);
        }
        // Read the oldest again: it becomes the most recently used.
        cache.get_or_build("set-0".into(), || panic!("a cached set was compiled again"));
        cache.get_or_build("set-new".into(), Redactor::default);
        cache.get_or_build("set-0".into(), || {
            panic!("the least recently used set was not evicted first")
        });
        let mut rebuilt = false;
        cache.get_or_build("set-1".into(), || {
            rebuilt = true;
            Redactor::default()
        });
        assert!(rebuilt, "the second oldest set should have gone");
    }

    #[test]
    fn a_compile_runs_with_no_cache_lock_held() {
        let cache = CompiledCache::new();
        cache.get_or_build("slow".into(), || {
            // Another thread can take the lock only when the compile does not hold it.
            let free = std::thread::scope(|scope| {
                scope
                    .spawn(|| cache.lock_is_free())
                    .join()
                    .expect("the probe thread panicked")
            });
            assert!(free, "a compile ran while the cache lock was held");
            Redactor::default()
        });
    }

    #[test]
    fn a_set_built_twice_is_stored_once_and_reused() {
        let cache = CompiledCache::new();
        let first = cache.get_or_build("same".into(), Redactor::default);
        let again = cache.get_or_build("same".into(), || panic!("a stored set was rebuilt"));
        assert!(Arc::ptr_eq(&first, &again));
        assert_eq!(cache.len(), 1);
    }
}
