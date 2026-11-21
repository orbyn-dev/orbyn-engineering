//! Reading a repository's committed `.orbyn/` files (docs/execution-profiles-and-library.md, "Reading committed `.orbyn/` files").
//! Read only, from the project's HEAD commit through Git objects, never from the working tree, never written. A file is read only when
//! it is a regular file of at most 64 KiB; it is parsed strictly, each entry is admitted on its own, and a bad one is dropped with a
//! problem line that names the file, the position and a stable word, never the text that was wrong. A suggestion is data: nothing here
//! runs, renders or inserts it.

use std::path::Path;

use orbyn_contracts::{
    CopiedSuggestion, CopySuggestion, EnvSetExport, LibraryKind, PromptExport, SuggestedEnvSet,
    SuggestedPrompt, SuggestedWorkflow, SuggestionProblem, Suggestions, WorkflowExport,
};
use serde_json::Value;

use super::{NAME_MAX, SUGGESTIONS_FILE_BYTES, env_set_fields, prompt_fields, workflow_fields};
use crate::app::Core;
use crate::error::{CoreError, Result};
use crate::profiles::clean;
use crate::source_control::git::git_hardened_capped;
use crate::store::{
    EnvSetFields, MAX_ENV_SETS, MAX_PROMPTS, MAX_WORKFLOWS, PromptFields, WorkflowFields,
};

pub(super) const WORKFLOWS: &str = "workflows.json";
pub(super) const PROMPTS: &str = "prompts.json";
pub(super) const ENV_SETS: &str = "env-sets.json";

pub(super) struct AdmittedWorkflow {
    pub fields: WorkflowFields,
    pub set_names: Vec<String>,
}

/// A stable word for why an entry or a file was not admitted.
pub(super) fn code_of(error: &CoreError) -> String {
    error.code().to_owned()
}

fn typed<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T> {
    serde_json::from_value(value.clone())
        .map_err(|_| CoreError::Invalid("entry: it does not match the library shape.".into()))
}

fn set_names(names: &[String]) -> Result<Vec<String>> {
    if names.len() > super::ENV_SETS_PER_WORKFLOW {
        return Err(CoreError::Invalid("env_set_names: too many sets.".into()));
    }
    names
        .iter()
        .map(|name| clean("env_set_names", name, NAME_MAX, 1, false))
        .collect()
}

pub(super) fn admit_workflow(value: &Value) -> Result<AdmittedWorkflow> {
    let entry: WorkflowExport = typed(value)?;
    Ok(AdmittedWorkflow {
        fields: workflow_fields(
            &entry.name,
            &entry.description,
            &entry.command,
            &entry.params,
            &[],
        )?,
        set_names: set_names(&entry.env_set_names)?,
    })
}

pub(super) fn admit_prompt(value: &Value) -> Result<PromptFields> {
    let entry: PromptExport = typed(value)?;
    prompt_fields(&entry.name, &entry.description, &entry.body, &entry.params)
}

/// An environment set holds key NAMES. A value that came with them (a map of name to value, or `{name, value}` objects) is dropped
/// here and reported by the second field, so it is never shown or stored.
pub(super) fn admit_env_set(value: &Value) -> Result<(EnvSetFields, bool)> {
    let mut value = value.clone();
    let mut dropped = false;
    if let Some(object) = value.as_object_mut()
        && let Some(keys) = object.get_mut("keys")
    {
        match keys {
            Value::Object(map) => {
                dropped = true;
                *keys = Value::Array(map.keys().map(|name| Value::String(name.clone())).collect());
            }
            Value::Array(items) => {
                for item in items.iter_mut() {
                    if let Value::Object(map) = item {
                        dropped = true;
                        *item = map.get("name").cloned().unwrap_or(Value::Null);
                    }
                }
            }
            _ => {}
        }
    }
    let entry: EnvSetExport = typed(&value)?;
    Ok((
        env_set_fields(&entry.name, &entry.description, &entry.keys)?,
        dropped,
    ))
}

