//! Immutable publication payloads, fenced admission and at-most-one send per review scope.
use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};
use crate::source_control::forge::crew_review::{CommentReceipt, ReviewTarget};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublicationState {
    Draft,
    Admitting,
    Sending,
    Published,
    Unknown,
    Stale,
    NeedsPerson,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Publication {
    pub id: String,
    pub subject: String,
    pub review: String,
    pub project: String,
    pub crew: String,
    pub lane: String,
    pub round: u32,
    pub target: ReviewTarget,
    pub origin: String,
    pub expected_author: u64,
    pub task_uri: String,
    pub body: String,
    pub body_sha256: String,
    pub scope_digest: String,
    pub replaces: Option<String>,
    pub ever_sending: bool,
    pub expires: i64,
    pub revision: i64,
    pub state: PublicationState,
    pub caller_run: Option<String>,
    pub caller_thread: Option<String>,
    pub claim: Option<String>,
    pub mode: Option<String>,
    pub decision: Option<String>,
    pub receipt: Option<CommentReceipt>,
}
impl Publication {
    pub(crate) fn admission_digest(&self) -> String {
        crate::util::sha256_hex(
            json!([
                self.immutable(),
                self.caller_run,
                self.caller_thread,
                self.claim,
                self.mode
            ])
            .to_string()
            .as_bytes(),
        )
    }
    fn immutable(&self) -> serde_json::Value {
        json!([
            self.id,
            self.subject,
            self.review,
            self.project,
            self.crew,
            self.lane,
            self.round,
            self.target,
            self.origin,
            self.expected_author,
            self.task_uri,
            self.body,
            self.body_sha256,
            self.scope_digest,
            self.replaces,
            self.expires
        ])
    }
}
fn stale() -> CoreError {
    CoreError::Invalid("review publication changed or is no longer eligible".into())
}
fn audit(tx: &rusqlite::Transaction<'_>, row: &Publication) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "core".into(),
            action: "crew.review.publication".into(),
            target: row.id.clone(),
            decision: None,
            source: "core".into(),
            thread_id: row.caller_thread.clone(),
            run_id: row.caller_run.clone(),
            detail: json!({"state":row.state,"body_sha256":row.body_sha256,"revision":row.revision,"decided_by":row.decision}),
        },
    )?;
    Ok(())
}
impl Store {
    /// Publication-only Stop fence. The generic run/event guard intentionally stays unchanged.
    pub(crate) fn publication_stop_requested(&self, run: &str) -> Result<bool> {
        Ok(self
            .reader()
            .query_row("SELECT stop_requested FROM runs WHERE id=?1", [run], |r| {
                r.get(0)
            })?)
    }
    pub(crate) fn review_publication_renewable(&self, row: &Publication) -> Result<bool> {
        let pre_send = row.state == PublicationState::Stale
            || (row.state == PublicationState::Draft && row.expires <= crate::util::unix_now());
        if !pre_send || row.ever_sending || row.receipt.is_some() {
            return Ok(false);
        }
        Ok(self.reader().query_row(
            "SELECT active=1 AND ever_sending=0 FROM crew_review_publications WHERE id=?1",
            [&row.id],
            |r| r.get(0),
        )?)
    }
    pub(crate) fn review_publication(&self, id: &str) -> Result<Option<Publication>> {
        let body: Option<String> = self
            .reader()
            .query_row(
                "SELECT body FROM crew_review_publications WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        body.map(|b| Ok(serde_json::from_str(&b)?)).transpose()
    }
    pub(crate) fn add_review_publication(&self, row: &Publication) -> Result<Publication> {
        if row.revision != 0 || row.state != PublicationState::Draft || row.claim.is_some() {
            return Err(stale());
        }
        self.commit(|tx,_| {
            let prior: Option<(String,bool)> = tx.query_row("SELECT body,ever_sending FROM crew_review_publications WHERE scope_digest=?1 AND active=1",[&row.scope_digest],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
            if let Some((body,sent)) = prior {
                let prior:Publication=serde_json::from_str(&body)?;
                if row.replaces.is_none() {return Ok(prior);}
                let renewable=prior.state==PublicationState::Stale || (prior.state==PublicationState::Draft && prior.expires<=crate::util::unix_now());
                if row.replaces.as_deref()!=Some(prior.id.as_str()) || !renewable
                    || sent || prior.ever_sending || prior.receipt.is_some() {return Err(stale());}
                // Retire only a provably pre-send stale scope. Its immutable body
                // and record remain intact; the new row needs a new gate decision.
                if tx.execute("UPDATE crew_review_publications SET active=0 WHERE id=?1 AND revision=?2 AND active=1 AND ever_sending=0",params![prior.id,prior.revision])?!=1 {return Err(stale());}
                insert_audit(tx,&AuditRecord {actor:"core".into(),action:"crew.review.publication.renew".into(),target:row.id.clone(),decision:None,source:"core".into(),thread_id:None,run_id:None,
                    detail:json!({"replaces":prior.id,"body_sha256":row.body_sha256,"scope_digest":row.scope_digest})})?;
            } else if row.replaces.is_some() {return Err(stale());}
            if row.ever_sending || row.receipt.is_some() || row.caller_run.is_some() || row.caller_thread.is_some() || row.mode.is_some() || row.decision.is_some() {return Err(stale());}
            audit(tx,row)?;
            tx.execute("INSERT INTO crew_review_publications(id,subject_id,review_task_id,scope_digest,replaces_id,active,ever_sending,revision,body) VALUES(?1,?2,?3,?4,?5,1,0,0,?6)",
                params![row.id,row.subject,row.review,row.scope_digest,row.replaces,serde_json::to_string(row)?])?;
            Ok(row.clone())
        })
    }
    /// Claim minting is internal and requires a real active run owned by this process.
    pub(crate) fn claim_review_publication(
        &self,
        row: &mut Publication,
        run: &str,
        thread: &str,
        mode: &str,
    ) -> Result<()> {
        let mut next = row.clone();
        next.state = PublicationState::Admitting;
        next.caller_run = Some(run.into());
        next.caller_thread = Some(thread.into());
        next.claim = Some(crate::util::new_id());
        next.mode = Some(mode.into());
        next.decision = None;
        self.save_publication(row, &mut next, Some((run, thread)))
    }
    pub(crate) fn update_review_publication(
        &self,
        row: &mut Publication,
        state: PublicationState,
        decision: Option<String>,
        receipt: Option<CommentReceipt>,
    ) -> Result<()> {
        let mut next = row.clone();
        next.state = state;
        next.ever_sending = row.ever_sending || state == PublicationState::Sending;
        if decision.is_some() {
            next.decision = decision;
        }
        if receipt.is_some() {
            next.receipt = receipt;
        }
        if state == PublicationState::Draft {
            next.claim = None;
            next.caller_run = None;
            next.caller_thread = None;
            next.mode = None;
        }
        self.save_publication(row, &mut next, None)
    }
    fn save_publication(
        &self,
        row: &mut Publication,
        next: &mut Publication,
        active: Option<(&str, &str)>,
    ) -> Result<()> {
        let expected = row.revision;
        next.revision = expected.checked_add(1).ok_or_else(stale)?;
        self.commit(|tx,_| {
            if let Some((run,thread))=active {
                if super::runs::guard_active(tx,&self.epoch,run)?.thread_id != thread {return Err(stale());}
                publication_not_stopped(tx,run)?;
            }
            let (body,sent):(String,bool)=tx.query_row("SELECT body,ever_sending FROM crew_review_publications WHERE id=?1 AND revision=?2 AND active=1",params![row.id,expected],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let old: Publication=serde_json::from_str(&body)?;
            if old.ever_sending!=sent || next.ever_sending!=(sent || next.state==PublicationState::Sending) {return Err(stale());}
            if active.is_none() && next.state!=PublicationState::Draft && (old.claim!=next.claim || old.caller_run!=next.caller_run || old.caller_thread!=next.caller_thread || old.mode!=next.mode) {return Err(stale());}
            if next.state==PublicationState::Sending {
                let run=next.caller_run.as_deref().ok_or_else(stale)?;
                let thread=next.caller_thread.as_deref().ok_or_else(stale)?;
                if super::runs::guard_active(tx,&self.epoch,run)?.thread_id!=thread || next.decision.is_none() {return Err(stale());}
                publication_not_stopped(tx,run)?;
            }
            let transition=matches!((old.state,next.state),
                (PublicationState::Draft,PublicationState::Admitting|PublicationState::Stale)
                |(PublicationState::Admitting,PublicationState::Admitting|PublicationState::Draft|PublicationState::Sending|PublicationState::Stale)
                |(PublicationState::Sending|PublicationState::Unknown,PublicationState::Published|PublicationState::Unknown|PublicationState::NeedsPerson)
            );
            if !transition || old.immutable()!=next.immutable() || (active.is_some() && old.state!=PublicationState::Draft) {return Err(stale());}
            audit(tx,next)?;
            if tx.execute("UPDATE crew_review_publications SET revision=?3,body=?4,ever_sending=?5 WHERE id=?1 AND revision=?2 AND active=1",params![row.id,expected,next.revision,serde_json::to_string(next)?,next.ever_sending])? != 1 {return Err(stale());}
            Ok(())
        })?;
        *row = next.clone();
        Ok(())
    }
    /// Drop/cancel releases only admission. Sending/unknown can never become retryable.
    pub(crate) fn release_review_publication(&self, id: &str, claim: &str) -> Result<()> {
        if let Some(mut row) = self.review_publication(id)?
            && row.state == PublicationState::Admitting
            && row.claim.as_deref() == Some(claim)
        {
            self.update_review_publication(&mut row, PublicationState::Draft, None, None)?;
        }
        Ok(())
    }
    /// A crash before sending needs fresh admission, never the dead run's grant.
    pub(crate) fn recover_review_publication_admission(&self, row: &mut Publication) -> Result<()> {
        if row.state != PublicationState::Admitting {
            return Ok(());
        }
        let run = row.caller_run.as_deref().ok_or_else(stale)?;
        let saved: Option<(String, String, bool)> = self
            .reader()
            .query_row(
                "SELECT owner_epoch,state,stop_requested FROM runs WHERE id=?1",
                [run],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let inactive = match saved {
            None => true,
            Some((owner, state, stopped)) => {
                stopped
                    || owner != self.epoch
                    || matches!(
                        state.as_str(),
                        "completed" | "failed" | "cancelled" | "interrupted"
                    )
            }
        };
        if inactive {
            self.update_review_publication(
                row,
                PublicationState::Draft,
                Some("fresh_admission_required_after_interruption".into()),
                None,
            )?;
        }
        Ok(())
    }
}

fn publication_not_stopped(tx: &rusqlite::Transaction<'_>, run: &str) -> Result<()> {
    if tx.query_row("SELECT stop_requested FROM runs WHERE id=?1", [run], |r| {
        r.get::<_, bool>(0)
    })? {
        return Err(stale());
    }
    Ok(())
}
