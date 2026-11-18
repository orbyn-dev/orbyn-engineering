//! The real `EscapeSource` for the backtest (slice 4n, after 4k): it reads the escape log and nothing else, and writes nothing.
//!
//! - **What counts.** A `confirmed` escape whose merge falls in the backtest's window and which was found within 30 days of
//!   its merge (the outcome window). `possible` and `dismissed` escapes are not escapes here, and a later one is outside the
//!   window. The escape record holds no PR number and no file list, so both are resolved, never guessed.
//! - **The pull request** is the merge's own record id (`crew_merges.id`), the number `backtest_history` already gives it.
//! - **The files** are those the fix or revert changed, read from git on the merge's checkout (`diff-tree`, read-only, through
//!   the hardened helper). The commit is the one the escape's `detected` event names (`revert:<commit>` or
//!   `follow_up_fix:<commit>`); a revert may fall back to the merge's `reverted_commit`. A post-merge check failure or a
//!   person's mark names no fix commit, so nothing is read and nothing is invented: the escape is **unresolved**.
//! - **Unresolved** escapes (no commit named, no checkout, git failed, the commit changed no file) are returned in
//!   `EscapeRead::unresolved` and counted per lane in the report. They are never dropped and never scored as misses.
//! - **Fails closed.** Any unreadable row, an escape log over its 5,000 cap (`Unreadable` from the store) or a store error is
//!   `NotMeasured` for the whole source: never a shortened list, never a partial count. A project whose log holds no
//!   escape at all (not even a possible one) is `NotMeasured` too: an empty log is not proof of no escapes.
//! - **Git runs on a blocking thread.** `read` is called from the backtest's blocking task; the estimate uses `measurable`,
//!   which reads the store only.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

use orbyn_contracts::{EscapeEventKind, EscapeSource as Source, EscapeState};

use super::history::OUTCOME_DAYS;
use super::outcomes::{Escape, EscapeRead, EscapeSource, NotMeasured, Unresolved, Window};
use crate::source_control::git::git_hardened_capped;
use crate::store::{EVENTS_PER_ESCAPE_MAX, EscapeRecord, Store};

const DAY_SECS: i64 = 86_400;
const FILES_MAX: usize = 2000;
const LIST_BYTES: usize = 1 << 20;

/// The files one commit changed, for a repository path. `None` when they cannot be read (no checkout, a bad id, git failed).
/// An empty list is also unresolved: a fix that changed no file says nothing to match.
pub trait CommitFiles: Send + Sync {
    fn files(&self, repo: &str, commit: &str) -> Option<Vec<String>>;
}

/// Reads them from the checkout with the hardened, capped, read-only git helper.
#[derive(Clone, Copy, Debug, Default)]
pub struct GitCommitFiles;

fn is_commit_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl CommitFiles for GitCommitFiles {
    fn files(&self, repo: &str, commit: &str) -> Option<Vec<String>> {
        if !is_commit_id(commit) || !Path::new(repo).is_dir() {
            return None;
        }
        let handle = tokio::runtime::Handle::try_current().ok()?;
        let repo = Path::new(repo);
        // A scoped thread, so this is safe to call from a thread that is inside the runtime as well as outside it.
        let bytes = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    handle.block_on(git_hardened_capped(
                        repo,
                        &[
                            "diff-tree",
                            "--no-commit-id",
                            "--name-only",
                            "-r",
                            "-z",
                            "--no-ext-diff",
                            commit,
                        ],
                        LIST_BYTES,
                    ))
                })
                .join()
                .ok()?
                .ok()
        })?;
        let names: BTreeSet<String> = String::from_utf8_lossy(&bytes)
            .split('\0')
            .filter(|name| !name.is_empty())
            .take(FILES_MAX)
            .map(str::to_owned)
            .collect();
        Some(names.into_iter().collect())
    }
}

/// The escape log as the backtest's outcomes.
pub struct EscapeLog {
    store: Arc<Store>,
    files: Arc<dyn CommitFiles>,
}

impl EscapeLog {
    pub fn new(store: Arc<Store>, files: Arc<dyn CommitFiles>) -> Self {
        Self { store, files }
    }

    /// The escape records the backtest may count: confirmed, merged in the window, found within the outcome window.
    /// `NotMeasured` for any store failure or an empty log.
    fn counted(&self, project: &str, window: Window) -> Result<Vec<EscapeRecord>, NotMeasured> {
        // Any escape at all: an empty log is not proof that nothing escaped.
        let (_, total) = self
            .store
            .crew_escape_page(project, None, 1, 0)
            .map_err(|_| NotMeasured)?;
        if total == 0 {
            return Err(NotMeasured);
        }
        // All of it or an error (over 5,000 is `Unreadable`): never a shortened log.
        let all = self
            .store
            .crew_escapes_of_project(project, window.from_unix)
            .map_err(|_| NotMeasured)?;
        Ok(all
            .into_iter()
            .filter(|e| {
                e.state == EscapeState::Confirmed
                    && e.merged_unix <= window.until_unix
                    && e.found_unix <= e.merged_unix.saturating_add(OUTCOME_DAYS * DAY_SECS)
            })
            .collect())
    }

    /// The commit that fixed or reverted the merge, as far as the log names it.
    fn fix_commit(
        &self,
        record: &EscapeRecord,
        reverted: Option<&str>,
    ) -> Result<Option<String>, NotMeasured> {
        let events = self
            .store
            .crew_escape_events(&record.id, EVENTS_PER_ESCAPE_MAX)
            .map_err(|_| NotMeasured)?;
        let named = events
            .iter()
            .find(|event| event.kind == EscapeEventKind::Detected)
            .and_then(|event| event.cause.split_once(':'))
            .and_then(|(kind, id)| match (record.source, kind) {
                (Source::Revert, "revert") | (Source::FollowUpFix, "follow_up_fix") => Some(id),
                _ => None,
            })
            .filter(|id| is_commit_id(id));
        let fallback = (record.source == Source::Revert)
            .then_some(reverted)
            .flatten()
            .filter(|id| is_commit_id(id));
        Ok(named.or(fallback).map(str::to_owned))
    }
}

impl EscapeSource for EscapeLog {
    fn escapes(&self, project: &str, window: Window) -> Result<Vec<Escape>, NotMeasured> {
        self.read(project, window).map(|read| read.escapes)
    }

    fn read(&self, project: &str, window: Window) -> Result<EscapeRead, NotMeasured> {
        let mut read = EscapeRead::default();
        for record in self.counted(project, window)? {
            let merge = self
                .store
                .crew_merge(&record.merge_task_id)
                .map_err(|_| NotMeasured)?;
            let pr = merge.as_ref().and_then(|m| u64::try_from(m.id).ok());
            let files = match (&merge, pr) {
                (Some(merge), Some(_)) => self
                    .fix_commit(&record, merge.reverted_commit.as_deref())?
                    .and_then(|commit| self.files.files(&merge.repo, &commit))
                    .filter(|files| !files.is_empty()),
                _ => None,
            };
            match (pr, files) {
                (Some(pr), Some(changed_files)) => read.escapes.push(Escape { pr, changed_files }),
                _ => read.unresolved.push(Unresolved { pr }),
            }
        }
        Ok(read)
    }

    fn measurable(&self, project: &str, window: Window) -> bool {
        self.counted(project, window).is_ok()
    }
}
