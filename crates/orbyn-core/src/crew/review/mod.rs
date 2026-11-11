//! Review stages (docs/crew.md, "Review stages"). A review lane is a run of its own: a reviewer on a
//! fresh thread, routed by role (`review`, with its lane) to another vendor than the builder where one is
//! configured, at the lead tier, reading the work and reporting typed findings. It never edits, and it never
//! sees the answer key or a check command. The group does not decide whether a finding is true: code does,
//! through a check that fails or a second independent run that agrees ([`decide`]). A confirmed finding sends
//! a fix brief to a builder, and the lanes run again ([`fix`], [`ended`]).

mod decide;
mod ended;
mod findings;
mod fix;
mod lanes;
mod rules;
mod status;

use orbyn_contracts::{CrewError, CrewId, CrewMember, CrewToken, ErrorCode, Thread};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub use decide::agrees;
pub use rules::{RULES_FILE, RuleLane, read_rules};
pub use status::{FindingStatus, status_of};

use super::stamp::Stamper;
use super::start::Caller;
use super::tool::{LEAD_MEMBER, invalid, stamp_now};
use crate::app::Core;
use crate::store::{CrewRun, CrewTask};
use crate::tools::mcp::McpBinding;

pub const REVIEW_TOOL: &str = "crew_review";
pub const FINDING_TOOL: &str = "crew_finding";
pub const CHECK_FINDING_TOOL: &str = "crew_check_finding";
pub const STATUS_TOOL: &str = "crew_review_status";
/// The review tools. The lead calls all but `crew_finding`, which only a reviewer calls.
pub const REVIEW_TOOLS: [&str; 4] = [REVIEW_TOOL, FINDING_TOOL, CHECK_FINDING_TOOL, STATUS_TOOL];

/// The most findings one review run files, and one independent second run (which only has to find one place).
pub(super) const MOST_FINDINGS: usize = 100;
pub(super) const MOST_SECOND_FINDINGS: usize = 10;
/// The lanes the merge queue is owed by default (`merge::owed_lanes`): the general code review, which the review tool calls "correctness", for
/// medium and high diffs, and a security review for high ones. They are lane tokens like any other, written by slice 4b when the lead names them.
pub const CODE_LANE: &str = "correctness";
pub const SECURITY_LANE: &str = "security";

/// The most fix briefs one reviewed task is sent, and the most rounds one lane runs on it. Past either a person decides.
pub(super) const MOST_FIXES: usize = 3;
pub(super) const MOST_ROUNDS: u32 = 5;

/// The tools as `tools/list` shows them.
pub fn specs() -> Vec<Value> {
    vec![
        json!({
            "name": REVIEW_TOOL,
            "description": "Preview: review finished work. `task` is a task that has passed; `lanes` names the \
                reviewer lanes to run (lowercase words such as correctness, security, performance, accessibility, design, \
                licence, docs, or one you name). There is no limit on lanes, and a lane can be added after a pass. Lanes \
                the repo's rules (`.orbyn/crew.json`) ask for run as well. Every lane is a separate read-only run, \
                routed to another vendor than the builder where one is configured. A lane named again runs again, as the \
                next round.",
            "inputSchema": {
                "type": "object",
                "properties": { "task": { "type": "string" }, "lanes": { "type": "array" } },
                "required": ["task"],
            },
        }),
        json!({
            "name": FINDING_TOOL,
            "description": "Preview, for a reviewer: file one finding. `severity` (critical, high, medium, low or info), \
                `path` and `line` (1-based), `claim`, `evidence` and optionally `repro`. The core adds the lane, the reviewed \
                commit, the author and the id. A finding blocks nothing until the core confirms it by a check that fails or \
                a second independent review. Refused once your run has ended.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "severity": { "type": "string" },
                    "path": { "type": "string" },
                    "line": { "type": "integer" },
                    "claim": { "type": "string" },
                    "evidence": { "type": "string" },
                    "repro": { "type": "string" },
                },
                "required": ["severity", "path", "line", "claim", "evidence"],
            },
        }),
        json!({
            "name": CHECK_FINDING_TOOL,
            "description": "Preview: have the core decide whether a finding is true. With `repro`, a check that states the \
                correct behaviour (a command, test or file_scope check): if it fails the finding is confirmed, and if it passes \
                it is not. Without `repro`, an independent second review of the same lane runs, and the finding is confirmed \
                only if that run reports the same place. A confirmed finding sends a fix brief to a builder and the lanes \
                run again. The same finding after a fix stops the loop.",
            "inputSchema": {
                "type": "object",
                "properties": { "finding": { "type": "string" }, "repro": { "type": "object" } },
                "required": ["finding"],
            },
        }),
        json!({
            "name": STATUS_TOOL,
            "description": "Preview: where a task's review stands: each lane and round, each finding with whether it is \
                unchecked, pending, confirmed or unconfirmed and whether it is open, fixed or regressed, and whether any \
                confirmed finding still blocks.",
            "inputSchema": {
                "type": "object",
                "properties": { "task": { "type": "string" } },
                "required": ["task"],
            },
        }),
    ]
}

