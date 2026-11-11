//! Watching linked pull requests for news: polling, what changed and the comment wake limit.

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::{self, Forge, forge_of};
use crate::store::WatchUpdate;
use orbyn_contracts::{PullCheck, PullWatch};
use std::path::Path;

impl Core {
    /// One pass of the pull request watch. A failure waits for the next pass.
    pub async fn watch_pulls(&self) {
        let threads = self.store.threads_with_open_pulls().unwrap_or_default();
        for id in threads {
            if let Err(error) = self.refresh_pulls(&id).await {
                tracing::debug!(%error, "could not refresh a pull request");
            }
        }
        let watches = self.store.pull_watches(None).unwrap_or_default();
        for watch in watches {
            if let Err(error) = self.check_watch(&watch).await {
                tracing::debug!(%error, "could not check a watched pull request");
            }
        }
    }

    /// Reads a watched pull request and wakes the thread on news. A read that
    /// fails counts toward the stop rule and never wakes anyone.
    pub(super) async fn check_watch(&self, watch: &crate::store::WatchRow) -> Result<()> {
        let number = watch.number;
        let thread = self.scm_thread(&watch.thread_id)?;
        let cwd = Path::new(&thread.cwd);
        let forge = forge_of(cwd).await?;
        let read = read_watch(forge, cwd, number).await;
        let (state, viewer) = match read {
            Ok(read) => read,
            Err(error) => {
                let failures = watch.failures + 1;
                if failures >= MAX_WATCH_FAILURES {
                    self.store.unwatch_pull(&watch.thread_id, number)?;
                } else {
                    self.store.update_watch(
                        &watch.thread_id,
                        number,
                        WatchUpdate::failed(failures),
                    )?;
                }
                return Err(error);
            }
        };
        if state.finished {
            self.store.unwatch_pull(&watch.thread_id, number)?;
            self.scm_reads.project_dropped(&thread.project_id);
            return Ok(());
        }
        let last = watch
            .snapshot
            .as_deref()
            .and_then(|text| serde_json::from_str(text).ok())
            .unwrap_or_default();
        let (news, comments_only) = watch_news(&last, &state, viewer.as_deref());
        if !news.is_empty() {
            self.scm_reads.project_dropped(&thread.project_id);
        }
        let wakes = if news.is_empty() {
            watch.comment_wakes
        } else if comments_only {
            watch.comment_wakes + 1
        } else {
            0
        };
        self.store.update_watch(
            &watch.thread_id,
            number,
            WatchUpdate {
                snapshot: Some(serde_json::to_string(&state)?),
                comment_wakes: wakes,
                failures: 0,
            },
        )?;
        if !news.is_empty() {
            let prompt = format!(
                "Pull request #{number} changed while you were watching it:\n{}",
                news.join("\n")
            );
            self.runs.enqueue(&watch.thread_id, &prompt, &[])?;
        }
        if wakes >= MAX_COMMENT_WAKES {
            self.store.unwatch_pull(&watch.thread_id, number)?;
        }
        Ok(())
    }

    /// Starts watching. A pull request watched before keeps what it last reported.
    pub fn watch_pull(&self, id: &str, number: u64) -> Result<PullWatch> {
        let thread = self.scm_thread(id)?;
        let lifecycle = thread.lifecycle.as_ref();
        let parked = lifecycle.is_some_and(|l| l.settled_at.is_some() || l.archived_at.is_some());
        if parked {
            return Err(CoreError::Invalid(
                "Unsettle this thread before watching a pull request.".into(),
            ));
        }
        if lifecycle.is_some_and(|l| l.relation.as_deref() == Some("subagent")) {
            return Err(CoreError::Invalid(
                "The thread that delegated this work watches pull requests.".into(),
            ));
        }
        self.store.watch_pull(id, number)
    }

    pub fn thread_watches(&self, id: &str) -> Result<Vec<PullWatch>> {
        Ok(self
            .store
            .pull_watches(Some(id))?
            .into_iter()
            .map(|row| PullWatch {
                thread_id: row.thread_id,
                number: row.number,
                created_at: row.created_at,
            })
            .collect())
    }
}

/// Watching ends after this many wakes in a row that bring only comments.
pub(super) const MAX_COMMENT_WAKES: i64 = 10;

/// Watching ends after this many failed reads in a row.
pub(super) const MAX_WATCH_FAILURES: i64 = 8;

/// What a watch compares between passes.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct WatchState {
    pub(super) checks: String,
    pub(super) conflicting: bool,
    /// Comment and review ids, oldest first, with their authors.
    pub(super) comments: Vec<(String, Option<String>)>,
    #[serde(skip)]
    pub(super) finished: bool,
}

pub(super) async fn read_watch(
    forge: Forge,
    cwd: &Path,
    number: u64,
) -> Result<(WatchState, Option<String>)> {
    let detail = forge::pull_detail(forge, cwd, number).await?;
    let finished = detail.pull.state != "open";
    let checks = forge::pull_checks(forge, cwd, number)
        .await
        .unwrap_or_default();
    let conversation = forge::conversation(forge, cwd, number).await?;
    let mut comments: Vec<(String, Option<String>)> = conversation
        .comments
        .iter()
        .chain(
            conversation
                .threads
                .iter()
                .flat_map(|thread| &thread.comments),
        )
        .map(|comment| (comment.id.clone(), comment.author.clone()))
        .collect();
    comments.sort();
    Ok((
        WatchState {
            checks: checks_word(&checks),
            conflicting: detail.mergeable.as_deref() == Some("conflicting"),
            comments,
            finished,
        },
        forge::viewer(forge, cwd).await,
    ))
}

pub(super) fn checks_word(checks: &[PullCheck]) -> String {
    if checks.iter().any(|check| check.state == "fail") {
        "fail"
    } else if checks.iter().any(|check| check.state == "pending") {
        "pending"
    } else if checks.is_empty() {
        "none"
    } else {
        "pass"
    }
    .to_owned()
}

/// What changed since the last pass, and whether comments are all of it. Your own
/// comments are not news.
pub(super) fn watch_news(
    last: &WatchState,
    now: &WatchState,
    viewer: Option<&str>,
) -> (Vec<String>, bool) {
    let mut news = Vec::new();
    if now.checks != last.checks {
        match now.checks.as_str() {
            "fail" => news.push("- A check failed.".to_owned()),
            "pass" => news.push("- All checks passed.".to_owned()),
            _ => {}
        }
    }
    if now.conflicting && !last.conflicting {
        news.push("- The branch now conflicts with its base.".to_owned());
    }
    let structural = !news.is_empty();
    let fresh = now
        .comments
        .iter()
        .filter(|comment| !last.comments.contains(comment))
        .filter(|(_, author)| author.as_deref() != viewer || viewer.is_none())
        .count();
    if fresh > 0 {
        news.push(format!("- {fresh} new comment(s) or review(s)."));
    }
    (news, !structural)
}
