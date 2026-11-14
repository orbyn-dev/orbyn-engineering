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
        let seen = u64::from(page.offset) + rows.len() as u64;
        Ok(wire::FlowMessageList {
            messages: rows
                .into_iter()
                .map(|m| wire::FlowMessage {
                    label: label_of(&m.kind).to_owned(),
                    id: m.id,
                    at: time_shape(m.at).unwrap_or_default(),
                    from: scrub.text(&m.sender, 60),
                    to: scrub.text(&m.recipient, 60),
                    kind: scrub.text(&m.kind, 20),
                    in_reply_to: hash_shape(m.in_reply_to),
                    delivery: delivery_of(m.mode.as_deref()),
                    delivered_at: time_shape(m.delivered_at),
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
