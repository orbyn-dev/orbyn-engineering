//! The Crew tools. `crew_route` is the dry run of the router. It answers which engine a brief would go to and
//! why, and starts nothing: no thread, no run, no worktree, no engine started for limits
//! (they are read from the cache only), and nothing is stored. It is
//! behind the Crew Preview flag (`CoreConfig::crew_preview`, off by default), so with the
//! flag off the tool is neither listed nor callable.

use orbyn_contracts::{
    CrewError, CrewId, CrewMember, CrewObject, CrewTimestamp, CrewToken, ErrorCode, RouteRole,
};
use serde_json::{Value, json};

use super::guard::refuse_secret;
use super::router::{RouteFailure, RouteInput, route};
use super::score::EvalHistory;
use super::stamp::Stamper;
use super::tiers::RouterConfig;
use crate::app::Core;
use crate::tools::mcp::McpBinding;

pub const ROUTE_TOOL: &str = "crew_route";
pub const START_TOOL: &str = "crew_start";
pub const REPORT_TOOL: &str = "crew_report";

/// Whether `name` is a Crew tool.
pub fn is_tool(name: &str) -> bool {
    [ROUTE_TOOL, START_TOOL, REPORT_TOOL].contains(&name) || super::BUS_TOOLS.contains(&name)
}

/// The member id the lead thread has in a crew (the conformance fixtures call it `lead-01`).
pub(super) const LEAD_MEMBER: &str = "lead-01";

/// The tool as `tools/list` shows it.
pub fn tool_specs() -> Vec<Value> {
    let mut tools = vec![
        route_spec(),
        json!({
            "name": START_TOOL,
            "description": "Preview: start a Crew task. `brief` is the brief draft the worker reads \
                (kind, outcome, inputs, files, allowed_effects, stop_when, budget, optionally needs and engine); \
                `answer_key` is what the core checks, as a draft (checks, protected_paths). The core routes the \
                brief, grants the files and runs a worker; the worker never sees the answer key. \
                Only the lead calls this.",
            "inputSchema": {
                "type": "object",
                "properties": { "brief": { "type": "object" }, "answer_key": { "type": "object" } },
                "required": ["brief", "answer_key"],
            },
        }),
        json!({
            "name": REPORT_TOOL,
            "description": "Preview: a Crew worker's result at the end of an attempt. `summary` is required; \
                `branch`, `commit`, `claims` and `not_verified` (what no check covers) are optional. \
                The core then checks the work and tells you the reason if a check fails.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "summary": { "type": "string" },
                    "branch": { "type": "string" },
                    "commit": { "type": "string" },
                    "claims": { "type": "array" },
                    "not_verified": { "type": "array" },
                },
                "required": ["summary"],
            },
        }),
    ];
    tools.extend(super::bus::specs());
    tools
}

fn route_spec() -> Value {
    json!({
        "name": ROUTE_TOOL,
        "description": "Preview: which engine a brief would be routed to and why, without starting anything. \
            `brief` is a Crew brief draft (kind, outcome, inputs, files, allowed_effects, stop_when, budget, \
            optionally needs and engine). Naming an engine in the brief assigns it; it is still checked, never swapped. \
            `role` (builder, code_review, security_review, finding_check, second_key, scout, review), `lane` \
            (with review) and `author_engine` (for the other-vendor rule) are optional.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "brief": { "type": "object" },
                "role": { "type": "string" },
                "lane": { "type": "string" },
                "author_engine": { "type": "string" },
            },
            "required": ["brief"],
        },
    })
}

pub(super) fn invalid(message: impl Into<String>) -> CrewError {
    CrewError::new(ErrorCode::InvalidObject, message)
}

fn token(args: &Value, key: &str) -> Result<Option<CrewToken>, CrewError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => CrewToken::new(text.clone())
            .map(Some)
            .map_err(|why| invalid(format!("{key}: {why}"))),
        Some(_) => Err(invalid(format!("{key}: must be text"))),
    }
}

/// Whole seconds, so a stamp is a plain RFC 3339 time.
pub(super) fn stamp_now() -> Result<CrewTimestamp, CrewError> {
    let now = time::OffsetDateTime::now_utc()
        .replace_nanosecond(0)
        .map_err(|e| invalid(e.to_string()))?;
    let text = now
        .format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| invalid(e.to_string()))?;
    CrewTimestamp::new(text).map_err(invalid)
}

