//! Data classes (docs/crew.md, "As built (slice 4i, routing)"): `.orbyn/crew.json` says which engines may take which paths.
//! The file is read at two places through hardened git, never from the checkout's files, and the stricter of the two applies: the
//! checkout's HEAD when a task starts (the task keeps that commit: an edit made after the start has no effect on it), and the
//! project's default branch as of the last fetch (`refs/remotes/origin/HEAD`, else `origin/main`, `origin/master` or `origin/trunk`,
//! else a local `init.defaultBranch`, `main`, `master` or `trunk`; with none of them HEAD alone is read, as before). An engine must
//! be allowed by both, a file either side cannot read blocks every engine, and the route card says when the two files differ.
//! An agent that moves HEAD to a commit without the file, or commits a looser one, therefore does not loosen the next task. The
//! default branch is not out of an agent's reach: a remote-tracking ref moves by fetch or pull, a local branch by a commit on it, and
//! either by git plumbing, `git remote set-head`, `remote rename|remove|set-url` or an edit of `.git`. Autopilot's N7 holds those
//! commands, but it reads command text only; the macOS confinement of #441 does not close the repository's `.git`, and Windows and
//! Linux workers are refused. A repository that loses its default-branch ref is read at HEAD alone again (nothing remembers the
//! earlier ref between tasks). See the known limits in docs/crew.md.
//! The router uses the result as a hard filter before scoring
//! ([`class_screen`](super::filters::class_screen)); each routing writes one `route_class` row to the approval ledger; and
//! three read-only views serve the routes. Orbyn never writes the file.

mod api;
mod classify;
mod file;
mod glob;
mod record;

pub(in crate::crew) use api::engine_words;
pub(in crate::crew) use record::Routed;

pub use classify::{PathClasses, classify, classify_paths};
pub use file::{
    BUILT_IN, ClassFile, FILE_MAX, INVALID_RULE, base_commit, read_class_file, read_pinned,
};
pub use glob::{Glob, path_problem};

#[cfg(test)]
pub(super) use file::{
    ENGINES_MAX, GLOBS_MAX, NAME_MAX, REASON_MAX, RULES_MAX, parse as parse_file, pinned_ref,
};

use orbyn_contracts::{Brief, Thread};

use crate::app::Core;

impl Core {
    /// The commit a task starts from and the classes of its brief's files at that commit. The commit is read once, so the
    /// classes and the worker's base are the same one.
    pub(super) async fn crew_path_classes(
        &self,
        lead: &Thread,
        brief: &Brief,
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
        if let Some((pin, there)) = read_pinned(root).await {
            // The default branch's rules are matched against the task's own tree, so leases and symlinks list the same files.
            let side = classify(root, base.as_deref(), there, &patterns).await;
            classes = classes.with_pinned(pin.name, pin.commit, side);
        }
        (base, classes)
    }
}
