//! Autopilot at the gate: every request of a Crew worker passes the always-denied list, the trip
//! wires and the chain before a rule or a person is asked (see the [README](README.md)). The
//! gate calls this through [`ActionReview`], bound by `McpService::bind_host` only when the Crew
//! Preview flag is on. Outside a Crew task only N1, the account, is enforced.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use async_trait::async_trait;
use orbyn_contracts::{ActionKind, BriefEffect, LeaseMode, ResultState, StopReason};
use serde_json::{Value, json};

use super::blast::{self, Blast, Facts, read_remotes};
use super::chain::{Approvers, Outcome, Summary, decide};
use super::hard::{self, Ctx, Scene, context, refuse_in};
use super::tier::{Grants, Need, tier};
use super::tripwire::{injection_in, injection_in_value, tripped};
use super::{Mode, ledger, read_allowed_hosts};
use crate::app::Core;
use crate::crew::lifecycle::is_final;
use crate::crew::wire_name;
use crate::error::Result;
use crate::governance::approvals::{ActionAsk, ActionReview, Review, RunScope};
use crate::store::{CrewRun, CrewTask};
use crate::util::unix_now;

/// Where a mode is chosen. A crew's choice beats its repository's, which beats the default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModeScope {
    Default,
    Repo(String),
    Crew(String),
}

/// The store's key for a scope.
pub fn scope_key(scope: &ModeScope) -> String {
    match scope {
        ModeScope::Default => "default".to_owned(),
        ModeScope::Repo(id) => format!("repo:{id}"),
        ModeScope::Crew(id) => format!("crew:{id}"),
    }
}

/// What Core keeps for Autopilot: the approvers, set by whoever wires the lead and a second engine.
#[derive(Default)]
pub struct Autopilot {
    pub(crate) approvers: Mutex<Approvers>,
    /// Commands held for a person, in memory only ([`super::held`]).
    pub(crate) held: Mutex<super::HeldBook>,
    /// Guarded commands let through, waiting for their exit status ([`super::exit`]), in memory only.
    pub(crate) exits: super::exit::ExitWatches,
}

/// Where a task's worktree is and what the always-denied list needs to know about it, read once per check.
pub(super) struct Ground {
    worktree: PathBuf,
    hosts: Vec<String>,
    branch: Option<String>,
    default: Option<String>,
    data: PathBuf,
}

impl Ground {
    pub(super) async fn read(core: &Core, worktree: &Path, base: Option<&str>) -> Self {
        Self {
            worktree: worktree.to_owned(),
            hosts: read_allowed_hosts(worktree, base).await,
            branch: current_branch(worktree).await,
            default: default_branch(worktree).await,
            data: core.config.data_dir.clone(),
        }
    }

    pub(super) fn scene<'a>(
        &'a self,
        protected: &'a [orbyn_contracts::CrewPathPattern],
    ) -> Scene<'a> {
        Scene {
            worktree: &self.worktree,
            protected,
            allowed_hosts: &self.hosts,
            data_dir: Some(&self.data),
            branch: self.branch.as_deref(),
            default_branch: self.default.as_deref(),
        }
    }
}

/// The gate's side: a weak hold on the core, so the gate does not keep it alive.
pub(crate) struct GateReview(pub Weak<Core>);

#[async_trait]
impl ActionReview for GateReview {
    async fn review(&self, scope: &RunScope, ask: &ActionAsk) -> Review {
        match self.0.upgrade() {
            Some(core) => core.crew_review_action(scope, ask).await,
            None => Review::Pass,
        }
    }
}

impl Core {
    /// Sets who approves for the lead and who may be a second key. A second key is the first of
    /// `others` on another vendor than the lead.
    pub fn set_crew_approvers(&self, approvers: Approvers) {
        *self
            .crew_autopilot
            .approvers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = approvers;
    }

    /// The person's choice for a scope.
    pub fn set_crew_autopilot_mode(&self, scope: &ModeScope, mode: Mode) -> Result<()> {
        self.store
            .set_crew_autopilot_mode(&scope_key(scope), mode.name())
    }

    /// The mode in force: the crew's, else the repository's, else the default, else `Full`.
    pub fn crew_autopilot_mode(&self, project_id: &str, crew_id: &str) -> Mode {
        [
            ModeScope::Crew(crew_id.to_owned()),
            ModeScope::Repo(project_id.to_owned()),
            ModeScope::Default,
        ]
        .iter()
        .find_map(|scope| {
            self.store
                .crew_autopilot_mode_of(&scope_key(scope))
                .ok()
                .flatten()
                .and_then(|text| Mode::parse(&text))
        })
        .unwrap_or_default()
    }

