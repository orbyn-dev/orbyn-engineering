//! A writing worker's own worktree (issue #436).
//!
//! A brief that writes gets a checkout of its own under the sibling `<repo>-wt` root Orbyn already
//! uses for every worktree, so a change in the lead's checkout is not the worker's; a brief that
//! writes nothing keeps the lead's checkout (docs/crew.md, "Isolation"). The worktree is made before
//! the worker's thread and removed when that start fails, so a refused start leaves no checkout
//! behind.
//!
//! It is one folder per crew member, `<repo>-wt/crew-<member>`, on branch `crew-<member>`: a member
//! that works a second task reuses its own checkout (git's own rule for a registered worktree at the
//! path), so the folders cannot pile up one per task. A folder git no longer lists, left by a crash,
//! is cleared by the next start for that member instead of blocking every retry forever, and so is the
//! registration a hand-deleted folder leaves behind: git refuses an add at a path a missing worktree
//! still holds. Only a folder Orbyn's own record says it made is cleared that way; any other folder is
//! kept and the start says what is in the way. A branch that another checkout already holds is reused
//! there, because git keeps one checkout for each branch.

use std::path::{Path, PathBuf};

use orbyn_contracts::{Brief, CrewError, CrewMember, Thread, Worktree};

use super::leases::internal;
use super::tool::invalid;
use crate::app::Core;
use crate::error::Result;
use crate::source_control::git;

/// The purpose a writing worker's worktree is made under: `crew-<member>`, so it cannot take a
/// purpose a person's thread would use. The member's id is kept whole, so two ids never name one
/// checkout: an id that adds a character git's name cannot hold, or one longer than the name, would
/// otherwise be given a name another id also gets, and two workers would write in one folder and on
/// one branch.
fn purpose(member: &CrewMember) -> Result<String, CrewError> {
    const PREFIX: &str = "crew-";
    let name = format!("{PREFIX}{}", member.as_str());
    if !git::valid_purpose(&name) {
        return Err(invalid(format!(
            "The member {} cannot name a writer's checkout: use lowercase letters, digits and \
             hyphens, at most {} characters.",
            member.as_str(),
            git::MAX_PURPOSE_CHARS - PREFIX.len()
        )));
    }
    Ok(name)
}

