//! Engines: models, capabilities, health, compatibility, updates and provider instances.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

use crate::PermissionMode;

fn every_permission_mode() -> Vec<PermissionMode> {
    PermissionMode::ALL.to_vec()
}

wire! {
/// One value of a model option, such as `high` for effort.
pub struct ModelOptionValue {
    pub id: String,
    pub label: String,
    pub default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
}
}

wire! {
/// A choice a model offers per turn.
pub struct ModelOption {
    /// `effort`, `service_tier`, `fast_mode` (`on` or `off`), `mode` for an ACP
    /// agent's session mode, or `config:<id>` for one of its config options.
    pub id: String,
    pub label: String,
    pub values: Vec<ModelOptionValue>,
}
}

wire! {
pub struct ModelInfo {
    /// What `--model`, `model` and `session/set_model` take.
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub is_default: bool,
    /// Added in settings rather than reported by the engine.
    pub custom: bool,
    pub options: Vec<ModelOption>,
    /// What a turn can carry, such as `text` and `image`. Empty is unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub input_modalities: Vec<String>,
    /// The model the engine suggests moving to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub upgrade: Option<String>,
}
}

wire! {
/// A subagent the engine offers, reached by mentioning it.
pub struct EngineAgent {
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}
}

wire! {
/// What was last chosen on an engine, carried to its new threads.
#[derive(Default)]
pub struct ModelSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub options: BTreeMap<String, String>,
}
}

wire! {
pub struct EngineModels {
    pub engine: String,
    /// `engine`, `documented` (a fixed list from the vendor's docs) or `none`.
    pub source: String,
    pub models: Vec<ModelInfo>,
    /// Why the engine could not be asked. Custom models still appear.
    pub error: Option<String>,
    pub checked_at: String,
    /// The list is a stand-in for an answer the engine did not give, so choices
    /// are not checked against it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub fallback: bool,
    /// Choices that apply to every model, such as an ACP agent's session modes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<ModelOption>>")]
    pub options: Vec<ModelOption>,
    /// Subagents the engine offers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<EngineAgent>>")]
    pub agents: Vec<EngineAgent>,
    /// Output styles the engine offers. Listed only; Orbyn does not set one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub output_styles: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub output_style: Option<String>,
    /// What was last chosen on this engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub remembered: Option<ModelSelection>,
}
}

wire_enum! {
pub enum EngineTier {
    Native,
    Acp,
    Agui,
    Terminal,
}
}

wire_enum! {
pub enum SkillDelivery {
    NativePlugin,
    NativeSkillRoot,
    McpSkillServer,
    PromptBootstrap,
}
}

wire! {
pub struct EngineCapabilities {
    pub streaming: bool,
    pub approvals: bool,
    pub resume: bool,
    pub mcp: bool,
    pub sandbox: bool,
    /// False means Orbyn cannot gate this engine's actions.
    pub managed: bool,
    pub skill_delivery: Vec<SkillDelivery>,
    /// The engine can pause and resume a goal, not only set and clear it.
    #[serde(default)]
    pub goal_control: bool,
    /// A denied action can carry your words back to the engine.
    #[serde(default)]
    pub decline_feedback: bool,
    /// The modes this engine offers. A thread's saved mode outside them runs
    /// Supervised and is shown that way.
    #[serde(default = "every_permission_mode")]
    pub permission_modes: Vec<PermissionMode>,
}
}

wire_enum! {
pub enum AuthState {
    LoggedIn,
    LoggedOut,
    Unknown,
    NotApplicable,
}
}

wire_enum! {
pub enum InstallerKind {
    Npm,
    Pnpm,
    Bun,
    Yarn,
    Volta,
    VitePlus,
    Homebrew,
    Winget,
    SelfManaged,
    Unknown,
}
}

wire_enum! {
pub enum VersionSupport {
    Tested,
    Untested,
    BelowMinimum,
    Unknown,
}
}

