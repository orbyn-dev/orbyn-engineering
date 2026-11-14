//! The screen for a command the core itself runs, such as a check of an answer key a person asks to run again. It is the same
//! reading a worker's command gets at the gate (the always-denied list, N1 to N10, then the blast-radius guard), applied before
//! the command starts. It runs nothing and changes nothing, and fails closed: a command it cannot read, or a list it cannot load,
//! is refused.
//!
//! A refusal names the rule and never the command. The core's own check commands were not screened before this: the verifier
//! (`verify`) and the merge queue run a key's or a repository's command as written.

use std::path::Path;

use orbyn_contracts::{ActionKind, CrewPathPattern};

use super::hard::{context, refuse_in};
use super::review::Ground;
use crate::app::Core;

/// What the screen found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screened {
    /// Nothing refused it.
    Clear,
    /// It must not run: the rule that refused it (`N1` to `N10`, `blast` or `unreadable`).
    Refused(&'static str),
}

/// Whether a quote is still open at the end of `text`, the way a shell reads one.
fn quote_open(text: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, '\\') | (Some('"'), '\\') => {
                chars.next();
            }
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), c) if c == open => quote = None,
            _ => {}
        }
    }
    quote.is_some()
}

impl Core {
    /// Screens one command that would run in `worktree`. `base` is the commit the task started from and `protected` the key's
    /// protected paths, as the gate reads them for a worker's command.
    pub(crate) async fn crew_screen_command(
        &self,
        worktree: &Path,
        base: Option<&str>,
        protected: &[CrewPathPattern],
        command: &str,
    ) -> Screened {
        if command.trim().is_empty() || quote_open(command) {
            return Screened::Refused("unreadable");
        }
        let ground = Ground::read(self, worktree, base).await;
        let scene = ground.scene(protected);
        let ctx = context(ActionKind::Command, command, &[], &scene);
        if ctx.commands.is_empty() {
            return Screened::Refused("unreadable");
        }
        if let Some(refusal) = refuse_in(&ctx) {
            return Screened::Refused(refusal.id);
        }
        match self.guard_of(&ctx, worktree).await {
            Ok(None) => Screened::Clear,
            Ok(Some(_)) | Err(_) => Screened::Refused("blast"),
        }
    }
}