    fn account_only(&self, ask: &ActionAsk) -> Review {
        if hard::account_refused(ask.kind, &ask.target, &ask.paths) {
            Review::Deny(denied_words("N1", hard::RULES[0].1))
        } else {
            Review::Pass
        }
    }

    /// The whole check for one action. Any failure to check is a denial.
    pub(crate) async fn crew_review_action(
        self: &Arc<Self>,
        scope: &RunScope,
        ask: &ActionAsk,
    ) -> Review {
        let found = self.store.crew_task_for_worker(&scope.thread_id);
        let Ok(found) = found else {
            return Review::Deny("Orbyn could not check this action.".to_owned());
        };
        let Some((task, run)) = found else {
            return self.account_only(ask);
        };
        match self.review_for_task(&task, &run, scope, ask).await {
            Ok(review) => review,
            Err(error) => {
                tracing::warn!(%error, "autopilot could not check an action; denying");
                Review::Deny("Orbyn could not check this action.".to_owned())
            }
        }
    }

    async fn review_for_task(
        self: &Arc<Self>,
        task: &CrewTask,
        run: &CrewRun,
        scope: &RunScope,
        ask: &ActionAsk,
    ) -> Result<Review> {
        if is_final(&task.state) {
            return Ok(Review::Deny("This task has ended.".to_owned()));
        }
        let (Some(brief), Some(key)) = (
            self.store.crew_brief(&task.id)?,
            self.store.crew_answer_key(&task.id)?,
        ) else {
            return Ok(Review::Deny(
                "This task has no brief to check against.".to_owned(),
            ));
        };
        let needles = crate::crew::key_needles(&key);
        let Some(worktree) = run
            .worktree
            .as_deref()
            .map(PathBuf::from)
            .or_else(|| scope.write_scope.as_ref().map(|w| w.checkout.clone()))
        else {
            return Ok(Review::Deny(
                "Orbyn could not find this task's worktree.".to_owned(),
            ));
        };
        let project = self.store.thread(&run.lead_thread_id)?.project_id;
        let mode = self.crew_autopilot_mode(&project, &task.crew_id);
        let ground = Ground::read(self, &worktree, run.base_commit.as_deref()).await;
        let scene = ground.scene(&key.protected_paths);
        let ctx = context(ask.kind, &ask.target, &ask.paths, &scene);
        let action =
            json!({ "kind": wire_name(&ask.kind), "target": ask.target, "paths": ask.paths });

        // Trip wire: text that reads as injected instructions. A file's own content is not scanned.
        let mut scanned = injection_in(&ask.summary).or_else(|| injection_in(&ask.target));
        if ask.kind != ActionKind::FileChange {
            scanned = scanned.or_else(|| injection_in_value(&ask.detail));
        }
        if let Some(class) = scanned {
            let body = json!({ "wire": "injection", "class": class, "action": action });
            self.autopilot_record(task, "tripwire", &body, &needles)?;
            self.autopilot_stop(
                task,
                StopReason::InjectionSuspected,
                "injection_suspected",
                "The task stopped: a request held text that looks like injected instructions.",
            );
            return Ok(Review::Deny(
                "The task stopped: a request held text that looks like injected instructions."
                    .to_owned(),
            ));
        }

        // The always-denied list: before any approver, rule or mode.
        if let Some(refusal) = refuse_in(&ctx) {
            let body = json!({ "action": action, "decided_by": "hard_rule", "rule": refusal.id, "name": refusal.rule, "mode": mode.name() });
            return self.autopilot_denied(
                task,
                &body,
                &denied_words(refusal.id, refusal.rule),
                &needles,
            );
        }

        // The blast-radius guard: a destructive command against a production-linked target needs a second key, even in Full. It is
        // above the tier (a check command or a lease does not exempt it) and below N3 and N8, which have already refused.
        let guard = self.guard_of(&ctx, &worktree).await?;

        // A person's one-time approval of exactly this command, now that the list above has passed it again. A guarded command's
        // approval also holds only while its targets are the ones that were held (a `git remote set-url` in between voids it).
        let resolved: &[String] = guard.as_ref().map_or(&[], |g| g.resolved.as_slice());
        if let Some(taken) = self.held_take(task, &scope.thread_id, ask, resolved, unix_now()) {
            let mut body = json!({ "action": action, "decided_by": "person", "hold": taken.id, "mode": mode.name() });
            if let Some(guard) = &taken.guard {
                body["guard"] = guard.clone();
            }
            let row = self.autopilot_record(task, "allowed", &body, &needles)?;
            if taken.guard.is_some() {
                self.exit_watch(
                    task,
                    &scope.thread_id,
                    ask,
                    Some(&taken.id),
                    Some(row),
                    taken.guard.as_ref(),
                    unix_now(),
                );
            }
            return Ok(Review::Approved);
        }

        let writes: Vec<_> = brief
            .files
            .iter()
            .filter(|g| g.mode == LeaseMode::Write)
            .map(|g| g.path.clone())
            .collect();
        let commands: Vec<String> = key
            .checks
            .iter()
            .filter_map(orbyn_contracts::CrewCheck::command)
            .collect();
        let mut tiered = tier(
            &ctx,
            &Grants {
                writes: &writes,
                check_commands: &commands,
            },
        );
        if guard.is_some() {
            tiered.need = Need::Guarded;
        }
        let mut summary = Summary {
            kind: wire_name(&ask.kind),
            target: ledger::shown(&ask.target, &needles),
            paths: ask
                .paths
                .iter()
                .take(20)
                .map(|p| ledger::shown(p, &needles))
                .collect(),
            brief: ledger::shown(&brief.outcome, &needles),
            effects: brief.allowed_effects.iter().map(effect_name).collect(),
            tier: tiered.need.tier().to_owned(),
            reasons: tiered.reasons.iter().map(|r| (*r).to_owned()).collect(),
        };
        if let Some(found) = &guard {
            summary.reasons.insert(
                0,
                format!(
                    "it can destroy data on a production-linked target ({}: {}, target {})",
                    wire_name(&found.category),
                    found.matched_by,
                    found.target
                ),
            );
        }
        let guard_body = guard.as_ref().map(guard_body);
        let approvers = self
            .crew_autopilot
            .approvers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let outcome = decide(mode, tiered.need, &summary, &approvers).await;
        // A command the chain refused is held for a person, who may approve that one command once.
        if let Outcome::Deny { reason, .. } = &outcome {
            self.held_make_guarded(
                task,
                &scope.thread_id,
                ask,
                reason,
                &needles,
                guard_body.as_ref(),
                unix_now(),
            );
        }
        let settled = Settled {
            ask,
            thread: &scope.thread_id,
            action: &action,
            guard: guard_body.as_ref(),
        };
        self.settle_review(task, &settled, &summary, mode, outcome, &needles)
    }

