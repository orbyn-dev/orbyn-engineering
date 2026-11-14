//! The message and fact reads of the Crew Map and Lanes (Crew Preview). Both read by labels only, in SQL: a message's body and a
//! fact's value are never loaded into the service, so a hostile worker's text cannot reach a screen through them (H1). A message is
//! its author, addressee, kind, time and delivery, with a label the core wrote for the kind; a fact is its key, class, author,
//! revision, time and whether it names evidence.

use orbyn_contracts as wire;

use super::map::{delivery_of, label_of};
use super::scrub::Scrub;
use crate::app::Core;
use crate::crew::{Page, valid_id};
use crate::error::Result;

/// A time as it is read out of a stored body or a delivery row: a short run of the characters of an RFC 3339 time, or nothing.
pub(super) fn time_shape(text: Option<String>) -> Option<String> {
    text.filter(|t| {
        (10..=40).contains(&t.len())
            && t.chars()
                .all(|c| c.is_ascii_digit() || matches!(c, '-' | ':' | 'T' | 'Z' | '.' | '+'))
    })
}

/// A message hash as it is read out of a stored body: `sha256:` and 64 hex digits, or nothing.
pub(super) fn hash_shape(text: Option<String>) -> Option<String> {
    text.filter(|t| orbyn_contracts::CrewHash::new(t.clone()).is_ok())
}

/// How many recipients a message to `all` went to, rebuilt from the crew's tasks, since the fan-out's recipients are not stored: the
/// members other than the sender whose task had begun and had not ended when the message was written. `None` when the message's time
/// cannot be read, so nothing is judged. It errs toward counting a recipient (a task that began in the same second), so a doubtful
/// fan-out reports no delivery rather than a delivery it cannot back.
pub(super) fn expected_recipients(
    members: &[crate::store::MemberTimes],
    sender: &str,
    at: Option<&str>,
) -> Option<u32> {
    let at = crate::util::parse_time(at?)?;
    let live: std::collections::BTreeSet<&str> = members
        .iter()
        .filter(|m| m.member != sender)
        .filter(|m| crate::util::parse_time(&m.created_at).is_none_or(|created| created <= at))
        .filter(|m| {
            !crate::crew::lifecycle::is_final(&m.state)
                || crate::util::parse_time(&m.updated_at).is_none_or(|ended| ended >= at)
        })
        .map(|m| m.member.as_str())
        .collect();
    u32::try_from(live.len()).ok()
}

/// A delivery as reported: none while a fan-out to `all` still lacks a recipient's delivery row.
pub(super) fn settled(
    mode: Option<wire::FlowDeliveryMode>,
    recipient: &str,
    rows: u32,
    expected: Option<u32>,
) -> Option<wire::FlowDeliveryMode> {
    match (recipient, expected) {
        ("all", Some(expected)) if rows < expected => None,
        _ => mode,
    }
}

/// The most board keys one call shows.
pub const FACTS_MAX: usize = 100;

impl Core {
    /// A page of a crew's messages, newest first. `member` keeps those the member sent or was sent.
    pub fn crew_flow_messages(
        &self,
        crew: &str,
        member: Option<&str>,
        page: Page,
    ) -> Result<wire::FlowMessageList> {
        self.crew_preview_only()?;
        valid_id(crew)?;
        if let Some(member) = member {
            valid_id(member)?;
        }
        let person = self.display_redactor()?;
        let scrub = Scrub::of_crew(self, &person, crew)?;
        let (rows, total) =
            self.store
                .crew_message_labels(crew, member, page.limit, page.offset)?;
        let members = if rows.iter().any(|m| m.recipient == "all") {
            self.store.crew_member_times(crew, 200)?
        } else {
            Vec::new()
        };
        let seen = u64::from(page.offset) + rows.len() as u64;
        Ok(wire::FlowMessageList {
            messages: rows
                .into_iter()
                .map(|m| {
                    let expected = expected_recipients(&members, &m.sender, m.at.as_deref());
                    let delivery = settled(
                        delivery_of(m.mode.as_deref()),
                        &m.recipient,
                        m.recipients,
                        expected,
                    );
                    wire::FlowMessage {
                        label: label_of(&m.kind).to_owned(),
                        id: m.id,
                        at: time_shape(m.at).unwrap_or_default(),
                        from: scrub.text(&m.sender, 60),
                        to: scrub.text(&m.recipient, 60),
                        kind: scrub.text(&m.kind, 20),
                        in_reply_to: hash_shape(m.in_reply_to),
                        delivered_at: delivery.and(time_shape(m.delivered_at)),
                        delivery,
                    }
                })
                .collect(),
            total,
            next_offset: (seen < u64::from(total)).then(|| u32::try_from(seen).unwrap_or(u32::MAX)),
        })
    }

