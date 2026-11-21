//! Import and export (docs/execution-profiles-and-library.md, "Import and export"). Export writes no file and takes no path: the
//! client saves the answer. Environment values are never exported, only a set's name and its key names. Import reads a document the
//! client gives, which is untrusted: at most 2 MiB, the library's own field limits, unknown fields dropped, each bad entry refused on
//! its own. It creates entries (renaming one whose name is taken to `Name (2)`) and never replaces or deletes one that exists.

use std::collections::{HashMap, HashSet};

use orbyn_contracts::{
    EnvSetExport, ImportAnswer, ImportLine, ImportOutcome, ImportProblem, LibraryDocument,
    ProfileDraft, PromptExport, WorkflowExport,
};
use serde_json::Value;

use super::suggest::{admit_env_set, admit_prompt, admit_workflow, code_of};
use super::{IMPORT_BYTES, NAME_MAX};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::store::{
    MAX_ENV_SETS, MAX_PROMPTS, MAX_WORKFLOWS, PROFILE_LIMIT, library_name_key as key,
};

const KINDS: [&str; 4] = ["profiles", "workflows", "prompts", "env_sets"];

/// `name`, or `name (2)`, `name (3)` ... the first not in `taken`, cut to fit the name limit.
fn free_name(name: &str, taken: &HashSet<String>) -> String {
    if !taken.contains(&key(name)) {
        return name.to_owned();
    }
    for n in 2..1000 {
        let suffix = format!(" ({n})");
        let room = NAME_MAX - suffix.chars().count();
        let base: String = name.chars().take(room).collect();
        let candidate = format!("{}{suffix}", base.trim_end());
        if !taken.contains(&key(&candidate)) {
            return candidate;
        }
    }
    name.to_owned()
}

/// What one kind has now, so a dry run and a real run choose the same names.
struct Bucket {
    names: HashSet<String>,
    count: usize,
    max: usize,
}

impl Bucket {
    fn take(&mut self, name: &str) -> std::result::Result<String, &'static str> {
        if self.count >= self.max {
            return Err("library_limit");
        }
        let free = free_name(name, &self.names);
        self.names.insert(key(&free));
        self.count += 1;
        Ok(free)
    }
}

fn line(
    kind: &str,
    index: usize,
    outcome: ImportOutcome,
    name: Option<String>,
    code: Option<String>,
) -> ImportLine {
    ImportLine {
        kind: kind.to_owned(),
        index: u32::try_from(index).unwrap_or(u32::MAX),
        outcome,
        name,
        code,
    }
}

fn tally(answer: &mut ImportAnswer, entry: ImportLine) {
    match entry.outcome {
        ImportOutcome::Created => answer.created += 1,
        ImportOutcome::Renamed => answer.renamed += 1,
        ImportOutcome::Refused => answer.refused += 1,
    }
    answer.lines.push(entry);
}

impl Core {
    /// `GET /api/library/export?kinds=`. Names only for sets; no value, no id.
    pub async fn export_library(&self, kinds: Option<&str>) -> Result<LibraryDocument> {
        let wanted: Vec<&str> = match kinds {
            None | Some("") => KINDS.to_vec(),
            Some(list) => {
                let wanted: Vec<&str> = list.split(',').map(str::trim).collect();
                if wanted.iter().any(|kind| !KINDS.contains(kind)) {
                    return Err(CoreError::Invalid(
                        "kinds: use profiles, workflows, prompts or env_sets.".into(),
                    ));
                }
                wanted
            }
        };
        let sets = self.store.library_env_sets()?;
        let mut document = LibraryDocument {
            format: "orbyn-library".into(),
            version: 1,
            exported_at: crate::util::now(),
            profiles: Vec::new(),
            workflows: Vec::new(),
            prompts: Vec::new(),
            env_sets: Vec::new(),
        };
        if wanted.contains(&"profiles") {
            document.profiles = self
                .list_profiles()
                .await?
                .into_iter()
                .filter(|profile| !profile.builtin)
                .map(|profile| ProfileDraft {
                    name: profile.name,
                    description: profile.description,
                    engine: profile.engine,
                    model_tier: profile.model_tier,
                    permission_mode: profile.permission_mode,
                    autopilot_mode: profile.autopilot_mode,
                })
                .collect();
        }
        if wanted.contains(&"workflows") {
            document.workflows = self
                .store
                .library_workflows()?
                .into_iter()
                .map(|row| WorkflowExport {
                    name: row.fields.name,
                    description: row.fields.description,
                    command: row.fields.command,
                    params: row.fields.params,
                    env_set_names: row
                        .fields
                        .env_set_ids
                        .iter()
                        .filter_map(|id| sets.iter().find(|set| &set.id == id))
                        .map(|set| set.fields.name.clone())
                        .collect(),
                })
                .collect();
        }
        if wanted.contains(&"prompts") {
            document.prompts = self
                .store
                .library_prompts()?
                .into_iter()
                .map(|row| PromptExport {
                    name: row.fields.name,
                    description: row.fields.description,
                    body: row.fields.body,
                    params: row.fields.params,
                })
                .collect();
        }
        if wanted.contains(&"env_sets") {
            document.env_sets = sets
                .into_iter()
                .map(|set| EnvSetExport {
                    name: set.fields.name,
                    description: set.fields.description,
                    keys: set.fields.keys,
                })
                .collect();
        }
        Ok(document)
    }

