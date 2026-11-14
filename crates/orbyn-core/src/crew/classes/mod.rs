//! Data classes (docs/crew.md, "As built (slice 4i, routing)"): `.orbyn/crew.json` says which engines may take which paths.
//! The file is read at the checkout's HEAD when a task starts, through hardened git and never from the checkout's files, and
//! the task keeps that commit: an edit made after the start has no effect on it. An agent that can run git can still move HEAD
//! to a commit without the file, so this is not a guarantee against an agent (see the known limits in docs/crew.md, issue #535). The router uses the result as a hard filter before scoring
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
pub use file::{BUILT_IN, ClassFile, FILE_MAX, INVALID_RULE, base_commit, read_class_file};
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
        let classes = classify(root, base.as_deref(), file, &patterns).await;
        (base, classes)
    }
}
