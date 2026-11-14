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
                    at: m.at.unwrap_or_default(),
                    from: scrub.text(&m.sender, 60),
                    to: scrub.text(&m.recipient, 60),
                    kind: scrub.text(&m.kind, 20),
                    in_reply_to: m.in_reply_to,
                    delivery: delivery_of(m.mode.as_deref()),
                    delivered_at: m.delivered_at,
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
                    at: f.at.unwrap_or_default(),
                    verified: f.verified,
                })
                .collect(),
            truncated: total as usize > FACTS_MAX,
            total,
        })
    }
}
