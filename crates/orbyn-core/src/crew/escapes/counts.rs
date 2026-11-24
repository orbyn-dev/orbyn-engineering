//! One area's escape counts by engine (docs/crew/escapes.md, "Screens and the reads they need"). What the `escape-route-counts` and
//! `escape-route-shifted` frames ask for: for one project, the merges in the log's window that touched one area, and the escapes
//! logged against them, per engine. **Display only:** the Router reads no escape count in v1, so nothing here decides which engine
//! takes a task. Counts only: no path, no command, no reason and no rate. Possible, confirmed and dismissed are always separate
//! numbers, and an engine under the sample threshold says so rather than showing a rate it cannot support.

use orbyn_contracts as wire;

use super::thresholds::escape_thresholds;
use crate::app::Core;
use crate::crew::autopilot::valid_id;
use crate::crew::redact;
use crate::error::{CoreError, Result};
use crate::util::unix_now;

/// The most engine rows one area's counts list. A project's merges in one area come from the few engines it runs, so this only bounds
/// a log a person filled by hand: a longer list is cut and `truncated` says so, never dropped silently.
pub const ESCAPE_COUNT_ENGINES_MAX: usize = 16;

/// The longest area name a request may carry, as the areas read allows. An area is the first two folders of a path, so it holds `/`.
const AREA_MAX: usize = 300;

fn valid_area(area: &str) -> Result<()> {
    if area.is_empty() || area.len() > AREA_MAX || area.chars().any(char::is_control) {
        return Err(CoreError::Invalid("that area is not valid".into()));
    }
    Ok(())
}

impl Core {
    /// One area's escape counts by engine, busiest first. A read: no GET writes, and an unknown project or an empty area is refused
    /// rather than served as nothing. `low_sample` is the log's own test (fewer merges in the area than the stored `min_merges`).
    pub fn crew_escape_counts(&self, project: &str, area: &str) -> Result<wire::EscapeCounts> {
        self.crew_escape_counts_capped(project, area, ESCAPE_COUNT_ENGINES_MAX)
    }

    /// The same read with the row cap a test can reach, as the log's own reads have one: a cut list says it was cut.
    pub(crate) fn crew_escape_counts_capped(
        &self,
        project: &str,
        area: &str,
        cap: usize,
    ) -> Result<wire::EscapeCounts> {
        self.crew_preview_only()?;
        valid_id(project)?;
        valid_area(area)?;
        self.store.project(project)?;
        let thresholds = escape_thresholds(&self.store)?;
        let now = unix_now();
        let since = now - i64::from(thresholds.report_days) * 86_400;
        let tallies = self
            .store
            .crew_escape_tallies_in_area(project, area, since)?;
        let merges = tallies.values().map(|t| t.merges).sum();
        let mut engines: Vec<wire::EscapeEngineCounts> = tallies
            .iter()
            .map(|(engine, tally)| wire::EscapeEngineCounts {
                engine: redact(engine),
                merges: tally.merges,
                possible: tally.possible,
                confirmed: tally.confirmed,
                dismissed: tally.dismissed,
                low_sample: tally.merges < thresholds.min_merges,
            })
            .collect();
        // Busiest first, then by name, so a cut list keeps the engines the area was mostly merged with.
        engines.sort_by(|a, b| {
            (std::cmp::Reverse(a.merges), &a.engine).cmp(&(std::cmp::Reverse(b.merges), &b.engine))
        });
        let truncated = engines.len() > cap;
        engines.truncate(cap);
        Ok(wire::EscapeCounts {
            project: project.to_owned(),
            area: redact(area),
            as_of: now,
            window_days: thresholds.report_days,
            min_merges: thresholds.min_merges,
            merges,
            engines,
            truncated,
        })
    }
}
