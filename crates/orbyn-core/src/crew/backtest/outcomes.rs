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

/// A confirmed escape inside the window whose changed files could not be read from git, so it cannot be matched. It is
/// counted and reported, never dropped and never given a guessed file list. `pr` is `None` only when the merge's own
/// record could not be found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unresolved {
    pub pr: Option<u64>,
}

/// What a source read: the escapes it could match, and those it could not.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EscapeRead {
    pub escapes: Vec<Escape>,
    pub unresolved: Vec<Unresolved>,
}

/// The escape log has nothing to report, which is not the same as no escapes: the backtest shows "Not measured".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotMeasured;

/// Where a backtest gets its outcomes.
pub trait EscapeSource {
    /// The escapes of `project` among pull requests merged in `window`, or `NotMeasured` when the log cannot say.
    fn escapes(&self, project: &str, window: Window) -> Result<Vec<Escape>, NotMeasured>;

    /// `escapes` and the escapes that could not be resolved. A source that cannot fail to resolve (a fixture) has none.
    fn read(&self, project: &str, window: Window) -> Result<EscapeRead, NotMeasured> {
        self.escapes(project, window).map(|escapes| EscapeRead {
            escapes,
            unresolved: Vec::new(),
        })
    }

    /// Whether `read` would not say Not measured, decided without reading git, for a caller that must not block.
    fn measurable(&self, project: &str, window: Window) -> bool {
        self.escapes(project, window).is_ok()
    }
}

/// A source that always reports `NotMeasured`: for tests, and for a core that has no escape log to read.
/// The real source over the escape log is `EscapeLog`.
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