impl Core {
    /// The worktree a writing worker runs in, or `None` for a brief whose lease writes nothing,
    /// which keeps a read-only worker in the lead's checkout. It starts at `base` when the start
    /// named a commit, and this clone's HEAD otherwise, as every other worktree does.
    pub(super) async fn crew_writer_worktree(
        &self,
        lead: &Thread,
        brief: &Brief,
        member: &CrewMember,
        base: Option<&str>,
    ) -> Result<Option<WriterWorktree>, CrewError> {
        if brief.write_paths().next().is_none() {
            return Ok(None);
        }
        let project = self.store.project(&lead.project_id).map_err(internal)?;
        let repo = PathBuf::from(&project.path);
        let purpose = purpose(member)?;
        let primary = git::primary_checkout(&repo).await.map_err(internal)?;
        let target = git::worktree_path(&primary, &purpose).map_err(internal)?;
        let root = git::worktree_root(&primary).map_err(internal)?;
        // A folder a person deleted by hand keeps its registration, and git then refuses any add at
        // that path ("missing but already registered worktree"), so every retry would fail the same
        // way. Only this member's stale registration goes; a folder that still exists, and a locked
        // registration, are left as they are.
        git::clear_missing_worktree(&repo, &target).await;
        let options = self
            .worktree_options_for(&project.id, base, None)
            .map_err(internal)?;
        // Git keeps one checkout for each branch, so a checkout that holds the member's branch is the
        // worker's wherever it is: it is reused where it is and keeps what it holds, and a start that
        // fails leaves it alone.
        let branch = branch_exists(&repo, &purpose).await.map_err(internal)?;
        let checkout = match branch {
            true => checkout_of(&repo, &purpose).await.map_err(internal)?,
            false => None,
        };
        if let Some(checkout) = &checkout
            && checkout.path.exists()
        {
            let start = self.member_base(&repo, &purpose, base).await?;
            return Ok(Some(WriterWorktree {
                worktree: Worktree {
                    path: checkout.path.to_string_lossy().into_owned(),
                    branch: purpose,
                    base: start,
                },
                created: false,
            }));
        }
        // A folder git no longer lists is cleared only when Orbyn's own record says it made one here.
        // Without that record the folder can hold a person's work, so it is kept and the start says
        // what is in the way instead of deleting it.
        if target.exists() && !listed(&repo, &target).await.map_err(internal)? {
            if !made_by_orbyn(&root, &purpose) {
                return Err(invalid(format!(
                    "The folder {} is in the way of the worker's worktree and Orbyn did not make it; \
                     move or remove it yourself.",
                    target.display()
                )));
            }
            std::fs::remove_dir_all(&target).map_err(|why| {
                invalid(format!(
                    "A folder in the way of the worker's worktree could not be removed: {why}"
                ))
            })?;
        }
        let path = target.to_string_lossy().into_owned();
        if branch {
            let start = self.member_base(&repo, &purpose, base).await?;
            // The member's branch is no checkout's, because a person removed the folder by hand and
            // git kept the registration it calls prunable: that registration goes, and the branch is
            // taken back here instead of refused.
            if let Some(checkout) = &checkout {
                git::clear_missing_worktree(&repo, &checkout.path).await;
            }
            record_made(&root, &purpose, &target)?;
            if let Err(error) = git::git(&repo, &["worktree", "add", &path, &purpose]).await {
                forget_made(&root, &purpose);
                return Err(internal(error));
            }
            // A recovered checkout is populated like a new one, so a worker in it sees the same files.
            git::populate_submodules(&target, &options.submodules).await;
            return Ok(Some(WriterWorktree {
                worktree: Worktree {
                    path,
                    branch: purpose,
                    base: start,
                },
                created: true,
            }));
        }
        // No branch of its own yet, so the folder must be free: one git still lists here is another
        // checkout on another branch, and it is never taken for the worker's.
        if target.exists() {
            return Err(invalid(format!(
                "{} is another checkout, not this member's worktree; move or remove it yourself.",
                target.display()
            )));
        }
        // A worker's branch is the crew's own: `crew-<member>`, never the person's
        // `branch_name_prefix`, and never the mode that reads a person's title.
        let start = match git::worktree_start(&repo, &options).await {
            Some(start) => start,
            None => text(
                &git::git(&repo, &["rev-parse", "HEAD"])
                    .await
                    .map_err(internal)?,
            ),
        };
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|why| {
                invalid(format!(
                    "The folder of the worker's worktree could not be made: {why}"
                ))
            })?;
        }
        record_made(&root, &purpose, &target)?;
        if let Err(error) =
            git::git(&repo, &["worktree", "add", "-b", &purpose, &path, &start]).await
        {
            forget_made(&root, &purpose);
            return Err(internal(error));
        }
        git::populate_submodules(&target, &options.submodules).await;
        Ok(Some(WriterWorktree {
            worktree: Worktree {
                path,
                branch: purpose,
                base: start,
            },
            created: true,
        }))
    }

    /// The base a run records for a member's checkout when this start admitted no commit of its own:
    /// the admitted commit when the task named one, else where the member's branch forked from this
    /// clone's HEAD, which is the commit the worker's work began at.
    async fn member_base(
        &self,
        repo: &Path,
        purpose: &str,
        base: Option<&str>,
    ) -> Result<String, CrewError> {
        match base {
            Some(base) => Ok(base.to_owned()),
            None => Ok(text(
                &git::git(repo, &["merge-base", "HEAD", purpose])
                    .await
                    .map_err(internal)?,
            )),
        }
    }

    /// Removes a writing worker's worktree when its start failed, so a refused start leaves no
    /// checkout behind. A forced `git worktree remove` takes the folder even from a worker that had
    /// written in it; the branch is left, as the explorer's own worktree removal leaves it. Only a
    /// folder under this project's own `<repo>-wt` root is ever removed.
    pub(super) async fn discard_crew_worktree(&self, lead: &Thread, path: &Path) {
        let Ok(project) = self.store.project(&lead.project_id) else {
            return;
        };
        let repo = PathBuf::from(&project.path);
        let Ok(primary) = git::primary_checkout(&repo).await else {
            return;
        };
        let Ok(owner_root) = git::worktree_root(&primary) else {
            return;
        };
        // Git's spelling of a path differs on Windows, so both sides are canonicalized first.
        let (Ok(root), Ok(wanted)) = (
            crate::util::canonicalize(&owner_root),
            crate::util::canonicalize(path),
        ) else {
            return;
        };
        if !wanted.starts_with(&root) {
            return;
        }
        let text = wanted.to_string_lossy().into_owned();
        if let Err(error) = git::git(&repo, &["worktree", "remove", "--force", &text]).await {
            tracing::warn!(%error, "could not remove a crew writer's worktree");
        }
        if wanted.exists()
            && let Err(error) = std::fs::remove_dir_all(&wanted)
        {
            tracing::warn!(%error, "could not remove a crew writer's worktree folder");
        }
        // The folder is gone, so the record that says Orbyn made it has nothing left to describe.
        if !wanted.exists()
            && let Some(name) = wanted.file_name()
        {
            forget_made(&owner_root, &name.to_string_lossy());
        }
    }
}