/// A finding's identity across rounds: its lane, its file and its claim with case and spacing ignored. The line
/// is left out because a fix moves it. A reworded claim is a new finding (a limit the README names).
pub fn fingerprint(lane: &str, path: &str, claim: &str) -> String {
    let claim: Vec<String> = claim.split_whitespace().map(str::to_lowercase).collect();
    crate::util::hex(&Sha256::digest(
        format!("{lane}\0{path}\0{}", claim.join(" ")).as_bytes(),
    ))
}

impl Core {
    /// Runs a review tool for the MCP service.
    pub(super) async fn crew_review_tool(
        self: &std::sync::Arc<Self>,
        binding: &McpBinding,
        tool: &str,
        args: &Value,
    ) -> Result<Value, CrewError> {
        if !self.crew_enabled() {
            return Err(CrewError::new(ErrorCode::UnknownTool, tool));
        }
        match tool {
            FINDING_TOOL => self.crew_finding_tool(binding, args),
            REVIEW_TOOL => self.crew_lanes_tool(binding, args).await,
            CHECK_FINDING_TOOL => self.crew_check_finding_tool(binding, args).await,
            _ => self.crew_status_tool(binding, args),
        }
    }

    /// The lead's thread and a stamper for what it asks the core to write. Only the lead calls these tools.
    pub(super) fn review_lead(
        &self,
        binding: &McpBinding,
        tool: &str,
    ) -> Result<(Thread, Stamper), CrewError> {
        if !matches!(self.crew_caller(binding)?, Caller::Lead) {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                format!("only the lead calls {tool}"),
            ));
        }
        let thread = self
            .store
            .thread(&binding.scope.thread_id)
            .map_err(|e| invalid(e.public_message()))?;
        let stamper = self.review_stamper(&thread, binding.scope.family.as_str())?;
        Ok((thread, stamper))
    }

    /// A stamper that writes as the lead of `thread`, for a brief the core sends on the lead's behalf.
    pub(super) fn review_stamper(
        &self,
        thread: &Thread,
        engine: &str,
    ) -> Result<Stamper, CrewError> {
        Ok(Stamper {
            crew_id: CrewId::new(thread.id.clone()).map_err(invalid)?,
            engine: CrewToken::new(engine.to_owned())
                .map_err(|why| invalid(format!("engine: {why}")))?,
            member: CrewMember::new(LEAD_MEMBER).map_err(invalid)?,
            now: stamp_now()?,
        })
    }

    /// The finished work a lane or a finding belongs to: in the lead's crew, not itself a review run.
    pub(super) fn review_subject(
        &self,
        thread: &Thread,
        id: &str,
    ) -> Result<(CrewTask, CrewRun), CrewError> {
        let unknown = || {
            CrewError::new(
                ErrorCode::InvalidObject,
                format!("{id}: no such task in this crew"),
            )
        };
        let task = self
            .store
            .crew_task(id)
            .map_err(super::leases::internal)?
            .filter(|task| task.crew_id == thread.id)
            .ok_or_else(unknown)?;
        let run = self
            .store
            .crew_run(&task.id)
            .map_err(super::leases::internal)?
            .ok_or_else(unknown)?;
        if self
            .store
            .crew_review(&task.id)
            .map_err(super::leases::internal)?
            .is_some()
        {
            return Err(invalid(
                "task: a review run is not itself reviewed; name the work it read",
            ));
        }
        Ok((task, run))
    }
}