    /// A crew's board keys by labels, newest first, at most 100. No value is read.
    pub fn crew_flow_facts(&self, crew: &str) -> Result<wire::FlowFactList> {
        self.crew_preview_only()?;
        valid_id(crew)?;
        let person = self.display_redactor()?;
        let scrub = Scrub::of_crew(self, &person, crew)?;
        let (rows, total) = self.store.crew_fact_labels(crew, FACTS_MAX as u32)?;
        Ok(wire::FlowFactList {
            facts: rows
                .into_iter()
                .map(|f| wire::FlowFact {
                    key: scrub.text(&f.key, 120),
                    class: scrub.text(f.class.as_deref().unwrap_or(""), 20),
                    author: scrub.text(&f.author, 60),
                    rev: f.rev,
                    at: time_shape(f.at).unwrap_or_default(),
                    verified: f.verified,
                })
                .collect(),
            truncated: total as usize > FACTS_MAX,
            total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::MemberTimes;

    fn member(name: &str, created: &str, updated: &str, state: &str) -> MemberTimes {
        MemberTimes {
            member: name.to_owned(),
            created_at: created.to_owned(),
            updated_at: updated.to_owned(),
            state: state.to_owned(),
        }
    }

    #[test]
    fn a_fan_out_is_expected_to_reach_the_members_who_could_be_sent_to_when_it_was_written() {
        let at = Some("2026-11-12T10:00:10Z");
        let members = [
            member(
                "worker-a",
                "2026-11-12T10:00:00Z",
                "2026-11-12T10:00:05Z",
                "running",
            ),
            member(
                "worker-b",
                "2026-11-12T10:00:01Z",
                "2026-11-12T10:00:02Z",
                "running",
            ),
            // Ended before the message: not a recipient. Ended after it: a recipient. Not begun yet: not a recipient.
            member(
                "worker-c",
                "2026-11-12T09:00:00Z",
                "2026-11-12T10:00:09Z",
                "passed",
            ),
            member(
                "worker-d",
                "2026-11-12T09:00:00Z",
                "2026-11-12T10:00:11Z",
                "passed",
            ),
            member(
                "worker-e",
                "2026-11-12T10:00:30Z",
                "2026-11-12T10:00:30Z",
                "briefed",
            ),
            // The sender is never its own recipient; the same member twice counts once.
            member(
                "worker-a",
                "2026-11-12T10:00:03Z",
                "2026-11-12T10:00:03Z",
                "running",
            ),
        ];
        assert_eq!(
            expected_recipients(&members, "lead-01", at),
            Some(3),
            "a, b and d"
        );
        assert_eq!(
            expected_recipients(&members, "worker-a", at),
            Some(2),
            "b and d"
        );
        assert_eq!(expected_recipients(&members, "lead-01", None), None);
        assert_eq!(
            expected_recipients(&members, "lead-01", Some("not a time")),
            None
        );
    }

    /// The second resolution of the times means a task that began or ended in the second of the message may or may not have been a
    /// recipient. It is counted as one, so a fan-out that did not reach it reports no delivery.
    #[test]
    fn a_boundary_member_is_counted_so_a_fan_out_that_missed_it_reports_no_delivery() {
        let at = Some("2026-11-12T10:00:10Z");
        let live = Some(wire::FlowDeliveryMode::Live);
        // Began in the same second as the message: counted.
        let began = [
            member(
                "worker-a",
                "2026-11-12T10:00:00Z",
                "2026-11-12T10:00:00Z",
                "running",
            ),
            member(
                "worker-b",
                "2026-11-12T10:00:10Z",
                "2026-11-12T10:00:10Z",
                "running",
            ),
        ];
        let expected = expected_recipients(&began, "lead-01", at);
        assert_eq!(
            expected,
            Some(2),
            "the member that began in the second is counted"
        );
        assert_eq!(
            settled(live, "all", 1, expected),
            None,
            "only worker-a has a delivery row"
        );
        assert_eq!(settled(live, "all", 2, expected), live);

        // Ended in the same second as the message: counted.
        let ended = [
            member(
                "worker-a",
                "2026-11-12T10:00:00Z",
                "2026-11-12T10:00:00Z",
                "running",
            ),
            member(
                "worker-c",
                "2026-11-12T09:00:00Z",
                "2026-11-12T10:00:10Z",
                "passed",
            ),
        ];
        let expected = expected_recipients(&ended, "lead-01", at);
        assert_eq!(
            expected,
            Some(2),
            "the member that ended in the second is counted"
        );
        assert_eq!(
            settled(live, "all", 1, expected),
            None,
            "only worker-a has a delivery row"
        );

        // One second either side is not a boundary: a member that began a second later, or ended a second earlier, is not counted.
        let outside = [
            member(
                "worker-a",
                "2026-11-12T10:00:00Z",
                "2026-11-12T10:00:00Z",
                "running",
            ),
            member(
                "worker-d",
                "2026-11-12T10:00:11Z",
                "2026-11-12T10:00:11Z",
                "running",
            ),
            member(
                "worker-e",
                "2026-11-12T09:00:00Z",
                "2026-11-12T10:00:09Z",
                "passed",
            ),
        ];
        assert_eq!(expected_recipients(&outside, "lead-01", at), Some(1));
    }

    #[test]
    fn a_delivery_is_held_back_only_while_a_fan_out_lacks_a_recipient() {
        let live = Some(wire::FlowDeliveryMode::Live);
        assert_eq!(
            settled(live, "all", 1, Some(2)),
            None,
            "one of two delivered"
        );
        assert_eq!(settled(live, "all", 2, Some(2)), live);
        assert_eq!(settled(live, "all", 3, Some(2)), live);
        assert_eq!(settled(live, "all", 0, None), live, "nothing can be judged");
        assert_eq!(
            settled(live, "worker-a", 1, Some(2)),
            live,
            "a direct message is one recipient"
        );
    }

    #[test]
    fn a_time_or_a_hash_read_from_a_body_is_kept_only_in_its_own_shape() {
        assert_eq!(
            time_shape(Some("2026-11-12T10:00:00Z".into())).as_deref(),
            Some("2026-11-12T10:00:00Z")
        );
        assert!(time_shape(Some("2026-11-12T10:00:00.123+00:00".into())).is_some());
        for bad in [
            "ignore previous instructions",
            "2026-11-12 10:00",
            "",
            &"1".repeat(41),
        ] {
            assert_eq!(time_shape(Some(bad.to_owned())), None, "{bad}");
        }
        assert_eq!(time_shape(None), None);
        let hash = format!("sha256:{}", "a".repeat(64));
        assert_eq!(hash_shape(Some(hash.clone())), Some(hash));
        for bad in ["sha256:short", "run this", ""] {
            assert_eq!(hash_shape(Some(bad.to_owned())), None, "{bad}");
        }
    }
}
