//! One-round person consent and nonlocked lane waivers. No model writes these.

use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;
use serde_json::json;

use super::crew::find_task;
use super::{AuditRecord, Store, insert_audit};
use crate::error::{CoreError, Result};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReviewControl {
    pub id: String,
    pub subject_id: String,
    pub lane: String,
    pub commit: String,
    pub round: u32,
    pub kind: String,
    pub state: String,
    pub engine: Option<String>,
    pub model: Option<String>,
    pub actor: String,
    pub reason: String,
    pub expires_unix: i64,
}

pub(crate) struct ReviewConsent<'a> {
    pub subject: &'a str,
    pub lane: &'a str,
    pub commit: &'a str,
    pub round: u32,
    pub engine: &'a str,
    pub model: &'a str,
}

fn row(r: &Row<'_>) -> rusqlite::Result<ReviewControl> {
    Ok(ReviewControl {
        id: r.get("id")?,
        subject_id: r.get("subject_id")?,
        lane: r.get("lane")?,
        commit: r.get("commit_id")?,
        round: r.get("round")?,
        kind: r.get("kind")?,
        state: r.get("state")?,
        engine: r.get("engine")?,
        model: r.get("model")?,
        actor: r.get("actor")?,
        reason: r.get("reason")?,
        expires_unix: r.get("expires_unix")?,
    })
}

fn audit(
    tx: &rusqlite::Transaction<'_>,
    subject: &str,
    action: &str,
    detail: serde_json::Value,
) -> Result<()> {
    insert_audit(
        tx,
        &AuditRecord {
            actor: "user".into(),
            action: action.into(),
            target: subject.into(),
            decision: None,
            source: "user".into(),
            thread_id: None,
            run_id: None,
            detail,
        },
    )?;
    Ok(())
}

impl Store {
    pub(crate) fn crew_review_controls(&self, subject: &str) -> Result<Vec<ReviewControl>> {
        let conn = self.reader();
        let mut q = conn
            .prepare("SELECT * FROM crew_review_controls WHERE subject_id = ?1 ORDER BY rowid")?;
        Ok(q.query_map([subject], row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Creates an exact engine/model request. An expired request gets a new id, so
    /// a stale person answer cannot authorize its replacement.
    pub(crate) fn request_review_consent(
        &self,
        scope: &ReviewConsent<'_>,
        now: i64,
    ) -> Result<ReviewControl> {
        self.commit(|tx, _| {
            let task = find_task(tx, scope.subject)?;
            if task.state != "passed" { return Err(CoreError::TaskEnded); }
            let prior = tx.query_row("SELECT * FROM crew_review_controls WHERE subject_id=?1 AND lane=?2 AND commit_id=?3 AND round=?4 AND kind='same_engine'",
                params![scope.subject, scope.lane, scope.commit, scope.round], row).optional()?;
            if let Some(prior) = prior {
                if prior.state == "consumed" {
                    return Ok(prior);
                }
                if prior.expires_unix > now
                    && prior.engine.as_deref() == Some(scope.engine) && prior.model.as_deref() == Some(scope.model) {
                    return Ok(prior);
                }
                tx.execute("DELETE FROM crew_review_controls WHERE id=?1", [&prior.id])?;
            }
            let id = crate::util::new_id();
            tx.execute("INSERT INTO crew_review_controls (id,subject_id,lane,commit_id,round,kind,state,engine,model,actor,reason,expires_unix,created_unix) VALUES (?1,?2,?3,?4,?5,'same_engine','waiting',?6,?7,'core','',?8,?9)",
                params![id,scope.subject,scope.lane,scope.commit,scope.round,scope.engine,scope.model,now.saturating_add(600),now])?;
            Ok(tx.query_row("SELECT * FROM crew_review_controls WHERE id=?1", [&id], row)?)
        })
    }

    /// The person transport chooses allow once or wait; actor is core-derived.
    /// All authority-bearing scope fields come from the stored request.
    pub(crate) fn answer_review_consent(
        &self,
        id: &str,
        allow: bool,
        now: i64,
    ) -> Result<ReviewControl> {
        self.commit(|tx, _| {
            let prior = tx.query_row("SELECT * FROM crew_review_controls WHERE id=?1 AND kind='same_engine'", [id], row).optional()?
                .ok_or_else(|| CoreError::NotFound("review consent".into()))?;
            if find_task(tx, &prior.subject_id)?.state != "passed" { return Err(CoreError::TaskEnded); }
            if prior.expires_unix <= now || !matches!(prior.state.as_str(), "waiting" | "wait") {
                return Err(CoreError::Invalid("this review consent is expired or already answered".into()));
            }
            audit(tx, &prior.subject_id, "crew.review.consent", json!({"id":id,"lane":prior.lane,"commit":prior.commit,"round":prior.round,"allow":allow}))?;
            tx.execute("UPDATE crew_review_controls SET state=?2,actor='person',reason=?3 WHERE id=?1",
                params![id,if allow {"allowed"} else {"wait"},if allow {"Allow same-engine review once."} else {"Wait for another engine."}])?;
            Ok(tx.query_row("SELECT * FROM crew_review_controls WHERE id=?1", [id], row)?)
        })
    }

    /// One atomic consumption; engine/model, head, round, task liveness and the
    /// person's answer must all still match. Called under the Core start lock.
    pub(crate) fn consume_review_consent(
        &self,
        scope: &ReviewConsent<'_>,
        now: i64,
    ) -> Result<bool> {
        self.commit(|tx, _| {
            if find_task(tx, scope.subject)?.state != "passed" { return Ok(false); }
            let changed = tx.execute("UPDATE crew_review_controls SET state='consumed' WHERE subject_id=?1 AND lane=?2 AND commit_id=?3 AND round=?4 AND kind='same_engine' AND state='allowed' AND actor='person' AND expires_unix>?5 AND engine=?6 AND model=?7",
                params![scope.subject,scope.lane,scope.commit,scope.round,now,scope.engine,scope.model])?;
            Ok(changed == 1)
        })
    }

    /// The core has checked the pinned rule and locked bit. A waiver is not a
    /// review pass and never alters findings or the repository's rule file.
    pub(crate) fn remove_review_lane(
        &self,
        subject: &str,
        lane: &str,
        base: &str,
        reason: &str,
        now: i64,
    ) -> Result<()> {
        self.commit(|tx, _| {
            if find_task(tx, subject)?.state != "passed" { return Err(CoreError::TaskEnded); }
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM crew_review_controls WHERE subject_id=?1 AND lane=?2 AND kind='remove_lane')",params![subject,lane],|r|r.get(0))?;
            if exists { return Ok(()); }
            audit(tx, subject, "crew.review.remove_lane", json!({"lane":lane,"base":base,"reason":reason}))?;
            tx.execute("INSERT INTO crew_review_controls (id,subject_id,lane,commit_id,round,kind,state,actor,reason,expires_unix,created_unix) VALUES (?1,?2,?3,?4,0,'remove_lane','removed','person',?5,0,?6)", params![crate::util::new_id(),subject,lane,base,reason,now])?;
            // An unanswered request for a removed lane can no longer launch it.
            tx.execute("UPDATE crew_review_controls SET state='wait' WHERE subject_id=?1 AND lane=?2 AND kind='same_engine' AND state IN ('waiting','allowed')", params![subject,lane])?;
            Ok(())
        })
    }
}