    /// `POST /api/library/import` (Person). `bytes` is the document as the client read it from a file the person picked.
    pub async fn import_library(&self, bytes: &[u8]) -> Result<ImportAnswer> {
        if bytes.len() > IMPORT_BYTES {
            return Err(CoreError::Invalid("document: use at most 2 MiB.".into()));
        }
        let document: Value = serde_json::from_slice(bytes)
            .map_err(|_| CoreError::Invalid("document: it is not valid JSON.".into()))?;
        if document.get("format").and_then(Value::as_str) != Some("orbyn-library")
            || document.get("version").and_then(Value::as_u64) != Some(1)
        {
            return Err(CoreError::Invalid(
                "document: this is not an Orbyn library export this version reads.".into(),
            ));
        }
        let dry = document
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !dry {
            self.store.library_audit(
                "library.import",
                "import",
                serde_json::json!({ "bytes": bytes.len() }),
            )?;
        }
        let entries = |kind: &str| -> Vec<Value> {
            document
                .get(kind)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        let (workflows, prompts, env_sets) = self.store.library_counts()?;
        let sets_now = self.store.library_env_sets()?;
        let mut set_ids: HashMap<String, String> = sets_now
            .iter()
            .map(|set| (key(&set.fields.name), set.id.clone()))
            .collect();
        let bucket = |names: HashSet<String>, count, max| Bucket { names, count, max };
        let mut sets_bucket = bucket(
            sets_now.iter().map(|set| key(&set.fields.name)).collect(),
            env_sets,
            MAX_ENV_SETS,
        );
        let mut workflows_bucket = bucket(
            self.store
                .library_workflows()?
                .iter()
                .map(|row| key(&row.fields.name))
                .collect(),
            workflows,
            MAX_WORKFLOWS,
        );
        let mut prompts_bucket = bucket(
            self.store
                .library_prompts()?
                .iter()
                .map(|row| key(&row.fields.name))
                .collect(),
            prompts,
            MAX_PROMPTS,
        );
        let saved_profiles: Vec<String> = self
            .list_profiles()
            .await?
            .into_iter()
            .map(|profile| key(&profile.name))
            .collect();
        let mut profile_names: HashSet<String> = saved_profiles.iter().cloned().collect();
        let mut profile_count = saved_profiles.len().saturating_sub(3);

        let mut answer = ImportAnswer {
            dry_run: dry,
            problems: Vec::new(),
            created: 0,
            renamed: 0,
            refused: 0,
            lines: Vec::new(),
        };
        let outcome = |asked: &str, given: &str| {
            if asked.trim() == given {
                ImportOutcome::Created
            } else {
                ImportOutcome::Renamed
            }
        };

        // Sets first, so a workflow in the same document can name one.
        for (index, value) in entries("env_sets").iter().enumerate() {
            let admitted = match admit_env_set(value) {
                Ok((fields, dropped)) => {
                    if dropped {
                        answer.problems.push(ImportProblem {
                            kind: "env_set".into(),
                            index: index as u32,
                            code: "value_dropped".into(),
                        });
                    }
                    fields
                }
                Err(error) => {
                    tally(
                        &mut answer,
                        line(
                            "env_set",
                            index,
                            ImportOutcome::Refused,
                            None,
                            Some(code_of(&error)),
                        ),
                    );
                    continue;
                }
            };
            let name = match sets_bucket.take(&admitted.name) {
                Ok(name) => name,
                Err(code) => {
                    tally(
                        &mut answer,
                        line(
                            "env_set",
                            index,
                            ImportOutcome::Refused,
                            None,
                            Some(code.into()),
                        ),
                    );
                    continue;
                }
            };
            let mut fields = admitted.clone();
            fields.name = name.clone();
            let id = if dry {
                String::new()
            } else {
                match self.store.create_library_env_set(&fields) {
                    Ok(row) => row.id,
                    Err(error) => {
                        tally(
                            &mut answer,
                            line(
                                "env_set",
                                index,
                                ImportOutcome::Refused,
                                None,
                                Some(code_of(&error)),
                            ),
                        );
                        continue;
                    }
                }
            };
            set_ids.insert(key(&name), id);
            tally(
                &mut answer,
                line(
                    "env_set",
                    index,
                    outcome(&admitted.name, &name),
                    Some(name),
                    None,
                ),
            );
        }

        for (index, value) in entries("workflows").iter().enumerate() {
            let admitted = match admit_workflow(value) {
                Ok(one) => one,
                Err(error) => {
                    tally(
                        &mut answer,
                        line(
                            "workflow",
                            index,
                            ImportOutcome::Refused,
                            None,
                            Some(code_of(&error)),
                        ),
                    );
                    continue;
                }
            };
            let mut ids = Vec::new();
            for set in &admitted.set_names {
                match set_ids.get(&key(set)) {
                    Some(id) if !ids.contains(id) => ids.push(id.clone()),
                    Some(_) => {}
                    None => answer.problems.push(ImportProblem {
                        kind: "workflow".into(),
                        index: index as u32,
                        code: "unknown_env_set".into(),
                    }),
                }
            }
            let name = match workflows_bucket.take(&admitted.fields.name) {
                Ok(name) => name,
                Err(code) => {
                    tally(
                        &mut answer,
                        line(
                            "workflow",
                            index,
                            ImportOutcome::Refused,
                            None,
                            Some(code.into()),
                        ),
                    );
                    continue;
                }
            };
            let mut fields = admitted.fields.clone();
            fields.name = name.clone();
            fields.env_set_ids = if dry { Vec::new() } else { ids };
            if !dry && let Err(error) = self.store.create_library_workflow_from(&fields, "import") {
                tally(
                    &mut answer,
                    line(
                        "workflow",
                        index,
                        ImportOutcome::Refused,
                        None,
                        Some(code_of(&error)),
                    ),
                );
                continue;
            }
            tally(
                &mut answer,
                line(
                    "workflow",
                    index,
                    outcome(&admitted.fields.name, &name),
                    Some(name),
                    None,
                ),
            );
        }

        for (index, value) in entries("prompts").iter().enumerate() {
            let admitted = match admit_prompt(value) {
                Ok(one) => one,
                Err(error) => {
                    tally(
                        &mut answer,
                        line(
                            "prompt",
                            index,
                            ImportOutcome::Refused,
                            None,
                            Some(code_of(&error)),
                        ),
                    );
                    continue;
                }
            };
            let name = match prompts_bucket.take(&admitted.name) {
                Ok(name) => name,
                Err(code) => {
                    tally(
                        &mut answer,
                        line(
                            "prompt",
                            index,
                            ImportOutcome::Refused,
                            None,
                            Some(code.into()),
                        ),
                    );
                    continue;
                }
            };
            let mut fields = admitted.clone();
            fields.name = name.clone();
            if !dry && let Err(error) = self.store.create_library_prompt_from(&fields, "import") {
                tally(
                    &mut answer,
                    line(
                        "prompt",
                        index,
                        ImportOutcome::Refused,
                        None,
                        Some(code_of(&error)),
                    ),
                );
                continue;
            }
            tally(
                &mut answer,
                line(
                    "prompt",
                    index,
                    outcome(&admitted.name, &name),
                    Some(name),
                    None,
                ),
            );
        }

        for (index, value) in entries("profiles").iter().enumerate() {
            let Ok(mut draft) = serde_json::from_value::<ProfileDraft>(value.clone()) else {
                tally(
                    &mut answer,
                    line(
                        "profile",
                        index,
                        ImportOutcome::Refused,
                        None,
                        Some("invalid_input".into()),
                    ),
                );
                continue;
            };
            let asked = draft.name.clone();
            if profile_count >= PROFILE_LIMIT {
                tally(
                    &mut answer,
                    line(
                        "profile",
                        index,
                        ImportOutcome::Refused,
                        None,
                        Some("profile_limit".into()),
                    ),
                );
                continue;
            }
            draft.name = free_name(draft.name.trim(), &profile_names);
            let checked = if dry {
                self.check_profile(&draft).await.map(|_| ())
            } else {
                self.create_profile(&draft).await.map(|_| ())
            };
            match checked {
                Ok(()) => {
                    profile_names.insert(key(&draft.name));
                    profile_count += 1;
                    let name = draft.name.clone();
                    tally(
                        &mut answer,
                        line("profile", index, outcome(&asked, &name), Some(name), None),
                    );
                }
                Err(error) => {
                    tally(
                        &mut answer,
                        line(
                            "profile",
                            index,
                            ImportOutcome::Refused,
                            None,
                            Some(code_of(&error)),
                        ),
                    );
                }
            }
        }

        Ok(answer)
    }
}
