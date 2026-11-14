//! Data classes (docs/crew.md, "As built (slice 4i, routing)"): `.orbyn/crew.json` says which engines may take which paths.
//! The file is read at two places through hardened git, never from the checkout's files, and the stricter of the two applies: the
//! checkout's HEAD when a task starts (the task keeps that commit: an edit made after the start has no effect on it), and the
//! project's default branch as of the last fetch (`refs/remotes/origin/HEAD`, else `origin/main`, `origin/master` or `origin/trunk`,
//! else a local `init.defaultBranch`, `main`, `master` or `trunk`; with none of them HEAD alone is read, as before). An engine must
//! be allowed by both, a file either side cannot read blocks every engine, and the route card says when the two files differ.
//! An agent that moves HEAD to a commit without the file, or commits a looser one, therefore does not loosen the next task. The
//! default branch is not out of an agent's reach: a remote-tracking ref moves only by fetch or pull, but a local branch moves by a
//! commit on it, and `git update-ref` or an edit of `.git` moves either (Autopilot's N7 holds `update-ref`); the macOS confinement of
//! #441 does not close the repository's `.git`, and Windows and Linux workers are refused. See the known limits in docs/crew.md.
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
    BUILT_IN, ClassFile, FILE_MAX, INVALID_RULE, PinnedRef, base_commit, pinned_ref,
    read_class_file,
};
pub use glob::{Glob, path_problem};

#[cfg(test)]
pub(super) use file::{
    ENGINES_MAX, GLOBS_MAX, NAME_MAX, REASON_MAX, RULES_MAX, parse as parse_file,
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
        if let Some(pin) = pinned_ref(root).await {
            let there = read_class_file(root, Some(&pin.commit)).await;
            let side = classify(root, Some(&pin.commit), there, &patterns).await;
            classes = classes.with_pinned(pin.name, pin.commit, side);
        }
        (base, classes)
    }
}
