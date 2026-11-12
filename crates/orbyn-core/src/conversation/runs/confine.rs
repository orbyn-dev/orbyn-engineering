use super::*;
use crate::host::confine::{Confinement, engine_refusal, unavailable};

impl RunManager {
    /// The restriction a run's engine process starts under: set for a crew worker, which
    /// runs as the person and must not read the core's data folder. A worker that cannot be
    /// restricted does not run at all (hard rule 7).
    pub(super) fn confinement_for(
        &self,
        thread: &Thread,
        driver: &dyn EngineDriver,
        library: &Path,
        checkout: &Path,
        start: &Path,
        run_dir: &Path,
    ) -> Result<Option<Confinement>> {
        if self.store.crew_task_for_worker(&thread.id)?.is_none() {
            return Ok(None);
        }
        // A worker whose core is not bound cannot be told apart from an unrestricted run.
        let Some(core) = self.crew.get().and_then(Weak::upgrade) else {
            return Err(CoreError::EngineUnavailable(
                "A crew worker could not be restricted: the core is not ready.".into(),
            ));
        };
        if !core.config.crew_confine_workers {
            return Ok(None);
        }
        if let Some(reason) = engine_refusal(driver.descriptor().tier, &driver.family()) {
            return Err(CoreError::EngineUnavailable(reason.to_owned()));
        }
        if let Some(reason) = unavailable() {
            return Err(CoreError::EngineUnavailable(reason));
        }
        let project = Path::new(&thread.cwd);
        let confinement = Confinement::new(
            &core.config.data_dir,
            run_dir,
            &[library],
            &[start, checkout, project],
        )?;
        Ok(Some(confinement))
    }
}
