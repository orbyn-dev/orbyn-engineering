//! Running a workflow (docs/execution-profiles-and-library.md, Part 2). It goes through the terminal machinery that typing the command
//! in a thread's terminal uses: the same folder-trust rule, the same shell, an audit row first. It adds two things a typed command has
//! not: the saved values of the sets the workflow lists, in the child's environment only, and a mask over the run's output that hides
//! those exact values. A run with sets needs the person's acknowledgement of the `env_values` grant. Only a Person route reaches it;
//! no Crew or Autopilot path calls it.

use orbyn_contracts::{ProfileGrant, RunWorkflow, RunWorkflowAnswer};
use serde_json::json;

use super::{keychain_error, params};
use crate::app::Core;
use crate::credentials::{EnvError, EnvSecret};
use crate::error::{CoreError, Result};
use crate::host::terminal::{PtySpec, RunSecrets, TerminalLabel, shell_command};
use crate::redaction::LiteralMasker;
use crate::store::AuditRecord;
use crate::util::new_id;

/// The grant codes a run needs. The one place the list is decided: `untrusted_source` for an entry that was not written here and
/// whose command is not acknowledged, `env_values` when sets are attached. None for a local entry with no sets.
pub(crate) fn needed_grants(untrusted: bool, sets_attached: bool) -> Vec<ProfileGrant> {
    let mut grants = Vec::new();
    if untrusted {
        grants.push(ProfileGrant::UntrustedSource);
    }
    if sets_attached {
        grants.push(ProfileGrant::EnvValues);
    }
    grants
}

fn grant_codes(grants: &[ProfileGrant]) -> Vec<String> {
    grants
        .iter()
        .filter_map(|grant| serde_json::to_value(grant).ok())
        .filter_map(|code| code.as_str().map(str::to_owned))
        .collect()
}

impl Core {
    /// `POST /api/library/workflows/{id}/run` (Person).
    pub async fn run_workflow(&self, id: &str, body: &RunWorkflow) -> Result<RunWorkflowAnswer> {
        let thread = match (&body.thread_id, &body.project_id) {
            (Some(thread), None) => self.store.thread(thread)?,
            (None, Some(project)) => {
                // Terminals belong to threads: a project's most recently changed one hosts the run.
                let mut threads = self.store.list_all_threads(project)?;
                threads.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
                threads.pop().ok_or_else(|| {
                    CoreError::Invalid("project_id: open a thread in this project first.".into())
                })?
            }
            _ => {
                return Err(CoreError::Invalid(
                    "thread_id: give exactly one of thread_id and project_id.".into(),
                ));
            }
        };
        if !self.store.project(&thread.project_id)?.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        let row = self.store.library_workflow(id)?;
        let command = params::render_command(
            &row.fields.command,
            &row.fields.params,
            &body.values,
            params::Shell::native(),
        )?;

        let mut sets = Vec::new();
        for set_id in &row.fields.env_set_ids {
            if let Ok(set) = self.store.library_env_set(set_id) {
                sets.push(set);
            }
        }
        let untrusted = super::needs_ack(&row);
        let needed = needed_grants(untrusted, !sets.is_empty());
        let wrong = needed.len() != body.ack_grants.len()
            || needed.iter().any(|grant| !body.ack_grants.contains(grant));
        if wrong {
            // Names only: what the run would inject, never a value.
            return Err(CoreError::Refused {
                status: 409,
                code: "confirm_required",
                message: "Confirm what this run grants before it starts.".into(),
                detail: json!({
                    "grants": needed,
                    "env_sets": sets.iter().map(|set| json!({ "name": set.fields.name, "keys": set.fields.keys })).collect::<Vec<_>>(),
                }),
            });
        }

        let mut secrets: Vec<EnvSecret> = Vec::new();
        for set in &sets {
            match self.env_sets.spawn_values(&set.id, &set.fields.keys) {
                Ok(values) => {
                    for value in values {
                        // A later set wins over an earlier one with the same key.
                        secrets.retain(|one| one.key != value.key);
                        secrets.push(value);
                    }
                }
                // No keychain here: the set contributes no values, and the run goes on.
                Err(EnvError::Unavailable) => {}
                Err(error) => return Err(keychain_error(error)),
            }
        }
        let run_secrets = if secrets.is_empty() {
            None
        } else {
            let pairs: Vec<(&str, &str)> = secrets
                .iter()
                .map(|one| (one.key.as_str(), one.expose()))
                .collect();
            Some(RunSecrets {
                mask: LiteralMasker::new(&pairs)?,
                env: secrets,
            })
        };

        let terminal = new_id();
        if untrusted {
            // The acknowledgement stays on the entry until its command or parameters change.
            self.store
                .ack_library_workflow(&row.id, &super::command_hash(&row.fields))?;
        }
        self.store.append_audit(&AuditRecord {
            actor: "user".into(),
            action: "library.run".into(),
            target: row.id.clone(),
            decision: None,
            source: "user".into(),
            thread_id: Some(thread.id.clone()),
            run_id: None,
            detail: json!({
                "workflow": row.id,
                "name": row.fields.name,
                "terminal": terminal,
                "grants": grant_codes(&needed),
                "source": row.source,
                "env_sets": sets.iter().map(|set| set.fields.name.clone()).collect::<Vec<_>>(),
            }),
        })?;
        let (program, args) = shell_command(&command);
        self.terminals.open_named_with(
            &thread.id,
            &terminal,
            TerminalLabel {
                title: Some(row.fields.name.clone()),
                ..TerminalLabel::default()
            },
            PtySpec {
                program,
                args,
                cwd: thread.cwd.clone().into(),
                cols: 120,
                rows: 32,
                engine: false,
            },
            run_secrets,
        )?;
        Ok(RunWorkflowAnswer {
            terminal_id: terminal,
        })
    }
}
