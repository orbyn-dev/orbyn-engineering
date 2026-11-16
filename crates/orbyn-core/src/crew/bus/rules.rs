//! What the guards count, as pure functions over the stored messages (SPEC.md H4). Nothing
//! here reads a clock, a store or a process.

use orbyn_contracts::{CrewMessage, CrewRole, MessageKind};

/// Messages between members `a` and `b` since the lead last stepped in with either of them.
///
/// Counted both ways. A message to the lead or to everyone is not part of any pair, and the
/// core's own notices are not a member's. A message from the lead to `a` or to `b` starts the
/// count again, for this pair and for every other pair that member is in (H4); a lead message
/// to `all` or to a third member resets nothing.
pub fn exchanges<'a>(log: impl IntoIterator<Item = &'a CrewMessage>, a: &str, b: &str) -> u32 {
    let mut count = 0;
    for message in log {
        let to = message.to.as_str();
        match message.author.role {
            CrewRole::Lead if to == a || to == b => count = 0,
            CrewRole::Worker | CrewRole::Reviewer => {
                let from = message.author.member.as_ref().map_or("", |m| m.as_str());
                if (from == a && to == b) || (from == b && to == a) {
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

/// Whether `message` was sent to `member`: addressed to it, or to `all` by somebody else.
/// The lead is addressed as `lead`.
pub fn is_for(message: &CrewMessage, member: &str, lead: bool) -> bool {
    let from_me = message
        .author
        .member
        .as_ref()
        .is_some_and(|m| m.as_str() == member);
    match message.to.as_str() {
        "all" => !from_me,
        "lead" => lead,
        to => to == member,
    }
}

/// The answer to `question` that was sent to `asker`, if there is one.
pub fn answer_to<'a>(
    log: impl IntoIterator<Item = &'a CrewMessage>,
    question: &str,
    asker: &str,
) -> Option<&'a CrewMessage> {
    log.into_iter().find(|message| {
        message.kind == MessageKind::Answer
            && message
                .in_reply_to
                .as_ref()
                .is_some_and(|q| q.as_str() == question)
            && (message.to.as_str() == asker || message.to.as_str() == "all")
    })
}
