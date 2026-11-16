//! Device-local ACP tier assignments. Saves compare the revision in one transaction.

use std::collections::BTreeMap;

use orbyn_contracts::RouteTier;
use rusqlite::{Connection, params};
use serde_json::json;

use super::{AuditRecord, Store, insert_audit};
use crate::crew::AcpTierAssignment;
use crate::error::{CoreError, Result};

pub(crate) struct AcpTierSettings {
    pub revision: i64,
    pub assignments: BTreeMap<String, AcpTierAssignment>,
}

fn snapshot(conn: &Connection) -> Result<AcpTierSettings> {
    let revision = conn.query_row(
        "SELECT revision FROM crew_acp_tier_revision WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    let mut q = conn.prepare("SELECT engine,tier,model FROM crew_acp_tiers ORDER BY engine")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut assignments = BTreeMap::new();
    for (engine, tier, model) in rows {
        let tier = match tier.as_str() {
            "lead" => RouteTier::Lead,
            "worker" => RouteTier::Worker,
            "scout" => RouteTier::Scout,
            _ => return Err(CoreError::Invalid("stored ACP tier is invalid".into())),
        };
        assignments.insert(engine, AcpTierAssignment { tier, model });
    }
    Ok(AcpTierSettings {
        revision,
        assignments,
    })
}

impl Store {
    pub(crate) fn crew_acp_tiers(&self) -> Result<AcpTierSettings> {
        let mut conn = self.reader();
        let tx = conn.transaction()?;
        let result = snapshot(&tx)?;
        tx.commit()?;
        Ok(result)
    }

    pub(crate) fn save_crew_acp_tiers(
        &self,
        seen: i64,
        assignments: &BTreeMap<String, AcpTierAssignment>,
    ) -> Result<AcpTierSettings> {
        self.commit(|tx, _| {
            let current = snapshot(tx)?;
            if current.revision != seen {
                return Err(CoreError::Invalid(
                    "ACP tier settings are stale; reload before saving".into(),
                ));
            }
            if current.revision > 0 && current.assignments == *assignments {
                return Ok(current);
            }
            let next = seen
                .checked_add(1)
                .ok_or_else(|| CoreError::Invalid("ACP tier revision is exhausted".into()))?;
            insert_audit(
                tx,
                &AuditRecord {
                    actor: "user".into(),
                    action: "crew.tiers.save".into(),
                    target: "device".into(),
                    decision: None,
                    source: "user".into(),
                    thread_id: None,
                    run_id: None,
                    detail: json!({"seen":seen,"assignments":assignments}),
                },
            )?;
            tx.execute(
                "UPDATE crew_acp_tier_revision SET revision=?2 WHERE singleton=1 AND revision=?1",
                params![seen, next],
            )?;
            tx.execute("DELETE FROM crew_acp_tiers", [])?;
            for (engine, assignment) in assignments {
                tx.execute(
                    "INSERT INTO crew_acp_tiers (engine,tier,model) VALUES (?1,?2,?3)",
                    params![engine, super::wire_name(assignment.tier)?, assignment.model],
                )?;
            }
            snapshot(tx)
        })
    }
}
