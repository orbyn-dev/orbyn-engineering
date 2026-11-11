//! `crew_post`: one message from a member to a member, the lead or everyone. The order of the
//! refusals is the spec's (SPEC.md section 9): the run (H7), the post limit (H5), the shape
//! (H9), the secret scan (H2), then the rule's own (H1, H4, H6).

use std::sync::Arc;

use orbyn_contracts::{
    CrewError, CrewId, CrewMessage, CrewObject, CrewRole, ErrorCode, MessageKind,
};
use serde_json::{Value, json};

use super::deliver::Target;
use super::rules::{exchanges, is_for};
use super::{ASK_TOOL, POST_TOOL, Sender};
use crate::app::Core;
use crate::crew::guard::{refuse_leak, refuse_secret};
use crate::crew::leases::internal;
use crate::crew::stamp::Stamper;
use crate::crew::tool::{invalid, stamp_now};
use crate::error::CoreError;
use crate::store::{CrewRun, CrewTask};
use crate::tools::mcp::McpBinding;

/// Where a message is stored and who receives it.
struct Route {
    /// The task whose run the message belongs to.
    task: CrewTask,
    targets: Vec<Target>,
    /// The other member of the pair the hop limit counts, for a message to one member.
    pair: Option<String>,
}

/// What a send came to.
pub(super) struct Sent {
    pub who: Sender,
    pub message: CrewMessage,
    /// `(recipient, mode)` for each recipient reached now.
    pub delivered: Vec<(String, String)>,
    /// The same message was already stored: nothing was counted or delivered again.
    pub duplicate: bool,
}

fn target_of(task: &CrewTask, run: &CrewRun) -> Target {
    Target {
        recipient: run.member.clone(),
        thread: run.worker_thread_id.clone(),
        // Its state is read again when the text is handed over (a worker being verified is
        // not handed anything).
        task: Some(task.id.clone()),
        guard: true,
    }
}

/// How a message is addressed to its sender's side: `lead` for the lead, else the member id.
fn address_of(message: &CrewMessage) -> String {
    match (message.author.role, &message.author.member) {
        (CrewRole::Lead, _) => "lead".to_owned(),
        (_, Some(member)) => member.as_str().to_owned(),
        _ => String::new(),
    }
}

impl Core {
    /// Where a message to `to` from `who` is stored, and who gets it (H7: a recipient whose
    /// run has ended refuses it as well).
    fn bus_route(
        &self,
        who: &Sender,
        members: &[(CrewTask, CrewRun)],
        to: &str,
    ) -> Result<Route, CrewError> {
        let live = |task: &CrewTask| !crate::crew::lifecycle::is_final(&task.state);
        let own = who
            .task
            .as_deref()
            .and_then(|id| members.iter().find(|(task, _)| task.id == id));
        match to {
            "lead" => {
                let (task, _) =
                    own.ok_or_else(|| invalid("to: the lead does not message itself"))?;
                Ok(Route {
                    task: task.clone(),
                    targets: vec![Target {
                        recipient: "lead".into(),
                        thread: Some(who.lead_thread.clone()),
                        task: None,
                        guard: false,
                    }],
                    pair: None,
                })
            }
            "all" => {
                let targets: Vec<Target> = members
                    .iter()
                    .filter(|(task, run)| live(task) && run.member != who.member.as_str())
                    .map(|(task, run)| target_of(task, run))
                    .collect();
                // A worker's broadcast is stored under its own run; the lead's under the newest
                // run that is live.
                let task = match own {
                    Some((task, _)) => task,
                    None => members
                        .iter()
                        .rev()
                        .map(|(task, _)| task)
                        .find(|task| live(task))
                        .ok_or_else(|| CrewError::new(ErrorCode::RunEnded, "no run is live"))?,
                };
                Ok(Route {
                    task: task.clone(),
                    targets,
                    pair: None,
                })
            }
            member => {
                if member == who.member.as_str() {
                    return Err(invalid("to: a member does not message itself"));
                }
                let (task, run) = members
                    .iter()
                    .find(|(_, run)| run.member == member)
                    .ok_or_else(|| {
                        CrewError::new(
                            ErrorCode::UnknownMember,
                            format!("{member} is not in this crew"),
                        )
                    })?;
                if !live(task) {
                    return Err(CrewError::new(
                        ErrorCode::RunEnded,
                        format!("the run of {member} has ended"),
                    ));
                }
                Ok(Route {
                    task: own.map_or_else(|| task.clone(), |(own, _)| own.clone()),
                    targets: vec![target_of(task, run)],
                    pair: Some(member.to_owned()),
                })
            }
        }
    }

