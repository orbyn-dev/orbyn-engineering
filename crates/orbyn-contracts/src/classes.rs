//! Data classes over HTTP (Crew Preview, slice 4i, routing): what a repository's `.orbyn/crew.json` says about which engines may see
//! which paths, read at the commit a task starts from, and how each routing used it. These are read-only views. Nothing here changes
//! the repository, and the file is never written by Orbyn.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::crew::{RouteDecision, RouteRejection, RouteRequested};

wire! {
/// One engine a rule names, and whether this machine has it.
pub struct CrewClassEngine {
    pub id: String,
    /// The engine's name when this machine has it, otherwise the id.
    pub name: String,
    pub on_this_machine: bool,
}
}

wire! {
/// One rule of the file: the paths it covers and the engines it allows there.
pub struct CrewClassRule {
    pub name: String,
    pub paths: Vec<String>,
    /// Empty means no engine may take these paths.
    pub engines: Vec<CrewClassEngine>,
    pub reason: String,
}
}

wire! {
/// What the repository's rules are at the commit a task would start from (`GET /api/crew/classes`).
pub struct CrewClassesView {
    pub thread: String,
    /// The commit the rules are read at. Absent when it could not be resolved.
    #[ts(optional = nullable)]
    pub commit: Option<String>,
    /// The file's blob id at that commit. Absent when the file is missing or unreadable.
    #[ts(optional = nullable)]
    pub blob: Option<String>,
    /// A label for the checkout the rules are read for: its branch, or the folder's name.
    pub worktree: String,
    /// The class every path outside the rules belongs to.
    pub built_in: String,
    pub rules: Vec<CrewClassRule>,
    /// Why the file is invalid. Any problem blocks every engine, so a path is never allowed by a file nobody could read.
    pub problems: Vec<String>,
    /// Keys of the file this version does not read, and other things worth knowing that do not block.
    pub notes: Vec<String>,
    /// True when the checkout's copy of the file is not the committed one.
    pub working_copy_differs: bool,
    /// Said exactly when `working_copy_differs`: an edit is not in force until it is committed.
    #[ts(optional = nullable)]
    pub working_copy_note: Option<String>,
}
}

wire! {
/// One class a path belongs to, and the pattern that put it there.
pub struct CrewClassMatch {
    pub name: String,
    pub pattern: String,
}
}

wire! {
/// One path of `GET /api/crew/classes/test`.
pub struct CrewClassTestPath {
    pub path: String,
    /// False when the path is not a repository path; `problem` says why and nothing else is filled in.
    pub valid: bool,
    #[ts(optional = nullable)]
    pub problem: Option<String>,
    /// The classes the path is in. Empty means the built-in class.
    pub classes: Vec<CrewClassMatch>,
    /// The engines every matching class allows. Absent means any signed-in engine.
    #[ts(optional = nullable)]
    pub allowed_engines: Option<Vec<CrewClassEngine>>,
    /// The sentence a route card shows.
    pub route_card: String,
}
}

wire! {
/// `GET /api/crew/classes/test`: up to 10 paths checked against the rules at the commit.
pub struct CrewClassTest {
    pub thread: String,
    #[ts(optional = nullable)]
    pub commit: Option<String>,
    #[ts(optional = nullable)]
    pub blob: Option<String>,
    pub problems: Vec<String>,
    pub paths: Vec<CrewClassTestPath>,
}
}

wire! {
/// A class a routing found in the brief's files.
pub struct CrewRouteClassHit {
    pub name: String,
    pub pattern: String,
    /// Up to 20 paths that put the brief in the class.
    pub samples: Vec<String>,
}
}

wire! {
/// An engine a rule blocked, and the rule's words.
pub struct CrewRouteBlocked {
    pub engine: String,
    pub rule: String,
    pub reason: String,
}
}

wire! {
/// One `route_class` row of the ledger: how the rules were used for one routing.
pub struct CrewRouteClassRow {
    pub id: String,
    pub at: String,
    /// The commit and file the rules were read from, so the row can be checked later.
    #[ts(optional = nullable)]
    pub commit: Option<String>,
    #[ts(optional = nullable)]
    pub blob: Option<String>,
    /// Set when the file was invalid and so blocked every engine.
    #[ts(optional = nullable)]
    pub problem: Option<String>,
    /// Set when the brief's files could not all be told, so every class counted.
    #[ts(optional = nullable)]
    pub unknown: Option<String>,
    pub classes: Vec<CrewRouteClassHit>,
    pub blocked: Vec<CrewRouteBlocked>,
    /// The engine chosen. Absent when none was.
    #[ts(optional = nullable)]
    pub chosen: Option<String>,
    /// Every engine the router rejected, whatever the reason. Present when nothing was chosen.
    pub rejected: Vec<RouteRejection>,
    #[ts(optional = nullable)]
    pub message: Option<String>,
    #[ts(optional = nullable)]
    pub requested: Option<RouteRequested>,
    /// Set when the stored row failed the secret check and kept no detail.
    #[ts(optional = nullable)]
    pub withheld: Option<String>,
}
}

wire! {
/// Why nothing was routed, as stored with the Needs you item.
pub struct CrewRouteRefusal {
    pub message: String,
    #[ts(optional = nullable)]
    pub requested: Option<RouteRequested>,
    pub rejected: Vec<RouteRejection>,
}
}

wire! {
/// `GET /api/crew/route/{task}`: the decision or the refusal, and the ledger rows of the routing.
pub struct CrewRouteView {
    pub task: String,
    pub state: String,
    #[ts(optional = nullable)]
    pub decision: Option<RouteDecision>,
    #[ts(optional = nullable)]
    pub refusal: Option<CrewRouteRefusal>,
    pub rows: Vec<CrewRouteClassRow>,
}
}