wire_enum! {
/// How badly the installed version is outside what the signed manifest supports.
pub enum CompatibilityStatus {
    /// A version the manifest names as known-incompatible.
    Broken,
    /// Below the manifest's minimum version.
    Unsupported,
    /// A version the manifest names as partly compatible.
    Graceful,
}
}

wire! {
/// A report sent through the engine's own feedback route. Orbyn collects no
/// credential and adds no conversation of its own.
pub struct EngineFeedbackRequest {
    /// What the person wrote, when they wrote something.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub reason: Option<String>,
    /// Attach the engine's logs, which can contain prompts and file paths.
    #[serde(default)]
    pub include_logs: bool,
    /// The engine's own thread, when the report is about one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub thread_id: Option<String>,
}
}

wire! {
/// What the engine answered once it accepted a report.
pub struct EngineFeedbackReceipt {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub prompt_hash: Option<String>,
}
}

wire! {
/// The signed manifest's warning about the installed version, and the version it
/// recommends instead. Only a signed manifest can produce one.
pub struct EngineCompatibility {
    pub status: CompatibilityStatus,
    pub message: String,
    /// The recommended version, when the manifest names one and it is not installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recommended_version: Option<String>,
    /// The installer command that installs that exact version, present only when
    /// the installer that owns this binary can install one version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recommended_command: Option<String>,
}
}

wire! {
pub struct EngineStatus {
    pub id: String,
    pub name: String,
    pub tier: EngineTier,
    pub capabilities: EngineCapabilities,
    pub installed: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub auth: AuthState,
    pub installer: InstallerKind,
    pub latest: Option<String>,
    pub support: VersionSupport,
    /// The signed manifest's warning about this version, when it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub compatibility: Option<EngineCompatibility>,
    /// Present only when the owning installer can update this binary.
    pub update_command: Option<String>,
    pub manual_hint: Option<String>,
    pub sign_in_command: Option<String>,
    pub updating: bool,
    pub problem: Option<String>,
    /// Credential variables this engine would inherit, by name only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub inherited_credentials: Vec<String>,
    /// The origin of an inherited endpoint override, with no path or user info.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub endpoint_origin: Option<String>,
    /// The engine this one is a named setup of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance_of: Option<String>,
    /// A color for telling instances apart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accent: Option<String>,
    /// The signed-in account, such as an email, when the CLI's own status reports one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account: Option<String>,
}
}

wire_enum! {
/// What a provider row says beside its ready, update or sign-in state: what Orbyn has
/// not seen or cannot control. Each is a fact about the provider, never a promise.
pub enum ProviderLabel {
    /// Orbyn can start it, but launch, models or approvals were never seen on a real install.
    NotVerified,
    /// It can act without asking, so Orbyn cannot ask before it acts.
    Unmanaged,
    /// Runs, but the installed version leaves out some of what Orbyn offers.
    LimitedSupport,
    /// The program or app exists but Orbyn cannot start it as an agent.
    CantStart,
    /// Not built. Shown so the gap is visible.
    Planned,
    /// No program was found, or the program found is a different one with the same name.
    NotFound,
}
}

wire_enum! {
/// The list's groups, in the order the Providers page shows them.
pub enum ProviderGroup {
    /// Seen working, or at least signed in or out: what threads can use.
    InUse,
    /// Found on this computer and startable, but not seen working on a real install.
    Found,
    /// The other engines that start over ACP and were only tried with a test agent.
    More,
    /// Found, and not startable as an agent.
    CantStart,
    /// Registered, but its program was not found. It stays so it can be edited or removed.
    NotFound,
    /// Not built.
    Planned,
}
}

