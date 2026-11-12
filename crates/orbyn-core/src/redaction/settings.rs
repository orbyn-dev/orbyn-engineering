//! The stored settings and the pure rules over them. The defaults are the safe ones: a missing,
//! unreadable or unknown document means everything is hidden, never nothing.

use std::collections::BTreeSet;

use orbyn_contracts::{PatternKind, PatternScope};
use serde::{Deserialize, Serialize};

use super::builtin::BUILTINS;
use super::custom::{self, MAX_PATTERNS, Problem};
use super::redactor::{Custom, Redactor};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredPattern {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub kind: PatternKind,
    pub applies_to: PatternScope,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Stored {
    pub terminal: bool,
    pub agents: bool,
    /// Ids of recognised formats the person turned off.
    pub formats_off: BTreeSet<String>,
    pub patterns: Vec<StoredPattern>,
    /// Projects whose agents read values as they are.
    pub projects_off: BTreeSet<String>,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            terminal: true,
            agents: true,
            formats_off: BTreeSet::new(),
            patterns: Vec::new(),
            projects_off: BTreeSet::new(),
        }
    }
}

/// Where values are hidden for the person, apart from Crew, which has one fixed rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Terminal,
    Agents,
}

impl Stored {
    /// A document that does not read is the default, which hides everything.
    pub fn parse(text: Option<&str>) -> Self {
        text.and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default()
    }

    /// Ids of formats that no longer exist drop out, so an old document cannot name a ghost.
    pub fn known_formats_off(&self) -> BTreeSet<String> {
        self.formats_off
            .iter()
            .filter(|id| BUILTINS.iter().any(|b| b.id == id.as_str()))
            .cloned()
            .collect()
    }

    /// The detectors for one surface. `None` project means the setting for every project.
    /// A surface that is off hides nothing, and the returned set says so with `is_empty`.
    pub fn redactor(&self, surface: Surface, project: Option<&str>) -> Redactor {
        let on = match surface {
            Surface::Terminal => self.terminal,
            Surface::Agents => {
                self.agents && project.is_none_or(|id| !self.projects_off.contains(id))
            }
        };
        if !on {
            return Redactor::default();
        }
        let mut compiled = Vec::new();
        for stored in &self.patterns {
            // A pattern that no longer compiles under today's limits is skipped, never fatal.
            if let Ok(regex) = custom::compile(stored.kind, &stored.pattern) {
                compiled.push((stored.name.as_str(), regex, stored.applies_to));
            }
        }
        Redactor::with(
            &self.known_formats_off(),
            compiled
                .into_iter()
                .map(|(name, regex, applies_to)| Custom {
                    name,
                    regex,
                    applies_to,
                })
                .collect(),
        )
    }

    /// Whether any change from `before` to `self` lets more reach an agent or a screen. A pattern
    /// that is gone, or whose text, type or place changed, counts: an edit can stop it matching.
    /// Adding a pattern, and renaming one, do not.
    pub fn widens(&self, before: &Stored) -> bool {
        (before.terminal && !self.terminal)
            || (before.agents && !self.agents)
            || !self.formats_off.is_subset(&before.formats_off)
            || !self.projects_off.is_subset(&before.projects_off)
            || before.patterns.iter().any(|old| {
                !self.patterns.iter().any(|new| {
                    new.id == old.id
                        && new.pattern == old.pattern
                        && new.kind == old.kind
                        && new.applies_to == old.applies_to
                })
            })
    }

    /// Only the person's patterns, for text shown back to them (the ledger). The formats are not
    /// part of it: what Crew stores is already clean of them.
    pub fn person_redactor(&self) -> Redactor {
        let mut compiled = Vec::new();
        for stored in &self.patterns {
            if let Ok(regex) = custom::compile(stored.kind, &stored.pattern) {
                compiled.push((stored.name.as_str(), regex, stored.applies_to));
            }
        }
        Redactor::only(
            compiled
                .into_iter()
                .map(|(name, regex, applies_to)| Custom {
                    name,
                    regex,
                    applies_to,
                })
                .collect(),
        )
    }
}

/// The checks a pattern passes before it is stored. Returns the cleaned name.
pub fn check_new(
    stored: &Stored,
    editing: Option<&str>,
    name: &str,
    kind: PatternKind,
    pattern: &str,
) -> Result<String, Problem> {
    let name = custom::clean_name(name)?;
    custom::compile(kind, pattern)?;
    let others = stored
        .patterns
        .iter()
        .filter(|p| Some(p.id.as_str()) != editing);
    if editing.is_none() && stored.patterns.len() >= MAX_PATTERNS {
        return Err(Problem {
            message: format!("Orbyn keeps at most {MAX_PATTERNS} patterns. Remove one first."),
            position: None,
        });
    }
    if others
        .into_iter()
        .any(|p| p.name.eq_ignore_ascii_case(&name))
    {
        return Err(Problem {
            message: "A pattern with this name is already in the list.".to_owned(),
            position: None,
        });
    }
    Ok(name)
}
