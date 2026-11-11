//! A draft becomes a brief the way `crew.start` will stamp it (SPEC.md section 2.2): the
//! caller supplies the fields it owns, and the core adds the envelope. A draft that carries
//! any envelope field is refused, so a lead cannot forge an author or an id.

use orbyn_contracts::{
    Brief, CrewAuthor, CrewError, CrewHash, CrewId, CrewMember, CrewObject, CrewRole,
    CrewTimestamp, CrewToken, ErrorCode, reject_floats,
};
use serde_json::{Value, json};

const STAMPED: [&str; 7] = [
    "type",
    "v",
    "id",
    "crew_id",
    "author",
    "created_at",
    "run_id",
];

/// Who is calling: the lead's engine, and the crew its brief joins.
pub struct Stamper {
    pub crew_id: CrewId,
    pub engine: CrewToken,
    pub member: CrewMember,
    pub now: CrewTimestamp,
}

impl Stamper {
    /// Reads `draft` as a brief, adds the envelope and hashes it. The result passes
    /// `Brief::validate` and carries the id that `CrewObject::parse` would check.
    pub fn brief(&self, draft: &Value) -> Result<Brief, CrewError> {
        let Some(fields) = draft.as_object() else {
            return Err(CrewError::new(
                ErrorCode::InvalidObject,
                "brief: a draft is an object",
            ));
        };
        if let Some(key) = STAMPED.iter().find(|key| fields.contains_key(**key)) {
            return Err(CrewError::new(
                ErrorCode::InvalidObject,
                format!("{key}: the core stamps this field; a draft does not carry it"),
            ));
        }
        reject_floats(draft)?;
        let mut full = draft.clone();
        let map = full.as_object_mut().expect("checked to be an object");
        map.insert("v".into(), json!(0));
        map.insert("id".into(), json!(format!("sha256:{}", "0".repeat(64))));
        map.insert("crew_id".into(), json!(self.crew_id));
        map.insert(
            "author".into(),
            json!(CrewAuthor {
                role: CrewRole::Lead,
                engine: self.engine.clone(),
                member: Some(self.member.clone()),
            }),
        );
        map.insert("created_at".into(), json!(self.now));
        let mut brief: Brief = serde_path_to_error::deserialize(full).map_err(|e| {
            CrewError::new(
                ErrorCode::InvalidObject,
                format!("{}: {}", e.path(), e.inner()),
            )
        })?;
        brief.validate()?;
        let id: CrewHash = CrewObject::Brief(brief.clone()).computed_id()?;
        brief.id = id;
        Ok(brief)
    }
}