/// The entries of a file as raw values, or the one word that says why the file gives none.
pub(super) fn file_entries(
    text: &str,
    kind: &str,
) -> std::result::Result<Vec<Value>, &'static str> {
    let value: Value = serde_json::from_str(text).map_err(|_| "not_json")?;
    match value {
        Value::Array(entries) => Ok(entries),
        Value::Object(mut object) => {
            if object.get("version").and_then(Value::as_u64) != Some(1) {
                return Err("unknown_version");
            }
            match object.remove(kind) {
                Some(Value::Array(entries)) => Ok(entries),
                _ => Err("invalid_input"),
            }
        }
        _ => Err("invalid_input"),
    }
}

enum Read {
    Absent,
    Problem(&'static str),
    Text(String),
}

/// `.orbyn/<name>` as the HEAD commit holds it. A symlink, a non-file or one over the cap is a problem and is not read.
async fn committed(root: &Path, name: &str) -> Read {
    let path = format!(".orbyn/{name}");
    let Ok(listing) =
        git_hardened_capped(root, &["ls-tree", "-l", "HEAD", "--", &path], 4096).await
    else {
        return Read::Absent;
    };
    let listing = String::from_utf8_lossy(&listing).into_owned();
    let Some(line) = listing.lines().next() else {
        return Read::Absent;
    };
    let Some((meta, _)) = line.split_once('\t') else {
        return Read::Problem("invalid_input");
    };
    let mut parts = meta.split_whitespace();
    let (mode, kind, sha, size) = (parts.next(), parts.next(), parts.next(), parts.next());
    let (Some(mode), Some(kind), Some(sha), Some(size)) = (mode, kind, sha, size) else {
        return Read::Problem("invalid_input");
    };
    if kind != "blob" || mode == "120000" {
        return Read::Problem("not_a_file");
    }
    if size
        .parse::<usize>()
        .map_or(true, |size| size > SUGGESTIONS_FILE_BYTES)
    {
        return Read::Problem("too_large");
    }
    match git_hardened_capped(root, &["cat-file", "blob", sha], SUGGESTIONS_FILE_BYTES).await {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => Read::Text(text),
            Err(_) => Read::Problem("not_json"),
        },
        Err(_) => Read::Problem("too_large"),
    }
}

impl Core {
    fn suggestion_root(&self, project_id: &str) -> Result<Option<std::path::PathBuf>> {
        let project = self.store.project(project_id)?;
        if !project.trusted {
            return Err(CoreError::FolderUntrusted);
        }
        Ok(project
            .is_git
            .then(|| std::path::PathBuf::from(project.path)))
    }

    /// `GET /api/projects/{id}/library/suggestions`.
    pub async fn library_suggestions(&self, project_id: &str) -> Result<Suggestions> {
        let mut answer = Suggestions {
            workflows: Vec::new(),
            prompts: Vec::new(),
            env_sets: Vec::new(),
            problems: Vec::new(),
        };
        let Some(root) = self.suggestion_root(project_id)? else {
            return Ok(answer);
        };
        let problem = |file: &str, index: usize, code: &str| SuggestionProblem {
            file: file.to_owned(),
            index: u32::try_from(index).unwrap_or(u32::MAX),
            code: code.to_owned(),
        };
        let at = |index: usize| u32::try_from(index).unwrap_or(u32::MAX);

        for (file, key, cap) in [
            (WORKFLOWS, "workflows", MAX_WORKFLOWS),
            (PROMPTS, "prompts", MAX_PROMPTS),
            (ENV_SETS, "env_sets", MAX_ENV_SETS),
        ] {
            let text = match committed(&root, file).await {
                Read::Absent => continue,
                Read::Problem(code) => {
                    answer.problems.push(problem(file, 0, code));
                    continue;
                }
                Read::Text(text) => text,
            };
            let entries = match file_entries(&text, key) {
                Ok(entries) => entries,
                Err(code) => {
                    answer.problems.push(problem(file, 0, code));
                    continue;
                }
            };
            for (index, value) in entries.iter().enumerate() {
                if index >= cap {
                    answer.problems.push(problem(file, index, "too_many"));
                    continue;
                }
                match file {
                    WORKFLOWS => match admit_workflow(value) {
                        Ok(one) => answer.workflows.push(SuggestedWorkflow {
                            file: file.to_owned(),
                            index: at(index),
                            name: one.fields.name,
                            description: one.fields.description,
                            command: one.fields.command,
                            params: one.fields.params,
                            env_set_names: one.set_names,
                        }),
                        Err(error) => answer.problems.push(problem(file, index, &code_of(&error))),
                    },
                    PROMPTS => match admit_prompt(value) {
                        Ok(one) => answer.prompts.push(SuggestedPrompt {
                            file: file.to_owned(),
                            index: at(index),
                            name: one.name,
                            description: one.description,
                            body: one.body,
                            params: one.params,
                        }),
                        Err(error) => answer.problems.push(problem(file, index, &code_of(&error))),
                    },
                    _ => match admit_env_set(value) {
                        Ok((one, dropped)) => {
                            if dropped {
                                answer.problems.push(problem(file, index, "value_dropped"));
                            }
                            answer.env_sets.push(SuggestedEnvSet {
                                file: file.to_owned(),
                                index: at(index),
                                name: one.name,
                                description: one.description,
                                keys: one.keys,
                            });
                        }
                        Err(error) => answer.problems.push(problem(file, index, &code_of(&error))),
                    },
                }
            }
        }
        Ok(answer)
    }

