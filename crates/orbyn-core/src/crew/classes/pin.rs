//! The default-branch commit a project remembers (issue #574). A task reads the rules at the default branch's tip; when that
//! ref disappears or moves somewhere that is not a descendant of the commit a task accepted earlier, HEAD alone would be a
//! weaker read, so the task blocks every engine until a person confirms the new commit or resets the memory.
//!
//! The memory lives in the existing `project_preferences` table (no migration) under a key no setting reads. A normal
//! fast-forward moves it in the same start that read it, so a later rewind is compared to the newest accepted commit, never an
//! older one. A person's confirm or reset is a local-only route listed in `changes::STATE_CHANGING` (Autopilot's N1 refuses an
//! agent's request to it), and no MCP tool reaches either. A worker that can write the store could still rewrite the row, as it
//! can the answer-key table until #441; that is a known limit of Preview, not an authority this code grants.

use std::path::{Path, PathBuf};

use orbyn_contracts as wire;

use super::classify::redacted_ref;
use super::file::{ClassFile, Pinned, PinnedRef, read_pinned};
use crate::app::Core;
use crate::crew::valid_id;
use crate::error::{CoreError, Result};
use crate::source_control::git::git_hardened as git;
use crate::store::{CrewPinnedRef, Store};

/// How the checkout's default branch compares to the commit this project remembered.
pub(in crate::crew) enum Decision {
    /// No ref and nothing remembered: HEAD alone is read, as before.
    None,
    /// The ref is the remembered commit or a descendant. `write` moves the memory when the start records it.
    Read {
        pin: PinnedRef,
        file: super::file::ClassFile,
        write: Option<CrewPinnedRef>,
    },
    /// `origin` is configured but its default branch cannot be read; the file itself blocks every engine.
    Unreadable {
        pin: PinnedRef,
        file: super::file::ClassFile,
    },
    /// A ref was remembered and is gone, or the current one does not descend from it. A person must confirm or reset.
    /// `current` is the ref the checkout reads now when there is one to confirm, even though it is not accepted yet.
    Blocked {
        name: String,
        commit: String,
        reason: String,
        missing: bool,
        current: Option<(String, String)>,
    },
}

/// The remembered default branch. The store checks it as it reads the row (a row that does not parse, or a commit that is not a
/// full object id, is `CoreError::StoredDataDamaged`), so no caller can compare against a broken pin. Callers fail closed on
/// the error.
pub(super) fn stored(store: &Store, project_id: &str) -> Result<Option<CrewPinnedRef>> {
    store.crew_pinned_ref(project_id)
}

/// What the checkout's default branch says now, against the commit last accepted for this project. Reads only.
pub(in crate::crew) async fn decide(root: &Path, stored: Option<&CrewPinnedRef>) -> Decision {
    match read_pinned(root).await {
        Pinned::Absent => match stored {
            Some(stored) => Decision::Blocked {
                name: stored.reference.clone(),
                commit: stored.commit.clone(),
                reason: missing_reason(stored),
                missing: true,
                current: None,
            },
            None => Decision::None,
        },
        Pinned::Unreadable(pin, file) => Decision::Unreadable { pin, file },
        Pinned::Read(pin, file) => {
            let Some(stored) = stored else {
                return Decision::Read {
                    write: Some(remembered(&pin)),
                    pin,
                    file,
                };
            };
            // A remote-tracking ref is what `git fetch` moves; a local branch is moved by a commit or `git branch -f`, which an
            // agent can do. Losing `origin/...` for a local branch is the weakening this memory exists to see, so the pin is
            // gone even when the local branch happens to sit on a descendant: a person confirms or resets it.
            if stored.remote && !pin.remote {
                return Decision::Blocked {
                    name: stored.reference.clone(),
                    commit: stored.commit.clone(),
                    reason: missing_reason(stored),
                    missing: true,
                    current: Some((pin.name, pin.commit)),
                };
            }
            if pin.commit == stored.commit {
                // A ref renamed at the same commit is no weaker: it reads the same tree.
                let write = (pin.name != stored.reference).then(|| remembered(&pin));
                return Decision::Read { pin, file, write };
            }
            if is_ancestor(root, &stored.commit, &pin.commit).await {
                return Decision::Read {
                    write: Some(remembered(&pin)),
                    pin,
                    file,
                };
            }
            Decision::Blocked {
                reason: rewritten_reason(&pin.name, &stored.commit, &pin.commit),
                name: pin.name,
                commit: pin.commit,
                missing: false,
                current: None,
            }
        }
    }
}

fn remembered(pin: &PinnedRef) -> CrewPinnedRef {
    CrewPinnedRef {
        reference: pin.name.clone(),
        commit: pin.commit.clone(),
        remote: pin.remote,
    }
}

/// Whether `older` is part of `newer`'s history. Any git failure (a missing object after a prune, an object a replace ref
/// tries to fake) counts as "no": the task blocks, never reads HEAD alone.
async fn is_ancestor(root: &Path, older: &str, newer: &str) -> bool {
    git(root, &["merge-base", "--is-ancestor", older, newer])
        .await
        .is_ok()
}

fn short(commit: &str) -> String {
    commit.chars().take(12).collect()
}

fn missing_reason(stored: &CrewPinnedRef) -> String {
    format!(
        "Orbyn remembered the default branch {} at {}. It is gone now, so this task may not read HEAD alone: fetch it again, or ask a person to reset the remembered default branch.",
        redacted_ref(&stored.reference),
        short(&stored.commit)
    )
}

fn rewritten_reason(name: &str, older: &str, newer: &str) -> String {
    format!(
        "the default branch {} now points at {}, which does not descend from the commit Orbyn remembered, {}. Ask a person to confirm the new default branch before a task starts.",
        redacted_ref(name),
        short(newer),
        short(older)
    )
}

