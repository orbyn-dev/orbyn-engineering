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
//! is cleared by the next start for that member instead of blocking every retry forever.

use std::path::{Path, PathBuf};

use orbyn_contracts::{Brief, CrewError, CrewMember, Thread, Worktree};

use super::leases::internal;
use super::tool::invalid;
use crate::app::Core;
use crate::error::Result;
use crate::source_control::branch_names::{Naming, NamingMode};
use crate::source_control::git;

/// The purpose a writing worker's worktree is made under: `crew-<member>`, so it cannot take a
/// purpose a person's thread would use. A member id is lowercase letters, digits and hyphens
/// already; anything else is replaced and the name cut to what git accepts, so an unusual member
/// cannot fail a start on its folder's name.
fn purpose(member: &CrewMember) -> String {
    let mut purpose = String::from("crew-");
    for character in member.as_str().chars() {
        let kept = character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-';
        purpose.push(if kept { character } else { '-' });
    }
    purpose.truncate(64);
    purpose.trim_end_matches('-').to_owned()
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
    ) -> Result<Option<Worktree>, CrewError> {
        if brief.write_paths().next().is_none() {
            return Ok(None);
        }
        let project = self.store.project(&lead.project_id).map_err(internal)?;
        let repo = PathBuf::from(&project.path);
        let purpose = purpose(member);
        let primary = git::primary_checkout(&repo).await.map_err(internal)?;
        let target = git::worktree_path(&primary, &purpose).map_err(internal)?;
        // A crashed attempt can leave a folder git no longer lists under Orbyn's own `<repo>-wt`
        // root, and `create_worktree` refuses a path that exists. The folder carries Orbyn's own
        // name in Orbyn's own root, so it is cleared instead of blocking every retry.
        if target.exists() && !listed(&repo, &target).await.map_err(internal)? {
            std::fs::remove_dir_all(&target).map_err(|why| {
                invalid(format!(
                    "A folder in the way of the worker's worktree could not be removed: {why}"
                ))
            })?;
        }
        let mut options = self
            .worktree_options_for(&project.id, base, None)
            .map_err(internal)?;
        // A worker's branch is the crew's own: `crew-<member>`, never the person's
        // `branch_name_prefix`, and never the mode that reads a person's title.
        options.naming = Naming {
            mode: NamingMode::Custom,
            prefix: String::new(),
            custom: None,
        };
        options.seed = None;
        // A branch an earlier attempt left with no worktree of its own — a start that failed and
        // was cleaned up, or a person removing the folder — is checked out again instead of
        // refused, and nothing is deleted: the member's branch is reused, as a registered
        // worktree at the path is.
        if !target.exists() && branch_exists(&repo, &purpose).await.map_err(internal)? {
            let path = target.to_string_lossy().into_owned();
            git::git(&repo, &["worktree", "add", &path, &purpose])
                .await
                .map_err(internal)?;
            let start = git::git(&repo, &["merge-base", "HEAD", &purpose])
                .await
                .map_err(internal)?;
            return Ok(Some(Worktree {
                path,
                branch: purpose,
                base: text(&start),
            }));
        }
        let worktree = git::create_worktree(&repo, &purpose, &options)
            .await
            .map_err(internal)?;
        Ok(Some(worktree))
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
        let Ok(root) = git::worktree_root(&primary) else {
            return;
        };
        // Git's spelling of a path differs on Windows, so both sides are canonicalized first.
        let (Ok(root), Ok(wanted)) = (
            crate::util::canonicalize(&root),
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
    }
}

/// Git's answer as text, trimmed.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

/// Whether the repository already has the local branch `name`.
async fn branch_exists(repo: &Path, name: &str) -> Result<bool> {
    let reference = format!("refs/heads/{name}");
    Ok(
        git::git(repo, &["rev-parse", "--verify", "--quiet", &reference])
            .await
            .is_ok(),
    )
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
