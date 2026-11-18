//! `GET /api/crew/escapes/possible`: the possible escapes of one project that a person has not decided, newest found first. Read-only:
//! a GET never writes and never runs the scan (the scan runs on the service's own tick, [`super::scan`]). A dismissed escape is not
//! listed. Rows are the same `EscapeRow` as the log's, so each says why in `line` and `reason`.

use orbyn_contracts as wire;

use super::view::row_wire;
use crate::app::Core;
use crate::crew::Page;
use crate::crew::autopilot::valid_id;
use crate::error::Result;
use crate::util::unix_now;

impl Core {
    /// One page of a project's possible escapes and the number of them in all.
    pub fn crew_escape_possible(&self, project: &str, page: Page) -> Result<wire::EscapePossible> {
        self.crew_preview_only()?;
        valid_id(project)?;
        self.store.project(project)?;
        let (rows, total) = self.store.crew_escape_page(
            project,
            Some(wire::EscapeState::Possible),
            page.limit,
            page.offset,
        )?;
        Ok(wire::EscapePossible {
            project: project.to_owned(),
            as_of: unix_now(),
            total,
            rows: rows.iter().map(row_wire).collect(),
        })
    }
}
