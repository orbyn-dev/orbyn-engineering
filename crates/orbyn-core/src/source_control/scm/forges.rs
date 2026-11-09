//! Which forge CLIs are installed and signed in, and publishing a project through one.

use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::source_control::forge::{self, ALL_FORGES, Forge};
use crate::source_control::git::git;
use orbyn_contracts::{ForgeStatus, PublishProject};
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) const FORGES_TTL: Duration = Duration::from_secs(60);

impl Core {
    /// Creates the remote and pushes, through the origin-less project's chosen forge.
    pub async fn publish_project(
        &self,
        project_id: &str,
        request: &PublishProject,
        actor: &str,
    ) -> Result<()> {
        let project = self.store.project(project_id)?;
        if !project.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        let cwd = Path::new(&project.path);
        if git(cwd, &["remote", "get-url", "origin"]).await.is_ok() {
            return Err(CoreError::Invalid(
                "This project already has a remote.".into(),
            ));
        }
        let name = request.name.as_deref().unwrap_or(&project.name);
        if name.starts_with('-') {
            return Err(CoreError::Invalid(
                "That is not a valid repository name.".into(),
            ));
        }
        let forge = match request.forge.as_deref() {
            None | Some("github") => Forge::GitHub,
            Some("gitlab") => Forge::GitLab,
            Some(_) => {
                return Err(CoreError::Unsupported(
                    "Only GitHub and GitLab can publish from Orbyn.".into(),
                ));
            }
        };
        self.scm_audit(actor, "git.publish", name, None)?;
        forge::publish(forge, cwd, name, request.private).await
    }

    /// Every forge and what its CLI says about itself, reused for a minute. A sign-in
    /// happens in the CLI, outside this service, so `rescan` asks the CLIs again; that
    /// also finds one installed a moment ago.
    pub async fn forges(&self, rescan: bool) -> Vec<ForgeStatus> {
        if !rescan && let Some(known) = self.known_forges() {
            return known;
        }
        let reads: Vec<_> = ALL_FORGES.into_iter().map(forge::forge_status).collect();
        let statuses = futures_util::future::join_all(reads).await;
        *self
            .forge_statuses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((Instant::now(), statuses.clone()));
        statuses
    }

    pub(super) fn known_forges(&self) -> Option<Vec<ForgeStatus>> {
        let known = self
            .forge_statuses
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (at, statuses) = known.as_ref()?;
        (at.elapsed() < FORGES_TTL).then(|| statuses.clone())
    }
}
