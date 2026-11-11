//! Repo rules that add review lanes: the `lanes` of `.orbyn/crew.json`. A rule can only add a lane or
//! tighten (docs/crew.md, "Review stages"): there is no field here that removes one, and an agent never
//! edits the file (N7). Other keys of the file belong to later slices and are ignored here. A file that
//! cannot be read as rules is an error, never an empty rule set, so a broken rule never means no review.

use std::path::Path;

use orbyn_contracts::{CrewPathPattern, CrewToken};
use serde::Deserialize;

use crate::crew::checks::covers;

/// Larger than any sane rule file.
const LARGEST: u64 = 64 * 1024;

/// Where the rules live, relative to the repository.
pub const RULES_FILE: &str = ".orbyn/crew.json";

#[derive(Debug, Default, Deserialize)]
struct File {
    #[serde(default)]
    lanes: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
struct Entry {
    lane: String,
    #[serde(default)]
    when: Option<When>,
    #[serde(default)]
    locked: bool,
}

#[derive(Debug, Deserialize)]
struct When {
    /// The lane runs when the change touches any of these paths.
    #[serde(default)]
    paths: Vec<String>,
}

/// One lane a rule adds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleLane {
    pub lane: CrewToken,
    /// Only a person removes a locked lane. Nothing here removes one.
    pub locked: bool,
    /// `None` means the lane always runs; otherwise it runs when the change touches one of these.
    when: Option<Vec<CrewPathPattern>>,
}

impl RuleLane {
    /// Whether the rule asks for this lane for a change that touched `changed`. When the changed paths are
    /// not known, a lane with a condition runs: not knowing is never a reason to skip a review.
    pub fn applies(&self, changed: Option<&[String]>) -> bool {
        match (&self.when, changed) {
            (None, _) | (Some(_), None) => true,
            (Some(patterns), Some(changed)) => changed
                .iter()
                .any(|path| patterns.iter().any(|p| covers(p, path))),
        }
    }
}

/// The lanes a repo's rules name. A missing file is no rules.
pub fn read_rules(root: &Path) -> Result<Vec<RuleLane>, String> {
    let path = root.join(RULES_FILE);
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("{RULES_FILE} cannot be read: {error}")),
    };
    if !meta.is_file() || meta.len() > LARGEST {
        return Err(format!("{RULES_FILE} is not a regular file under 64 KiB"));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("{RULES_FILE} cannot be read: {error}"))?;
    parse(&text)
}

pub(super) fn parse(text: &str) -> Result<Vec<RuleLane>, String> {
    let file: File = serde_json::from_str(text)
        .map_err(|error| format!("{RULES_FILE} is not valid: {error}"))?;
    file.lanes
        .into_iter()
        .map(|entry| {
            let lane = CrewToken::new(entry.lane.clone())
                .map_err(|why| format!("{RULES_FILE}: lane {:?}: {why}", entry.lane))?;
            let when = entry
                .when
                .map(|when| {
                    when.paths
                        .iter()
                        .map(|p| {
                            CrewPathPattern::new(p.clone())
                                .map_err(|why| format!("{RULES_FILE}: path {p:?}: {why}"))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            Ok(RuleLane {
                lane,
                locked: entry.locked,
                when,
            })
        })
        .collect()
}
