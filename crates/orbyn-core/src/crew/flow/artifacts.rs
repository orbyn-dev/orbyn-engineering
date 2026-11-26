//! One Flow task's artifacts, check results and diff (Crew Preview): the evidence rows of the build under way, the
//! artifacts those rows name, the change the checks read, as a summary of files and counts, and the commit the checkout is
//! on now.
//!
//! **What it reads.** The store's read connection for the rows, and read-only git on the task's own checkout for the change
//! (see [`changed_since`]) and for the commit it is on now (see [`checkout_head`]): three read-only git calls, no patch, no
//! write to the repository, no lock beyond the read. It is the one Flow read that runs a program, because a diff and a
//! checkout's head are not stored.
//!
//! **What it never returns.** The answer key or any part of a check beyond its id and kind: not its command, its output, what
//! it proves, and not an Evidence id. The task's key is read for its checks' ids and kinds only, to label a row, which is the
//! one place a Flow read opens a key at all; every text is redacted, the key's text withheld and the length cut
//! ([`scrub`](super::scrub)). Nothing here writes, approves, merges or reverts.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use orbyn_contracts as wire;
use orbyn_contracts::Evidence;
use tokio::time::timeout;

use super::facts::Facts;
use super::scrub::Scrub;
use crate::app::Core;
use crate::crew::valid_id;
use crate::error::{CoreError, Result};

/// The most evidence rows one task shows; a build's checks are a handful in practice.
pub const CHECKS_MAX: usize = 64;
/// The most files the diff summary lists. A change with more is cut and says so.
pub const DIFF_FILES_MAX: usize = 200;
/// How long the change may spend in git before the read gives up and says it could not read it. A checkout held by another
/// process, such as a long `git gc` or a lock a worker left, must not hold this GET open: the caller asks again.
const GIT_READ_MAX: Duration = Duration::from_secs(10);
/// The longest path, and the longest artifact name, the read shows in characters.
const PATH_MAX: usize = 400;
const NAME_MAX: usize = 120;

/// One file that changed, before it is put in the view.
struct Change {
    files: Vec<wire::FlowDiffFile>,
    /// Files the change touched, before the cap.
    total: u32,
    /// Lines added over every changed file, before the cap.
    added: u32,
    /// Lines removed over every changed file, before the cap.
    removed: u32,
}

impl Core {
    /// One task's checks, the artifacts they named, the change they read and the commit the checkout is on now.
    pub async fn crew_flow_task_artifacts(&self, id: &str) -> Result<wire::FlowTaskArtifacts> {
        self.crew_preview_only()?;
        valid_id(id)?;
        let task = self
            .store
            .crew_task(id)?
            .filter(|_| self.store.crew_task_is_work(id).unwrap_or(false))
            .ok_or_else(|| CoreError::NotFound("task".into()))?;
        let person = self.display_redactor()?;
        let facts = self.flow_facts(task)?;
        let scrub = Scrub::of_crew(self, &person, &facts.task.crew_id)?;
        // The task that is building now: the latest fix when a finding sent one, else the task itself. Its rows are the ones
        // the checks bar counts, and the newest of them is the verification that decided the task.
        let building = facts
            .fixes
            .last()
            .map_or_else(|| facts.task.id.clone(), |(fix, _)| fix.fix_task_id.clone());
        let kinds = self.check_kinds(&building)?;
        let mut checks: Vec<wire::FlowCheck> = facts
            .build_evidence
            .iter()
            .map(|row| flow_check(row, &building, &kinds, &scrub))
            .collect();
        let truncated = checks.len() > CHECKS_MAX;
        checks.truncate(CHECKS_MAX);
        let (diff, diff_absent) = diff_of(&facts, &scrub).await;
        let (head, head_absent) = head_of(&facts).await;
        Ok(wire::FlowTaskArtifacts {
            crew: facts.task.crew_id.clone(),
            task: facts.task.id.clone(),
            checks,
            diff,
            diff_absent,
            head,
            head_absent,
            truncated,
        })
    }