/// Where the record that Orbyn made a member's folder is kept: a folder beside the worktrees it
/// describes, `<repo>-wt/.orbyn/<purpose>`. Names Orbyn gives are lowercase letters, digits and
/// hyphens, so no worktree folder can be mistaken for it.
fn record_dir(root: &Path) -> PathBuf {
    root.join(".orbyn")
}

fn record_path(root: &Path, purpose: &str) -> PathBuf {
    record_dir(root).join(purpose)
}

/// Writes the record before the folder is made, so a crash during `git worktree add` still leaves
/// the proof the next start clears the folder with.
fn record_made(root: &Path, purpose: &str, made: &Path) -> Result<(), CrewError> {
    std::fs::create_dir_all(record_dir(root)).map_err(|why| {
        invalid(format!(
            "The record of the worker's worktree could not be prepared: {why}"
        ))
    })?;
    std::fs::write(
        record_path(root, purpose),
        made.to_string_lossy().as_bytes(),
    )
    .map_err(|why| {
        invalid(format!(
            "The record of the worker's worktree could not be written: {why}"
        ))
    })
}

/// Forgets the record once the folder it describes is gone.
fn forget_made(root: &Path, purpose: &str) {
    if let Err(error) = std::fs::remove_file(record_path(root, purpose))
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "could not remove the record of a crew writer's worktree");
    }
}

/// Whether Orbyn's own record says it made this member's folder.
fn made_by_orbyn(root: &Path, purpose: &str) -> bool {
    record_path(root, purpose).is_file()
}

/// Git's answer as text, trimmed.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

/// The checkout a writing worker runs in, and whether this start made it: an earlier attempt's or
/// another task's registered checkout is reused, and a start that fails then leaves it alone.
#[derive(Debug)]
pub(super) struct WriterWorktree {
    pub worktree: Worktree,
    /// `true` when this start created the folder, so only that one is removed on a failed start.
    pub created: bool,
}

/// Whether the repository already has the local branch `name`. `git rev-parse --verify --quiet`
/// answers exit code 1 for a branch that is not there; any other failure is a broken repository and is
/// reported instead of being read as "no branch", which would make the caller try to create one.
async fn branch_exists(repo: &Path, name: &str) -> Result<bool> {
    let reference = format!("refs/heads/{name}");
    let output = git::git_raw(repo, &["rev-parse", "--verify", "--quiet", &reference]).await?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(crate::error::CoreError::Git(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        )),
    }
}

/// The registered checkout `branch` is on, if there is one: git keeps one checkout for each branch,
/// so a branch another checkout holds is that checkout's to keep.
async fn checkout_of(repo: &Path, branch: &str) -> Result<Option<git::RegisteredWorktree>> {
    Ok(git::linked_worktrees(repo)
        .await?
        .into_iter()
        .find(|worktree| worktree.branch.as_deref() == Some(branch)))
}

/// Whether git still lists `path` as a worktree of `repo`. Git spells a path differently on Windows,
/// so the canonicalized paths are compared too.
async fn listed(repo: &Path, path: &Path) -> Result<bool> {
    let wanted = crate::util::canonicalize(path).ok();
    Ok(git::linked_worktrees(repo).await?.iter().any(|worktree| {
        worktree.path == path
            || (wanted.is_some() && crate::util::canonicalize(&worktree.path).ok() == wanted)
    }))
}
