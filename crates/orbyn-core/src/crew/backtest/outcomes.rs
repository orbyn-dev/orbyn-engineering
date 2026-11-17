//! Outcomes of a backtest: the escapes, that is, merges that were later reverted or fixed. They come from the
//! escape log only; a backtest never reads reverts or fixes from the forge itself.

use std::collections::BTreeMap;

/// The merge times a source is asked about, in unix seconds, both ends included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub from_unix: i64,
    pub until_unix: i64,
}

/// A merged pull request that was later reverted or fixed, and the files that revert or fix changed.
///
/// Behaviour is not recorded here: how a finding about behaviour would match a fix that changed no flagged file is
/// undefined, so matching is by file only (see `score::match_escape`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Escape {
    pub pr: u64,
    pub changed_files: Vec<String>,
}

/// The escape log has nothing to report, which is not the same as no escapes: the backtest shows "Not measured".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotMeasured;

/// Where a backtest gets its outcomes.
pub trait EscapeSource {
    /// The escapes of `project` among pull requests merged in `window`, or `NotMeasured` when the log cannot say.
    fn escapes(&self, project: &str, window: Window) -> Result<Vec<Escape>, NotMeasured>;
}

/// The source used until the escape log (slice 4k) is merged and an implementation over it exists: it always
/// reports `NotMeasured`. The real implementation comes with the escape log.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoEscapeLog;

impl EscapeSource for NoEscapeLog {
    fn escapes(&self, _project: &str, _window: Window) -> Result<Vec<Escape>, NotMeasured> {
        Err(NotMeasured)
    }
}

/// A fixed set of escapes per project, for tests. The window is applied to nothing: it hands back what it holds.
#[derive(Clone, Debug, Default)]
pub struct FixtureEscapes(BTreeMap<String, Vec<Escape>>);

impl FixtureEscapes {
    pub fn with(mut self, project: &str, escapes: Vec<Escape>) -> Self {
        self.0.insert(project.to_owned(), escapes);
        self
    }
}

impl EscapeSource for FixtureEscapes {
    fn escapes(&self, project: &str, _window: Window) -> Result<Vec<Escape>, NotMeasured> {
        self.0.get(project).cloned().ok_or(NotMeasured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Window = Window {
        from_unix: 0,
        until_unix: 10,
    };

    fn escape(pr: u64, file: &str) -> Escape {
        Escape {
            pr,
            changed_files: vec![file.to_owned()],
        }
    }

    #[test]
    fn the_fixture_returns_the_project_escapes() {
        let source = FixtureEscapes::default().with("p", vec![escape(1, "a.rs")]);
        assert_eq!(source.escapes("p", WINDOW), Ok(vec![escape(1, "a.rs")]));
    }

    #[test]
    fn a_project_the_fixture_does_not_know_is_not_measured() {
        let source = FixtureEscapes::default().with("p", vec![]);
        assert_eq!(source.escapes("other", WINDOW), Err(NotMeasured));
        assert_eq!(source.escapes("p", WINDOW), Ok(vec![]));
    }

    #[test]
    fn until_the_escape_log_exists_nothing_is_measured() {
        assert_eq!(NoEscapeLog.escapes("p", WINDOW), Err(NotMeasured));
    }
}
