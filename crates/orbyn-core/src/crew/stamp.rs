//! A draft becomes an object the way `crew.start` and `crew.report` stamp it (SPEC.md section
//! 2.2): the caller supplies the fields it owns, and the core adds the envelope. A draft that
//! carries any envelope field is refused, so a member cannot forge an author, an id or a run.

use orbyn_contracts::{
    AnswerKey, Brief, CrewAuthor, CrewError, CrewHash, CrewId, CrewMember, CrewMessage, CrewObject,
    CrewRole, CrewTimestamp, CrewToken, ErrorCode, Fact, Report, reject_floats,
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

/// Who is calling: the caller's engine, its member id and the crew its object joins.
pub struct Stamper {
    pub crew_id: CrewId,
    pub engine: CrewToken,
    pub member: CrewMember,
    pub now: CrewTimestamp,
}

fn invalid(message: String) -> CrewError {
    CrewError::new(ErrorCode::InvalidObject, message)
}

impl Stamper {
    /// The draft with the envelope added and `own` (fields the core fills in) set. A draft
    /// that names an envelope field, or one of `own`, is refused.
    fn envelope(
        &self,
        what: &str,
        draft: &Value,
        role: CrewRole,
        own: &[(&str, Value)],
    ) -> Result<Value, CrewError> {
        let Some(fields) = draft.as_object() else {
            return Err(invalid(format!("{what}: a draft is an object")));
        };
        let named = STAMPED
            .iter()
            .copied()
            .chain(own.iter().map(|(key, _)| *key))
            .find(|key| fields.contains_key(*key));
        if let Some(key) = named {
            return Err(invalid(format!(
                "{key}: the core stamps this field; a draft does not carry it"
            )));
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
                role,
                engine: self.engine.clone(),
                member: Some(self.member.clone()),
            }),
        );
        map.insert("created_at".into(), json!(self.now));
        for (key, value) in own {
            map.insert((*key).to_owned(), value.clone());
        }
        Ok(full)
    }

    fn read<T: serde::de::DeserializeOwned>(full: Value) -> Result<T, CrewError> {
        serde_path_to_error::deserialize(full)
            .map_err(|e| invalid(format!("{}: {}", e.path(), e.inner())))
    }

    /// Reads `draft` as a brief, adds the envelope and hashes it. The result passes
    /// `Brief::validate` and carries the id that `CrewObject::parse` would check.
    pub fn brief(&self, draft: &Value) -> Result<Brief, CrewError> {
        let full = self.envelope("brief", draft, CrewRole::Lead, &[])?;
        let mut brief: Brief = Self::read(full)?;
        brief.validate()?;
        brief.id = CrewObject::Brief(brief.clone()).computed_id()?;
        Ok(brief)
    }

    /// The lead's answer key for the brief `brief`. The core names the brief, so a draft that
    /// does is refused.
    pub fn answer_key(&self, draft: &Value, brief: &CrewHash) -> Result<AnswerKey, CrewError> {
        let own = [("brief", json!(brief))];
        let full = self.envelope("answer_key", draft, CrewRole::Lead, &own)?;
        let mut key: AnswerKey = Self::read(full)?;
        key.validate()?;
        key.id = CrewObject::AnswerKey(key.clone()).computed_id()?;
        Ok(key)
    }

    /// A worker's report on `brief` in `run`. An absent `claims` or `not_verified` is empty.
    pub fn report(
        &self,
        draft: &Value,
        run: &CrewId,
        brief: &CrewHash,
    ) -> Result<Report, CrewError> {
        let own = [("run_id", json!(run)), ("brief", json!(brief))];
        let mut full = self.envelope("report", draft, CrewRole::Worker, &own)?;
        let map = full.as_object_mut().expect("an object");
        for key in ["claims", "not_verified"] {
            map.entry(key).or_insert_with(|| json!([]));
        }
        let mut report: Report = Self::read(full)?;
        report.validate()?;
        report.id = CrewObject::Report(report.clone()).computed_id()?;
        Ok(report)
    }

    /// A member's message in `run`. The sender's role and member id come from the caller, so a
    /// draft that names an author, an id or a run is refused.
    pub fn message(
        &self,
        draft: &Value,
        run: &CrewId,
        role: CrewRole,
    ) -> Result<CrewMessage, CrewError> {
        let own = [("run_id", json!(run))];
        let full = self.envelope("message", draft, role, &own)?;
        let mut message: CrewMessage = Self::read(full)?;
        message.validate()?;
        message.id = CrewObject::Message(message.clone()).computed_id()?;
        Ok(message)
    }

    /// A member's board write in `run`. The core sets `rev` to one past the revision the
    /// writer read (`base_rev`), so a draft that names `rev` is refused. An absent `evidence`
    /// is empty.
    pub fn fact(&self, draft: &Value, run: &CrewId, role: CrewRole) -> Result<Fact, CrewError> {
        let base = draft
            .get("base_rev")
            .and_then(Value::as_u64)
            .ok_or_else(|| invalid("base_rev: the revision you read, or 0 for a new key".into()))?;
        let own = [
            ("run_id", json!(run)),
            ("rev", json!(base.saturating_add(1))),
        ];
        let mut full = self.envelope("fact", draft, role, &own)?;
        full.as_object_mut()
            .expect("an object")
            .entry("evidence")
            .or_insert_with(|| json!([]));
        let mut fact: Fact = Self::read(full)?;
        fact.validate()?;
        fact.id = CrewObject::Fact(fact.clone()).computed_id()?;
        Ok(fact)
    }
}