impl Core {
    /// Runs a Crew tool for the MCP service. A refusal is the protocol's error object, so the
    /// caller reads its code. The text and whether it is an error.
    pub async fn crew_tool_text(
        self: &std::sync::Arc<Self>,
        binding: &McpBinding,
        tool: &str,
        args: &Value,
    ) -> (String, bool) {
        if let Err(error) = self.note_tool(binding, tool) {
            tracing::warn!(%error, "could not record a tool call");
        }
        let answer = match tool {
            START_TOOL => self.crew_start_tool(binding, args).await,
            REPORT_TOOL => self.crew_report_tool(binding, args).await,
            bus if super::BUS_TOOLS.contains(&bus) => self.crew_bus_tool(binding, bus, args).await,
            _ => self.crew_route_tool(binding, args).await,
        };
        match answer {
            Ok(answer) => (answer.to_string(), false),
            Err(error) => (json!(error).to_string(), true),
        }
    }

    /// Whether the Crew tools are on. Off, `crew_route` does not exist for any caller.
    pub fn crew_enabled(&self) -> bool {
        self.config.crew_preview
    }

    /// The dry run. Its refusals are Crew errors (SPEC.md section 9); a brief nobody can take
    /// is not an error but an answer, with every rejection and its reason.
    pub async fn crew_route_tool(
        &self,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        if !self.crew_enabled() {
            return Err(CrewError::new(ErrorCode::UnknownTool, "crew_route"));
        }
        // The router's view is the lead's: a worker's binding cannot ask it.
        if !matches!(self.crew_caller(binding)?, super::start::Caller::Lead) {
            return Err(CrewError::new(
                ErrorCode::NotAuthorized,
                "only the lead routes a brief",
            ));
        }
        let Some(fields) = args.as_object() else {
            return Err(invalid("arguments: an object"));
        };
        if let Some(extra) = fields
            .keys()
            .find(|key| !["brief", "role", "lane", "author_engine"].contains(&key.as_str()))
        {
            return Err(invalid(format!("{extra}: not an argument of crew_route")));
        }
        let draft = fields
            .get("brief")
            .ok_or_else(|| invalid("brief: required"))?;
        let thread = self
            .store
            .thread(&binding.scope.thread_id)
            .map_err(|e| invalid(e.public_message()))?;
        let stamper = Stamper {
            crew_id: CrewId::new(thread.id.clone()).map_err(invalid)?,
            engine: CrewToken::new(binding.scope.family.clone())
                .map_err(|why| invalid(format!("engine: {why}")))?,
            member: CrewMember::new(LEAD_MEMBER).map_err(invalid)?,
            now: stamp_now()?,
        };
        let brief = stamper.brief(draft)?;
        let role: Option<RouteRole> = match fields.get("role") {
            None | Some(Value::Null) => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|_| invalid("role: not a route role"))?,
            ),
        };
        let lane = token(args, "lane")?;
        let author_engine = token(args, "author_engine")?;
        // H2, after the shape: nothing the lead sent holds a secret.
        refuse_secret(args)?;

        let machine = self
            .crew_machine(Some(&thread.project_id))
            .await
            .map_err(|e| invalid(e.public_message()))?;
        let input = RouteInput {
            brief: &brief,
            run_id: CrewId::new(crate::util::new_id()).map_err(invalid)?,
            created_at: stamper.now.clone(),
            role,
            lane,
            author_engine,
            default_engine: machine.default_engine.clone(),
        };
        let answer = match route(
            &input,
            &machine.views,
            &RouterConfig::default(),
            &EvalHistory::default(),
        ) {
            Ok(decision) => {
                let alternatives: Vec<&orbyn_contracts::RouteScore> = decision
                    .scores
                    .iter()
                    .filter(|s| s.engine != decision.engine)
                    .collect();
                json!({
                    "dry_run": true,
                    "routed": true,
                    "decision": CrewObject::RouteDecision(decision.clone()).to_value(),
                    "alternatives": alternatives,
                    "left_out": machine.left_out,
                })
            }
            Err(RouteFailure::Invalid(error)) => return Err(error),
            Err(RouteFailure::Refused(refusal)) => json!({
                "dry_run": true,
                "routed": false,
                "refusal": {
                    "message": refusal.message,
                    "requested": refusal.requested,
                    "rejected": refusal.rejected,
                },
                "left_out": machine.left_out,
            }),
        };
        // The answer is made of this machine's engines and models; prove it holds no secret.
        refuse_secret(&answer)?;
        Ok(answer)
    }
}