    /// What the blast-radius guard holds in this command, if anything. It reads this device's two lists and, for a push, the
    /// worktree's own remotes. A list that cannot be read is an error, so the action is denied: not knowing is never a reason to allow.
    pub(super) async fn guard_of(&self, ctx: &Ctx<'_>, worktree: &Path) -> Result<Option<Blast>> {
        let pushes = ctx
            .commands
            .iter()
            .any(|c| c.program == "git" && c.args.iter().any(|a| a == "push"));
        let remotes = if pushes {
            read_remotes(worktree).await
        } else {
            std::collections::HashMap::new()
        };
        let facts = Facts::new(self.store.crew_guard_targets()?, remotes);
        Ok(blast::blast(ctx, &facts))
    }

    fn settle_review(
        self: &Arc<Self>,
        task: &CrewTask,
        settled: &Settled<'_>,
        summary: &Summary,
        mode: Mode,
        outcome: Outcome,
        needles: &[String],
    ) -> Result<Review> {
        let action = settled.action;
        let answer = |a: &super::Answer| json!({ "approve": a.approve, "reason": a.reason });
        let base = |decided_by: &str| {
            let mut body = json!({ "action": action, "decided_by": decided_by, "tier": summary.tier, "reasons": summary.reasons, "mode": mode.name() });
            if let Some(guard) = settled.guard {
                body["guard"] = guard.clone();
            }
            body
        };
        match outcome {
            Outcome::Allow {
                decided_by,
                lead,
                second,
            } => {
                let mut body = base(decided_by);
                body["lead"] = lead.as_ref().map_or(Value::Null, answer);
                body["second"] = second.as_ref().map_or(
                    Value::Null,
                    |(engine, a)| json!({ "engine": engine, "answer": answer(a) }),
                );
                let row = self.autopilot_record(task, "allowed", &body, needles)?;
                if settled.guard.is_some() {
                    self.exit_watch(
                        task,
                        settled.thread,
                        settled.ask,
                        None,
                        Some(row),
                        settled.guard,
                        unix_now(),
                    );
                }
                Ok(Review::Approved)
            }
            Outcome::Person { why } => {
                let mut body = base("person");
                body["why"] = json!(why);
                self.autopilot_record(task, "person", &body, needles)?;
                Ok(Review::Person)
            }
            Outcome::Deny {
                decided_by,
                reason,
                lead,
                second,
                disagreement,
                needs_second_engine,
            } => {
                let mut body = base(decided_by);
                body["lead"] = lead.as_ref().map_or(Value::Null, answer);
                body["second"] = second.as_ref().map_or(
                    Value::Null,
                    |(engine, a)| json!({ "engine": engine, "answer": answer(a) }),
                );
                body["needs_second_engine"] = json!(needs_second_engine);
                body["reason"] = json!(reason);
                if disagreement {
                    self.autopilot_record(task, "disagreement", &body, needles)?;
                }
                self.autopilot_denied(
                    task,
                    &body,
                    &format!("Denied: {}", ledger::shown(&reason, needles)),
                    needles,
                )
            }
        }
    }

