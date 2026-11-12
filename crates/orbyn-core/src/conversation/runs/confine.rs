use super::*;
use crate::host::confine::{Confinement, engine_refusal, unavailable};

/// Where one worker's turn starts and what it resumes.
pub(super) struct Launch<'a> {
    pub library: &'a Path,
    pub checkout: &'a Path,
    pub start: &'a Path,
    pub run_dir: &'a Path,
    /// The engine's own session id from an earlier turn, none on the first.
    pub resume: Option<&'a str>,
}

impl RunManager {
    /// The restriction a run's engine process starts under: set for a crew worker, which
    /// runs as the person and must not read the core's data folder. A worker that cannot be
    /// restricted does not run at all (hard rule 7). A Claude worker also loses its sight of
    /// every saved Claude conversation but its own, which `resume` names (none on a first turn).
    pub(super) fn confinement_for(
        &self,
        thread: &Thread,
        driver: &dyn EngineDriver,
        launch: &Launch<'_>,
    ) -> Result<Option<Confinement>> {
        let Launch {
            library,
            checkout,
            start,
            run_dir,
            resume,
        } = *launch;
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
        let projects = crate::agents::usage::history_root(crate::agents::usage::Engine::Claude);
        close_transcripts(confinement, &driver.family(), projects, resume).map(Some)
    }
}

/// The lead's own transcript lives in the same folder as the worker's: for Claude, its saved
/// conversations are closed except the worker's own session. Other engines keep theirs
/// elsewhere, and `docs/crew.md` lists them as not covered.
fn close_transcripts(
    confinement: Confinement,
    family: &str,
    projects: Option<std::path::PathBuf>,
    resume: Option<&str>,
) -> Result<Confinement> {
    if family != "claude" {
        return Ok(confinement);
    }
    let projects = projects.ok_or_else(|| {
        CoreError::EngineUnavailable(
            "A crew worker could not be restricted: Claude's folder is unknown.".into(),
        )
    })?;
    confinement.with_claude_transcripts(&projects, resume)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_claude_worker_gets_the_transcript_rules_and_a_missing_folder_refuses_it() {
        let dir = crate::testing::TempDir::new();
        let data = dir.path().join("data");
        let config = dir.path().join("claude");
        std::fs::create_dir_all(data.join("runs/w")).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        let rules = Confinement::new(&data, &data.join("runs/w"), &[], &[]).unwrap();
        let projects = Some(config.join("projects"));
        let claude = close_transcripts(rules.clone(), "claude", projects.clone(), None).unwrap();
        assert!(
            claude.fresh_session().is_some(),
            "a first turn names its id"
        );
        let later =
            close_transcripts(rules.clone(), "claude", projects.clone(), Some("s1")).unwrap();
        assert_eq!(later.transcripts.unwrap().session, "s1");
        // Another engine is left as it was, whatever folder is named.
        let acp = close_transcripts(rules.clone(), "gemini", projects, None).unwrap();
        assert_eq!(acp, rules);
        // No home folder, or one that does not resolve: the worker is refused.
        assert!(close_transcripts(rules.clone(), "claude", None, None).is_err());
        let missing = Some(dir.path().join("none/projects"));
        assert!(close_transcripts(rules, "claude", missing, None).is_err());
    }
}
