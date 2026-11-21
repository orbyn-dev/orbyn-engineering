//! A repository lookup over the checkouts the app already holds: which registered
//! projects' own Git configs name one repository, matched by its `owner/name`.
//! Read without a transport: no forge is asked and no remote URL leaves the read.

use orbyn_contracts::{ProjectScopeCheckout, RepositoryLookup};

use crate::app::Core;
use crate::error::{CoreError, Result};

/// The longest `owner/name` path accepted. A nested GitLab group path fits too.
const MAX_PATH_BYTES: usize = 512;
const MAX_SEGMENTS: usize = 16;
const MAX_SEGMENT_BYTES: usize = 128;

/// A repository path as a remote names it: `owner/name`, or a nested group path, of
/// plain Git-host characters. Case is kept; matching ignores ASCII case.
fn repository_path(raw: &str) -> Result<&str> {
    let refused = || {
        CoreError::Invalid(
            "A repository name is owner/name, for example orbyn-dev/orbyn-engineering.".into(),
        )
    };
    if raw.is_empty() || raw.len() > MAX_PATH_BYTES {
        return Err(refused());
    }
    let segments: Vec<&str> = raw.split('/').collect();
    if segments.len() < 2 || segments.len() > MAX_SEGMENTS {
        return Err(refused());
    }
    for segment in segments {
        if segment.is_empty()
            || segment.len() > MAX_SEGMENT_BYTES
            || matches!(segment, "." | "..")
            || !segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err(refused());
        }
    }
    Ok(raw)
}

impl Core {
    /// The app's own record of a repository: the registered checkouts whose own Git
    /// config names it. Nothing is asked of a forge, so a repository the person has not
    /// added is simply not found rather than read from the forge.
    pub async fn repository_lookup(&self, repository: &str) -> Result<RepositoryLookup> {
        let wanted = repository_path(repository)?;
        let records = self.repository_checkouts().await?;
        let complete = records
            .iter()
            .all(|(_, checkout)| checkout.metadata_complete);
        let mut checkouts: Vec<ProjectScopeCheckout> = records
            .into_iter()
            .filter(|(_, checkout)| {
                checkout
                    .repository_label
                    .as_deref()
                    .is_some_and(|label| label.eq_ignore_ascii_case(wanted))
            })
            .map(|(_, checkout)| checkout)
            .collect();
        checkouts.sort_by(|left, right| left.project_id.cmp(&right.project_id));
        Ok(RepositoryLookup {
            complete,
            checkouts,
        })
    }
}