    /// The kind of each of a task's checks, by the check's id. The key is read for these two fields only.
    fn check_kinds(&self, task_id: &str) -> Result<HashMap<String, wire::CheckKind>> {
        Ok(self
            .store
            .crew_answer_key(task_id)?
            .map(|key| {
                key.checks
                    .into_iter()
                    .map(|check| (check.id.as_str().to_owned(), check.kind))
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// One stored row as the checks bar draws it: which check, how it went, on which commit, and what it named.
fn flow_check(
    row: &Evidence,
    task: &str,
    kinds: &HashMap<String, wire::CheckKind>,
    scrub: &Scrub,
) -> wire::FlowCheck {
    wire::FlowCheck {
        id: row.check.as_str().to_owned(),
        kind: kinds.get(row.check.as_str()).copied(),
        task: task.to_owned(),
        outcome: row.outcome,
        exit_code: row.exit_code,
        duration_ms: row.duration_ms,
        commit: row.commit.as_str().to_owned(),
        artifacts: row
            .artifacts
            .iter()
            .map(|artifact| wire::FlowArtifact {
                kind: artifact.kind,
                name: artifact
                    .name
                    .as_ref()
                    .map(|name| scrub.text(name.as_str(), NAME_MAX)),
                hash: artifact.hash.as_str().to_owned(),
            })
            .collect(),
    }
}

/// The task's checkout on this machine, when it has one: the run's worktree, which is a directory now. No run, no worktree or
/// a worktree that is gone is no checkout.
fn checkout_root(facts: &Facts) -> Option<&Path> {
    let checkout = facts.run.as_ref()?.worktree.as_deref()?;
    let root = Path::new(checkout);
    root.is_dir().then_some(root)
}

/// The change the checks read, or the reason there is none. A task with no checkout, no base commit or a checkout git cannot
/// read still answers: its rows are there, and this names what is missing.
async fn diff_of(facts: &Facts, scrub: &Scrub) -> (Option<wire::FlowTaskDiff>, Option<String>) {
    let Some(root) = checkout_root(facts) else {
        return (None, Some("no_checkout".to_owned()));
    };
    let Some(base) = facts.run.as_ref().and_then(|run| run.base_commit.as_deref()) else {
        return (None, Some("no_base".to_owned()));
    };
    match changed_since(root, base, scrub).await {
        Ok(change) => (
            Some(wire::FlowTaskDiff {
                base: base.to_owned(),
                truncated: change.total > u32::try_from(change.files.len()).unwrap_or(u32::MAX),
                files: change.files,
                files_total: change.total,
                added: change.added,
                removed: change.removed,
            }),
            None,
        ),
        Err(_) => (None, Some("unreadable".to_owned())),
    }
}

/// The commit the checkout is on now, or the reason it is not known. An artifact and a diff are pinned to a commit; this is
/// the commit that shows they are stale, because it is the one the checkout has moved on to.
async fn head_of(facts: &Facts) -> (Option<String>, Option<String>) {
    let Some(root) = checkout_root(facts) else {
        return (None, Some("no_checkout".to_owned()));
    };
    match checkout_head(root).await {
        Ok(head) => (Some(head), None),
        Err(_) => (None, Some("unreadable".to_owned())),
    }
}

/// `git rev-parse --verify --quiet HEAD`: read-only, byte exact, and bounded like the diff so a checkout another process holds
/// cannot hold this GET open. A checkout whose head is not a commit reads as one git cannot give.
async fn checkout_head(root: &Path) -> Result<String> {
    use crate::source_control::git::git_byte_exact as git;
    let head = timeout(
        GIT_READ_MAX,
        git(root, &["rev-parse", "--verify", "--quiet", "HEAD"]),
    )
    .await
    .map_err(|_| CoreError::Git("The checkout's head took too long to read.".into()))??;
    let head = String::from_utf8_lossy(&head).trim().to_owned();
    if !crate::source_control::git::is_revision(&head) {
        return Err(CoreError::Git(
            "The checkout has no commit to be on.".into(),
        ));
    }
    Ok(head)
}

/// The files the checkout changed since `base`, as counts: `git diff` by status and by number, both read-only and both
/// byte exact, and no patch. A file the task left untracked is not one git tracks, so it is not listed.
async fn changed_since(root: &Path, base: &str, scrub: &Scrub) -> Result<Change> {
    use crate::source_control::git::git_byte_exact as git;
    if !crate::source_control::git::is_revision(base) {
        return Err(CoreError::Invalid("That is not a commit.".into()));
    }
    // A rename is read as a deletion and an addition, so every row is one path and the counts stay unambiguous.
    let flags = ["--no-renames", "--no-ext-diff", "--no-textconv"];
    let mut statuses_args = vec!["diff"];
    statuses_args.extend(flags);
    statuses_args.extend(["--name-status", "-z", base]);
    let statuses = timeout(GIT_READ_MAX, git(root, &statuses_args))
        .await
        .map_err(|_| CoreError::Git("The change took too long to read.".into()))??;
    let mut counts_args = vec!["diff"];
    counts_args.extend(flags);
    counts_args.extend(["--numstat", "-z", base]);
    let counts = timeout(GIT_READ_MAX, git(root, &counts_args))
        .await
        .map_err(|_| CoreError::Git("The change took too long to read.".into()))??;
    let counts = line_counts(&counts);
    let mut change = Change {
        files: Vec::new(),
        total: 0,
        added: 0,
        removed: 0,
    };
    for (path, status) in name_statuses(&statuses) {
        let (added, removed) = counts.get(&path).copied().unwrap_or((0, 0));
        change.total = change.total.saturating_add(1);
        change.added = change.added.saturating_add(added);
        change.removed = change.removed.saturating_add(removed);
        if change.files.len() < DIFF_FILES_MAX {
            change.files.push(wire::FlowDiffFile {
                path: scrub.text(&path, PATH_MAX),
                status: status.to_owned(),
                added,
                removed,
            });
        }
    }
    Ok(change)
}

/// `git diff --name-status -z`: a status byte, then the path. A rename is read as a deletion and an addition, so every entry
/// is one pair.
fn name_statuses(bytes: &[u8]) -> Vec<(String, &'static str)> {
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut entries = Vec::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            continue;
        }
        let Some(path) = fields.next() else {
            break;
        };
        let status = match status.first() {
            Some(b'A') => "added",
            Some(b'D') => "deleted",
            _ => "modified",
        };
        entries.push((String::from_utf8_lossy(path).into_owned(), status));
    }
    entries
}

/// `git diff --numstat -z`: added, removed and the path, per file. A binary file's counts are `-`, which is no count.
fn line_counts(bytes: &[u8]) -> HashMap<String, (u32, u32)> {
    let mut counts = HashMap::new();
    for field in bytes.split(|byte| *byte == 0) {
        let row = String::from_utf8_lossy(field);
        let mut parts = row.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        counts.insert(
            path.to_owned(),
            (added.parse().unwrap_or(0), removed.parse().unwrap_or(0)),
        );
    }
    counts
}
