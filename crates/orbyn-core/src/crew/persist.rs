//! Keeping a decision with its task so it can be shown later.

use orbyn_contracts::{CrewError, CrewObject, RouteDecision};

use super::guard::{key_needles, refuse_leak, refuse_secret};
use crate::app::Core;
use crate::error::{CoreError, Result};

fn refused(error: CrewError) -> CoreError {
    CoreError::Invalid(format!("route decision refused: {error}"))
}

impl Core {
    /// Stores `decision` for the task it answers. Before it goes in, no string of it may
    /// hold a secret (H2) or any text of the task's answer key (H1): a decision is built from
    /// this machine's own engines and models, and the check proves it stays that way.
    /// The task's state is not moved here; the lifecycle is slice 3's.
    pub fn store_route_decision(&self, task_id: &str, decision: &RouteDecision) -> Result<()> {
        let object = CrewObject::RouteDecision(decision.clone());
        let value = object.to_value();
        refuse_secret(&value).map_err(refused)?;
        if let Some(key) = self.store.crew_answer_key(task_id)? {
            refuse_leak(&value, &key_needles(&key)).map_err(refused)?;
        }
        self.store.add_crew_object(task_id, &object)
    }
}