    /// Records a denial, trips the wire if the counts say so, and returns what the requester reads.
    fn autopilot_denied(
        self: &Arc<Self>,
        task: &CrewTask,
        body: &Value,
        words: &str,
        needles: &[String],
    ) -> Result<Review> {
        self.autopilot_record(task, "denied", body, needles)?;
        let (in_a_row, in_a_run) = self.store.crew_autopilot_denials(&task.id)?;
        if tripped(in_a_row, in_a_run) {
            let wire = json!({ "wire": "denials", "in_a_row": in_a_row, "in_a_run": in_a_run });
            self.autopilot_record(task, "tripwire", &wire, needles)?;
            self.autopilot_stop(
                task,
                StopReason::TooManyDenials,
                "too_many_denials",
                "The task stopped: too many denials.",
            );
            return Ok(Review::Deny(
                "The task stopped: too many denials.".to_owned(),
            ));
        }
        Ok(Review::Deny(words.to_owned()))
    }

    /// Stops a task through the lifecycle, so its leases end, its worker's run is cancelled and the lead is told.
    pub(crate) fn autopilot_stop(
        self: &Arc<Self>,
        task: &CrewTask,
        reason: StopReason,
        name: &str,
        why: &str,
    ) {
        match self.store.move_crew_task(&task.id, "stopped", Some(name)) {
            Ok(moved) => self.end_crew_task(&moved, ResultState::Stopped, Some(reason), why, None),
            Err(error) => tracing::debug!(%error, "autopilot found the task already moved"),
        }
    }
}

/// What a settled review needs besides the outcome.
struct Settled<'a> {
    ask: &'a ActionAsk,
    thread: &'a str,
    action: &'a Value,
    guard: Option<&'a Value>,
}

/// The ledger's account of why the guard held a command: its category, the target (redacted when the row is stored), what matched
/// and the list that forced it, if one did.
fn guard_body(found: &Blast) -> Value {
    json!({
        "category": wire_name(&found.category),
        "target": found.target,
        "matched_by": found.matched_by,
        "list": found.list,
        "resolved": found.resolved,
    })
}

fn denied_words(id: &str, rule: &str) -> String {
    format!("Denied by Autopilot ({id}): {rule}. No approval can change this.")
}

fn effect_name(effect: &BriefEffect) -> String {
    wire_name(effect)
}

async fn current_branch(worktree: &Path) -> Option<String> {
    let out = crate::source_control::git::git(worktree, &["symbolic-ref", "--short", "-q", "HEAD"])
        .await
        .ok()?;
    let name = crate::source_control::git::text(&out);
    (!name.is_empty()).then_some(name)
}

/// The repository's default branch as `origin/HEAD` names it, when there is such a remote.
async fn default_branch(worktree: &Path) -> Option<String> {
    let out = crate::source_control::git::git(
        worktree,
        &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
    )
    .await
    .ok()?;
    let name = crate::source_control::git::text(&out);
    name.strip_prefix("origin/").map(str::to_owned)
}