/// What a read-only view shows of the default branch, decided against the project's memory but never written.
pub(super) enum Shown {
    /// No ref and nothing remembered: HEAD alone is read.
    None,
    /// The ref and the file at it, for the view to compare with the checkout's copy.
    Side { pin: PinnedRef, file: ClassFile },
    /// A remembered ref is gone or does not descend from the current one: a person must confirm or reset.
    Blocked {
        name: String,
        commit: String,
        reason: String,
    },
}

/// The decision a view shows. A broken stored row is an error, never "nothing remembered".
pub(super) async fn show(store: &Store, project_id: &str, root: &Path) -> Result<Shown> {
    let stored = stored(store, project_id)?;
    Ok(match decide(root, stored.as_ref()).await {
        Decision::None => Shown::None,
        Decision::Read { pin, file, .. } | Decision::Unreadable { pin, file } => {
            Shown::Side { pin, file }
        }
        Decision::Blocked {
            name,
            commit,
            reason,
            ..
        } => Shown::Blocked {
            name,
            commit,
            reason,
        },
    })
}

impl Core {
    fn pin_root(&self, project_id: &str) -> Result<PathBuf> {
        self.crew_preview_only()?;
        valid_id(project_id)?;
        let project = self.store.project(project_id)?;
        if !project.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        Ok(PathBuf::from(project.path))
    }

    /// `GET /api/crew/projects/{id}/pinned-ref`: the default branch the checkout reads now, the one the project remembered,
    /// and whether a start would accept it without a person.
    pub async fn crew_pinned_ref_view(&self, project_id: &str) -> Result<wire::CrewPinnedRefView> {
        let root = self.pin_root(project_id)?;
        let stored = stored(&self.store, project_id)?;
        let decision = decide(&root, stored.as_ref()).await;
        let (current_ref, current_commit, accepted, missing, blocked) = match decision {
            Decision::None => (None, None, true, false, None),
            Decision::Read { pin, .. } => (
                Some(redacted_ref(&pin.name)),
                Some(pin.commit),
                true,
                false,
                None,
            ),
            Decision::Unreadable { file, .. } => {
                (None, None, false, false, Some(file.problem_text()))
            }
            Decision::Blocked {
                name,
                commit,
                reason,
                missing,
                current,
            } => {
                let current = current.or_else(|| (!missing).then_some((name, commit)));
                (
                    current.as_ref().map(|(name, _)| redacted_ref(name)),
                    current.map(|(_, commit)| commit),
                    false,
                    missing,
                    Some(reason),
                )
            }
        };
        Ok(wire::CrewPinnedRefView {
            project: project_id.to_owned(),
            current_ref,
            current_commit,
            stored_ref: stored.as_ref().map(|pin| redacted_ref(&pin.reference)),
            stored_commit: stored.as_ref().map(|pin| pin.commit.clone()),
            accepted,
            missing,
            blocked,
        })
    }

    /// `POST /api/crew/projects/{id}/pinned-ref/confirm`: a person accepts the default branch's new commit. The body must name
    /// the whole commit the view showed, so a ref that moved again is refused as stale rather than confirmed unseen.
    pub async fn crew_confirm_pinned_ref(
        &self,
        project_id: &str,
        commit: &str,
    ) -> Result<wire::CrewPinnedRefView> {
        let root = self.pin_root(project_id)?;
        let _one_at_a_time = self.crew_starts.lock().await;
        let pin = match read_pinned(&root).await {
            Pinned::Read(pin, _) => pin,
            Pinned::Absent => {
                return Err(CoreError::Invalid(
                    "there is no default-branch ref to confirm; reset the remembered one instead"
                        .into(),
                ));
            }
            Pinned::Unreadable(_, file) => return Err(CoreError::Invalid(file.problem_text())),
        };
        if pin.commit != commit {
            return Err(CoreError::Invalid(
                "the default branch is not the commit that was shown; reload and confirm the commit it has now"
                    .into(),
            ));
        }
        self.store
            .confirm_crew_pinned_ref(project_id, &remembered(&pin), "user")?;
        drop(_one_at_a_time);
        self.crew_pinned_ref_view(project_id).await
    }

    /// `POST /api/crew/projects/{id}/pinned-ref/reset`: a person clears what was remembered. A later task reads the ref the
    /// checkout has then, or HEAD alone when it has none.
    pub async fn crew_reset_pinned_ref(&self, project_id: &str) -> Result<wire::CrewPinnedRefView> {
        self.pin_root(project_id)?;
        let _one_at_a_time = self.crew_starts.lock().await;
        self.store.reset_crew_pinned_ref(project_id, "user")?;
        drop(_one_at_a_time);
        self.crew_pinned_ref_view(project_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reasons_name_the_ref_and_both_commits() {
        let stored = CrewPinnedRef {
            reference: "origin/main".into(),
            commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            remote: true,
        };
        let missing = missing_reason(&stored);
        assert!(missing.contains("origin/main") && missing.contains("aaaaaaaaaaaa"));
        let moved = rewritten_reason(
            "origin/main",
            &stored.commit,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        );
        assert!(
            moved.contains("origin/main")
                && moved.contains("bbbbbbbbbbbb")
                && moved.contains("aaaaaaaaaaaa")
        );
    }

    #[test]
    fn a_ref_name_in_a_reason_is_redacted() {
        let token = format!("ghp_{}", "q".repeat(36));
        let stored = CrewPinnedRef {
            reference: format!("origin/{token}"),
            commit: "a".repeat(40),
            remote: true,
        };
        assert!(!missing_reason(&stored).contains(&token));
        assert!(
            !rewritten_reason(&stored.reference, &stored.commit, &"b".repeat(40)).contains(&token)
        );
    }
}
