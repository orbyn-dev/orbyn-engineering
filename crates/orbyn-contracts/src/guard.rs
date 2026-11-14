//! The blast-radius guard over HTTP (Crew Preview, slice 4i): the two lists of targets that live on this device (`prod` and `safe`),
//! and the read-only test of what the guard says about a command. A destructive command against a production-linked target always
//! needs a second key, even in Full; the person can always approve it alone, once. The lists are kept in the service's data folder,
//! never in the repository, so a repository file cannot loosen the guard.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

wire_enum! {
/// Which list a target is on. `prod` forces a target to count as production-linked; `safe` exempts it. A target on both lists is
/// production-linked: `prod` wins.
pub enum GuardList {
    Prod,
    Safe,
}
}

wire_enum! {
/// What a listed name names.
pub enum GuardTargetKind {
    Host,
    Profile,
    Project,
    Context,
    Workspace,
    Bucket,
}
}

wire_enum! {
/// The kind of destructive command the guard knows. The patterns are built in: no repository file can add or remove one.
pub enum GuardCategory {
    /// `rm -r`, `find -delete`, `rsync --delete`, `shred`, `truncate`, also inside `ssh host '...'`.
    FileDeletion,
    /// `psql`, `mysql`, `mongosh`, `redis-cli` or `cqlsh` running DROP, TRUNCATE, DELETE, an UPDATE with no WHERE, FLUSHALL.
    Database,
    /// `aws s3 rm|rb`, `aws ... delete-*|terminate-*`, `gcloud` or `az` `delete`, `gsutil rm`.
    Cloud,
    /// `docker rm` or `prune` against a remote host or context.
    Containers,
    /// `git push` that forces, deletes or uses a `+refspec`, to a remote that is not local.
    Git,
    /// A command the guard could not read that holds a destructive word and a non-local word. Failing closed.
    Unparsed,
}
}

wire_enum! {
/// What the guard says about a command.
pub enum GuardVerdict {
    /// Not a destructive command the guard knows.
    NotDestructive,
    /// Destructive, but every target is local.
    Local,
    /// Destructive against a remote target the person listed as safe.
    Exempt,
    /// Destructive against a production-linked target: it needs a second key, or the person.
    Guarded,
}
}

wire! {
/// One entry of a list on this device.
pub struct GuardTarget {
    pub id: String,
    pub list: GuardList,
    /// An exact name, or `*.suffix` for the suffix and everything under it. Lowercase, letters, digits, `.`, `_` and `-` only.
    pub pattern: String,
    pub kind: GuardTargetKind,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub note: Option<String>,
    pub added_at: String,
}
}

wire! {
/// Both lists, and the most entries they may hold together.
pub struct GuardTargets {
    pub items: Vec<GuardTarget>,
    pub max_entries: u32,
}
}

wire! {
/// The body that adds an entry. Adding a `safe` entry loosens the guard and needs the person's presence proof, as approving once does.
#[serde(deny_unknown_fields)]
pub struct GuardTargetAdd {
    pub list: GuardList,
    pub pattern: String,
    pub kind: GuardTargetKind,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub note: Option<String>,
}
}

wire! {
/// What the guard says about a command, for the Settings test box. It runs nothing and changes nothing.
pub struct GuardTest {
    /// The command as it is shown: redacted and cut to 400 characters.
    pub command: String,
    pub verdict: GuardVerdict,
    #[serde(default)]
    #[ts(optional = nullable)]
    pub category: Option<GuardCategory>,
    /// What matched, such as `rm -r` or `psql: DROP`.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub matched_by: Option<String>,
    /// The target the guard read, redacted, or `unresolved` / `not named` where it could not.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub target: Option<String>,
    pub local: bool,
    pub production_linked: bool,
    /// The list entry that decided it, when one did.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub matched_entry: Option<GuardTarget>,
}
}