wire! {
/// One row of the grouped provider list.
pub struct ProviderEntry {
    /// The engine's id. For a planned or app-only row, the id it will have.
    pub id: String,
    /// The registry's display name.
    pub name: String,
    pub group: ProviderGroup,
    /// The labels that apply, in the order they are drawn.
    pub labels: Vec<ProviderLabel>,
    /// The line under the name, when the labels need words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// True when an engine of this id is registered: it has a switch and, for an instance, Edit.
    pub registered: bool,
    /// True when a detail page exists for it.
    pub openable: bool,
}
}

wire! {
/// The provider list as the router reads it, grouped for the Providers page.
pub struct ProviderList {
    pub providers: Vec<ProviderEntry>,
}
}

wire! {
/// One reachability check: whether anything answered at a host, not whether it is signed in.
pub struct DependencyCheck {
    pub host: String,
    /// False when the check was not made, for example in offline mode.
    pub checked: bool,
    pub reachable: bool,
    /// The HTTP status of the answer; any answer shows the host is reachable.
    pub status: Option<u16>,
    pub latency_ms: Option<u64>,
    pub message: Option<String>,
}
}

wire! {
pub struct ProviderHealth {
    pub engine: String,
    pub name: String,
    /// Its program was found and answered `--version`.
    pub installed: bool,
    pub auth: AuthState,
    /// The program's own problem, from the probe.
    pub problem: Option<String>,
    /// The provider's service, for Claude and Codex; absent for engines with no fixed host.
    pub service: Option<DependencyCheck>,
}
}

wire! {
/// What the app depends on, as reachable from this machine right now.
pub struct DependencyHealth {
    pub checked_at: String,
    pub github: DependencyCheck,
    pub providers: Vec<ProviderHealth>,
}
}

wire! {
pub struct ProbeBinary {
    /// An absolute path to a program.
    pub path: String,
}
}

wire! {
/// What running `--version` on a path showed.
pub struct BinaryProbe {
    /// A file is there and could be started.
    pub exists: bool,
    pub path: Option<String>,
    pub version: Option<String>,
    pub problem: Option<String>,
}
}

wire! {
/// How many conversations an engine keeps on this machine, from its own history folder.
pub struct NativeSessionCount {
    pub engine: String,
    pub count: u64,
    /// The history folder was too large to count to the end, so `count` is a floor.
    pub truncated: bool,
}
}

wire! {
/// One engine's outcome in an update of every engine with a newer version.
pub struct EngineUpdateOutcome {
    pub id: String,
    pub name: String,
    /// The version it reports afterwards, when the update worked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}
}

wire! {
/// One step of an engine update while it runs, sent on
/// `GET /api/engines/update-events`. Every `started` is followed by one `finished`.
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineUpdateProgress {
    /// The owning installer started this command.
    Started { engine: String, command: String },
    /// A line the installer printed, as it printed it.
    Output { engine: String, line: String },
    /// The update ended: `version` when it worked, `error` when it did not.
    Finished {
        engine: String,
        version: Option<String>,
        error: Option<String>,
    },
}
}

wire! {
/// A named setup of an engine, which threads pick like any engine. It holds what
/// launches and nothing about an account: no environment, no credential, no config
/// folder.
pub struct ProviderInstance {
    pub id: String,
    /// The engine this runs: a built-in, custom or detected one, not an instance.
    pub driver: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accent: Option<String>,
    /// A turned-off instance is not listed and refuses new runs.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    /// Runs this program instead of the driver's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub program: Option<String>,
    /// Added after the driver's own arguments. ACP, terminal and AG-UI engines only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub args: Vec<String>,
    /// Claude only: compact after this many tokens, 100000 to 1000000. Empty means
    /// Claude's own default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub auto_compact_window: Option<String>,
}
}

fn enabled_by_default() -> bool {
    true
}

wire! {
pub struct CustomEngine {
    pub id: String,
    pub name: String,
    pub tier: EngineTier,
    /// ACP or terminal launch command; AG-UI endpoint URL for `agui`.
    pub target: String,
    pub args: Vec<String>,
}
}