    /// `POST /api/projects/{id}/library/suggestions/copy` (Person): one entry into the device library, as an entry from a repository.
    /// Nothing is written to the repository. An environment set arrives with every key `present: false`.
    pub async fn copy_suggestion(
        &self,
        project_id: &str,
        body: &CopySuggestion,
    ) -> Result<CopiedSuggestion> {
        let Some(root) = self.suggestion_root(project_id)? else {
            return Err(CoreError::NotFound("suggestion".into()));
        };
        let (key, file) = match body.file.as_str() {
            WORKFLOWS => ("workflows", WORKFLOWS),
            PROMPTS => ("prompts", PROMPTS),
            ENV_SETS => ("env_sets", ENV_SETS),
            _ => {
                return Err(CoreError::Invalid(
                    "file: name a file under .orbyn/.".into(),
                ));
            }
        };
        let Read::Text(text) = committed(&root, file).await else {
            return Err(CoreError::NotFound("suggestion".into()));
        };
        let entries = file_entries(&text, key)
            .map_err(|code| CoreError::Invalid(format!("file: {code}.")))?;
        let value = entries
            .get(body.index as usize)
            .ok_or_else(|| CoreError::NotFound("suggestion".into()))?;
        match file {
            WORKFLOWS => {
                let one = admit_workflow(value)?;
                let mut fields = one.fields;
                fields.env_set_ids = self.set_ids_by_name(&one.set_names)?;
                let row = self.store.create_library_workflow_from(&fields, "repo")?;
                Ok(CopiedSuggestion {
                    kind: LibraryKind::Workflow,
                    entry: serde_json::to_value(super::workflow_wire(row))?,
                })
            }
            PROMPTS => {
                let row = self
                    .store
                    .create_library_prompt_from(&admit_prompt(value)?, "repo")?;
                Ok(CopiedSuggestion {
                    kind: LibraryKind::Prompt,
                    entry: serde_json::to_value(super::prompt_wire(row))?,
                })
            }
            _ => {
                let (fields, _) = admit_env_set(value)?;
                let row = self.store.create_library_env_set(&fields)?;
                Ok(CopiedSuggestion {
                    kind: LibraryKind::EnvSet,
                    entry: serde_json::to_value(self.env_set_view(row))?,
                })
            }
        }
    }

    /// Device sets by name (case-insensitive): a name with no set here is left out.
    pub(super) fn set_ids_by_name(&self, names: &[String]) -> Result<Vec<String>> {
        let sets = self.store.library_env_sets()?;
        let mut ids = Vec::new();
        for name in names {
            let key = crate::store::library_name_key(name);
            if let Some(set) = sets
                .iter()
                .find(|set| crate::store::library_name_key(&set.fields.name) == key)
                && !ids.contains(&set.id)
            {
                ids.push(set.id.clone());
            }
        }
        Ok(ids)
    }
}
