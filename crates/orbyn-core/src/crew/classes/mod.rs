//! Data classes (docs/crew.md, "As built (slice 4i, routing)"): `.orbyn/crew.json` says which engines may take which paths.
//! The file is read at two places through hardened git, never from the checkout's files, and the stricter of the two applies: the
//! checkout's HEAD when a task starts (the task keeps that commit: an edit made after the start has no effect on it), and the
//! project's default branch as of the last fetch (`refs/remotes/origin/HEAD`, else `origin/main`, `origin/master` or `origin/trunk`,
//! else, with no `origin` remote, a local `init.defaultBranch`, `main`, `master` or `trunk`; with none of them HEAD alone is read, as before). An engine must
//! be allowed by both, a file either side cannot read blocks every engine, and the route card says when the two files differ.
//! An agent that moves HEAD to a commit without the file, or commits a looser one, therefore does not loosen the next task. The
//! default branch is not out of an agent's reach: a remote-tracking ref moves by fetch or pull, a local branch by a commit on it, and
//! either by git plumbing, `git remote set-head`, `remote rename|remove|set-url` or an edit of `.git`. Autopilot's N7 holds those
//! commands, but it reads command text only; the macOS confinement of #441 does not close the repository's `.git`, and Windows and
//! Linux workers are refused. A project with an `origin` remote and no resolvable remote-tracking default-branch ref blocks every
//! engine under its own rule name, `default branch unreadable`: no local branch stands in, because an agent could have deleted the
//! ref. A person fixes it with `git fetch origin` (a brand-new project with an empty remote is blocked until its first push, by
//! design) or `git remote set-head origin --auto`. With no `origin` at all the local default branch, else HEAD alone, is read.
//! Each project remembers the default-branch ref and commit its last started task accepted (the existing `project_preferences`
//! row `crew_pinned_ref`, no migration): the same commit or a descendant moves the memory forward, while a ref that is gone or a
//! move that does not descend blocks every engine under `default branch changed` until a person confirms the new commit or resets
//! the memory. A dry run never writes it. See the known limits in docs/crew.md.
//! The router uses the result as a hard filter before scoring
//! ([`class_screen`](super::filters::screen)); each routing writes one `route_class` row to the approval ledger; and the read-only
//! views serve the routes. Orbyn never writes the file.

mod api;
mod classify;
mod file;
mod glob;
mod pin;
mod record;

pub(in crate::crew) use api::engine_words;
pub(in crate::crew) use record::Routed;

pub use classify::{PathClasses, classify, classify_paths};
pub use file::{BUILT_IN, ClassFile, FILE_MAX, base_commit, read_class_file};
pub use glob::{Glob, path_problem};

#[cfg(test)]
pub(super) use file::{
    ENGINES_MAX, GLOBS_MAX, NAME_MAX, REASON_MAX, RULES_MAX, parse as parse_file, pinned_ref,
};

use orbyn_contracts::{Brief, Thread};

use crate::app::Core;

impl Core {
    /// The commit a task starts from and the classes of its brief's files at that commit. The commit is read once, so the
    /// classes and the worker's base are the same one. `record` moves the project's remembered default-branch pin when the ref
    /// is the remembered commit or a descendant of it; a dry run passes `false` and changes nothing.
    pub(super) async fn crew_path_classes(
        &self,
        lead: &Thread,
        brief: &Brief,
        record: bool,
    ) -> (Option<String>, PathClasses) {
        let root = std::path::Path::new(&lead.cwd);
        let base = base_commit(root).await;
        let file = read_class_file(root, base.as_deref()).await;
        let patterns: Vec<String> = brief
            .files
            .iter()
            .map(|grant| grant.path.as_str().to_owned())
            .collect();
        let mut classes = classify(root, base.as_deref(), file, &patterns).await;
        let stored = match pin::stored(&self.store, &lead.project_id) {
            Ok(pin) => pin,
            Err(error) => {
                return (
                    base,
                    classes.with_pinned_blocked(
                        "default branch".to_owned(),
                        "none".to_owned(),
                        format!(
                            "the default branch Orbyn remembered cannot be read ({}), so this task cannot tell whether a ref was weakened",
                            error.public_message()
                        ),
                    ),
                );
            }
        };
        match pin::decide(root, stored.as_ref()).await {
            pin::Decision::None => {}
            pin::Decision::Read { pin, file, write } => {
                if record
                    && let Some(next) = write
                    && let Err(error) = self.store.set_crew_pinned_ref(&lead.project_id, &next)
                {
                    return (
                        base,
                        classes.with_pinned_blocked(
                            pin.name,
                            pin.commit,
                            format!(
                                "the default branch's commit cannot be remembered ({}), so this task cannot tell whether the next one would read a looser ref",
                                error.public_message()
                            ),
                        ),
                    );
                }
                // The default branch's rules are matched against the task's own tree, so leases and symlinks list the same files.
                let side = classify(root, base.as_deref(), file, &patterns).await;
                classes = classes.with_pinned(pin.name, pin.commit, side);
            }
            pin::Decision::Unreadable { pin, file } => {
                let side = classify(root, base.as_deref(), file, &patterns).await;
                classes = classes.with_pinned(pin.name, pin.commit, side);
            }
            pin::Decision::Blocked {
                name,
                commit,
                reason,
                ..
            } => {
                classes = classes.with_pinned_blocked(name, commit, reason);
            }
        }
        (base, classes)
    }
}