    /// The common part of `crew_post` and `crew_ask`: guards, stamp, store, deliver.
    pub(super) async fn send_crew_message(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
        tool: &str,
    ) -> Result<Sent, CrewError> {
        let who = self.bus_sender(binding, tool)?;
        let limits = self.crew_limits();
        let Some(fields) = args.as_object() else {
            return Err(invalid("arguments: an object"));
        };
        let mut draft = Value::Object(fields.clone());
        {
            let map = draft.as_object_mut().expect("an object");
            if tool == ASK_TOOL {
                match map.get("kind") {
                    None => {
                        map.insert("kind".into(), json!("question"));
                    }
                    Some(kind) if kind == "question" => {}
                    Some(_) => return Err(invalid("kind: crew_ask sends a question")),
                }
            } else if map.contains_key("wait_seconds") {
                return Err(invalid("wait_seconds: only crew_ask waits"));
            }
        }
        // One bus writer at a time: the counts below and the store they guard are one step.
        let guard = self.crew_bus.lock.lock().await;
        let crew = who.crew_id.as_str();
        let members = self.store.crew_tasks_in_crew(crew).map_err(internal)?;

        // H5: a worker posts at most `posts` times in its run; the lead is not counted.
        if let Some(task) = &who.task {
            let posted = self
                .store
                .count_crew_posts(task, who.member.as_str())
                .map_err(internal)?;
            if posted >= limits.posts {
                let error = CrewError::new(
                    ErrorCode::PostLimitExceeded,
                    format!("{} reached {} posts in this run", who.member, limits.posts),
                );
                self.tell_lead_of(&who, &error);
                return Err(error);
            }
        }

        let to = fields
            .get("to")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("to: required, a member id, `lead` or `all`"))?;
        // A reserved word that is not an addressee (`core`, `person`) is a bad draft.
        orbyn_contracts::CrewAddressee::new(to).map_err(|why| invalid(format!("to: {why}")))?;
        let route = self.bus_route(&who, &members, to)?;
        let run = CrewId::new(route.task.run_id.clone()).map_err(invalid)?;
        let stamper = Stamper {
            crew_id: who.crew_id.clone(),
            engine: who.engine.clone(),
            member: who.member.clone(),
            now: stamp_now()?,
        };
        let message = stamper.message(&draft, &run, who.role)?;
        if message.body.as_str().chars().count() > limits.body_chars {
            return Err(invalid(format!(
                "body: at most {} characters",
                limits.body_chars
            )));
        }
        // H2, after the shape.
        let object = CrewObject::Message(message.clone()).to_value();
        refuse_secret(&object)?;
        // H1: what reaches a worker holds nothing of any key in the crew. The lead's own stays
        // with the lead.
        if message.to.as_str() != "lead" {
            refuse_leak(&object, &self.crew_key_needles(crew)?)?;
        }

        let log = self.store.crew_messages_in_crew(crew).map_err(internal)?;
        if log.iter().any(|(_, m)| m.id == message.id) {
            return Ok(Sent {
                who,
                message,
                delivered: Vec::new(),
                duplicate: true,
            });
        }
        // H6: a reply names a question that was put to the sender.
        let mut question: Option<&CrewMessage> = None;
        if let Some(reply) = &message.in_reply_to {
            question = log
                .iter()
                .map(|(_, m)| m)
                .find(|m| m.id == *reply && m.kind == MessageKind::Question);
            let me = who.member.as_str();
            if !question.is_some_and(|q| is_for(q, me, who.is_lead())) {
                return Err(invalid(
                    "in_reply_to: no question put to you has this id".to_string(),
                ));
            }
        }
        // H4: at most `exchanges` messages between two members; the lead is told of the next.
        if who.role == CrewRole::Worker
            && let Some(other) = &route.pair
        {
            let pair = exchanges(log.iter().map(|(_, m)| m), who.member.as_str(), other);
            if pair >= limits.exchanges {
                let error = CrewError::new(
                    ErrorCode::HopLimitExceeded,
                    format!(
                        "{} and {other} reached {} messages; the lead decides how they go on",
                        who.member, limits.exchanges
                    ),
                );
                self.tell_lead_of(&who, &error);
                return Err(error);
            }
        }
        self.store
            .add_crew_member_object(&route.task.id, &CrewObject::Message(message.clone()))
            .map_err(|error| match error {
                CoreError::RunClosed => CrewError::new(ErrorCode::RunEnded, "this run has ended"),
                other => internal(other),
            })?;
        // An answer to a question somebody waits on is returned to that `crew_ask` itself.
        let inline = question
            .filter(|q| {
                message.kind == MessageKind::Answer
                    && self
                        .crew_bus
                        .waiting
                        .lock()
                        .is_ok_and(|waiting| waiting.contains(q.id.as_str()))
            })
            .map(address_of);
        if tool == ASK_TOOL
            && let Ok(mut waiting) = self.crew_bus.waiting.lock()
        {
            waiting.insert(message.id.as_str().to_owned());
        }
        let needles = self.crew_key_needles(crew)?;
        // H4: a lead message to one member restarts the count of every pair that member is in. The
        // ledger records each pair with its count before, so real runs show whether a routine message
        // re-armed a looping pair.
        if who.role == CrewRole::Lead
            && let Some(member) = &route.pair
        {
            let others: Vec<String> = members.iter().map(|(_, run)| run.member.clone()).collect();
            let pairs = super::ledger::rearmed(log.iter().map(|(_, m)| m), member, &others);
            self.record_hop_reset(&route.task, &message, member, &pairs, &needles);
        }
        drop(guard);
        self.crew_bus.wake.notify_waiters();

        let mut delivered = Vec::new();
        for target in &route.targets {
            if inline.as_deref() == Some(target.recipient.as_str()) {
                let thread = target.thread.clone().unwrap_or_default();
                let _ = self.store.record_crew_delivery(
                    message.id.as_str(),
                    &target.recipient,
                    &thread,
                    "live",
                    Some("returned to the crew_ask that is waiting for it"),
                );
                delivered.push((target.recipient.clone(), "live".to_owned()));
                continue;
            }
            let mode = self.deliver_crew_message(&message, target, &needles);
            delivered.push((target.recipient.clone(), mode.to_owned()));
        }
        Ok(Sent {
            who,
            message,
            delivered,
            duplicate: false,
        })
    }

    /// `crew_post`.
    pub async fn crew_post_tool(
        self: &Arc<Self>,
        binding: &McpBinding,
        args: &Value,
    ) -> Result<Value, CrewError> {
        let sent = self.send_crew_message(binding, args, POST_TOOL).await?;
        Ok(json!({
            "id": sent.message.id,
            "duplicate": sent.duplicate,
            "delivered": sent.delivered.iter()
                .map(|(to, mode)| json!({ "to": to, "mode": mode }))
                .collect::<Vec<_>>(),
        }))
    }
}
