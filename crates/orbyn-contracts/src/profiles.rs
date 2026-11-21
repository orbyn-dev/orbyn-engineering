//! Execution profiles (docs/execution-profiles-and-library.md, Part 1): a named preset of engine, model tier, permission
//! mode and Autopilot mode, copied onto a thread or a crew when the person applies it. There is no read-only value: no
//! read-only permission mode exists.

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use crate::{AutopilotMode, PermissionMode};

wire_enum! {
/// The tier words of docs/crew.md: the concrete model is resolved from the tier when a profile is applied.
pub enum ProfileTier {
    Lead,
    Builder,
    Scout,
}
}

wire_enum! {
/// A grant a profile apply or a library workflow run raises. The confirm of "Applying a grant-raising profile" lists these and nothing else.
pub enum ProfileGrant {
    /// `permission_mode: "full_access"`: runs any command without asking.
    FullAccess,
    /// `autopilot_mode: "full"`: merges when checks pass.
    AutopilotFull,
    /// A library workflow run that has environment sets attached: their saved values go into the command's environment.
    EnvValues,
    /// A library workflow run whose entry came from a file, a repository or a shared link, and whose command the person has not yet acknowledged.
    UntrustedSource,
}
}

wire_enum! {
/// Whether a profile's engine can be used on this machine now. A client treats a word it does not know as unknown.
pub enum ProfileEngineState {
    #[serde(rename = "ok")]
    #[ts(rename = "ok")]
    Ready,
    NotFound,
    CannotStart,
    Disabled,
    Unmanaged,
    /// The profile names no engine: the thread's own engine is used.
    #[serde(rename = "none")]
    #[ts(rename = "none")]
    NoEngine,
}
}

wire! {
pub struct ProfileAvailability {
    pub engine: ProfileEngineState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}
}

wire! {
/// A save-time note that never blocks: the engine catalog can change, so a profile made on one machine state stays valid on another.
pub struct ProfileWarning {
    pub field: String,
    pub code: String,
    pub message: String,
}
}

wire! {
pub struct Profile {
    /// A lowercase ULID, or `builtin:<slug>` for a built-in.
    pub id: String,
    pub name: String,
    pub description: String,
    /// An engine id, or `null`: keep the thread's engine.
    pub engine: Option<String>,
    pub model_tier: ProfileTier,
    pub permission_mode: PermissionMode,
    pub autopilot_mode: AutopilotMode,
    /// Read-only, listed first, cannot be edited or deleted.
    pub builtin: bool,
    /// Starts at 1 and grows by one on every change; sent back as `seen_revision`.
    pub revision: u32,
    pub created_at: String,
    pub updated_at: String,
    pub availability: ProfileAvailability,
    /// Only `GET /api/profiles/{id}` and the answers of a create or a change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub warnings: Option<Vec<ProfileWarning>>,
}
}

wire! {
/// `POST /api/profiles` and `POST /api/profiles/check`: the shape minus `id`, `builtin`, `revision` and the timestamps.
pub struct ProfileDraft {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub engine: Option<String>,
    pub model_tier: ProfileTier,
    pub permission_mode: PermissionMode,
    pub autopilot_mode: AutopilotMode,
}
}

/// A key that is present, even as `null`, is `Some`: `null` clears the engine, an absent key leaves it.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

wire! {
/// `PATCH /api/profiles/{id}`. Absent fields are left alone; a stale `seen_revision` is `409 revision_conflict`.
pub struct UpdateProfile {
    pub seen_revision: u32,
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "present")]
    #[ts(optional, type = "string | null")]
    pub engine: Option<Option<String>>,
    #[serde(default)]
    #[ts(optional)]
    pub model_tier: Option<ProfileTier>,
    #[serde(default)]
    #[ts(optional)]
    pub permission_mode: Option<PermissionMode>,
    #[serde(default)]
    #[ts(optional)]
    pub autopilot_mode: Option<AutopilotMode>,
}
}

wire! {
pub struct ProfileCheck {
    pub warnings: Vec<ProfileWarning>,
    pub availability: ProfileAvailability,
}
}

wire! {
/// `PUT /api/threads/{id}/profile`. `profile_id: null` only forgets the chip and keeps the values.
pub struct ApplyProfile {
    pub profile_id: Option<String>,
    /// When present and stale, `409 revision_conflict` and nothing is applied.
    #[serde(default)]
    #[ts(optional)]
    pub seen_revision: Option<u32>,
    /// The grant codes the person saw. Only a grant-raising profile needs them.
    #[serde(default)]
    pub ack_grants: Vec<ProfileGrant>,
}
}

wire! {
/// `POST /api/crew/profile/apply`.
pub struct ApplyCrewProfile {
    pub crew_id: String,
    pub profile_id: String,
    #[serde(default)]
    #[ts(optional)]
    pub seen_revision: Option<u32>,
    #[serde(default)]
    pub ack_grants: Vec<ProfileGrant>,
}
}

wire! {
/// What an apply copied. A field that does not apply to the target is absent and named in `ignored`.
pub struct AppliedProfileValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub engine: Option<String>,
    /// The concrete model the tier resolved to. For a crew it is the builder default; the router still picks per brief.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub permission_mode: Option<PermissionMode>,
    /// A crew only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub autopilot_mode: Option<AutopilotMode>,
}
}

wire! {
/// What a thread or crew last got. The name and revision are those at apply time.
pub struct ProfileApplication {
    pub profile_id: String,
    pub name: String,
    pub revision: u32,
    /// The person changed the model or permission mode afterwards.
    pub customized: bool,
    pub applied_at: String,
}
}

wire! {
pub struct ApplyProfileAnswer {
    pub applied: AppliedProfileValues,
    /// Fields that do not apply to this target: `autopilot_mode` on every plain thread.
    pub ignored: Vec<String>,
    /// `null` when the chip was cleared.
    pub application: Option<ProfileApplication>,
}
}
