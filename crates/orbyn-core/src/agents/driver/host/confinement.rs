//! The `confinement` object of an `open`: what the engine process may be started under.
//! The core decides it, the host only applies it. If the restriction cannot be built here,
//! the run is refused; if the host cannot apply it, `open` fails `unsupported` and the run
//! is refused too. An unconfined fallback does not exist.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::agents::driver::RunSpec;
use crate::agents::driver::worker_identity::worker_env;
use crate::error::{CoreError, Result};
use crate::host::confine::Confinement;
use crate::host::process::ResolvedCommand;

/// A variable whose name says it holds a credential. A frame carries no credential, so these
/// never go into the allowlist: a key or token the engine needs is the host's spawn
/// environment, not a request field.
fn names_a_secret(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "PASSPHRASE",
        "CREDENTIAL",
        "API_KEY",
        "ACCESS_KEY",
        "PRIVATE_KEY",
    ]
    .iter()
    .any(|part| upper.contains(part))
        || upper.ends_with("_KEY")
        || crate::host::child_env::is_account_name(&upper)
}

/// The complete environment an engine under confinement gets: what a CLI worker is given
/// today (`host::child_env`), without the service's data folder or any credential-named
/// variable, plus the worker's identity.
pub fn allowlist(
    inherited: &[(OsString, OsString)],
    prefer_saved: bool,
    identity: &[(String, String)],
    worker_id: &str,
) -> BTreeMap<String, String> {
    let plan = crate::host::child_env::plan(inherited, true, prefer_saved);
    let mut env = BTreeMap::new();
    for (name, value) in inherited {
        if plan.remove.contains(name) {
            continue;
        }
        if let (Some(name), Some(value)) = (name.to_str(), value.to_str()) {
            env.insert(name.to_owned(), value.to_owned());
        }
    }
    for (name, value) in &plan.set {
        if let (Some(name), Some(value)) = (name.to_str(), value.to_str()) {
            env.insert(name.to_owned(), value.to_owned());
        }
    }
    let base = env.get("PATH").map(OsString::from);
    let path = crate::host::child_env::widened(base.as_deref());
    if let Some(path) = path.to_str() {
        env.insert("PATH".into(), path.to_owned());
    }
    env.remove("ORBYN_DATA_DIR");
    env.retain(|name, _| !names_a_secret(name));
    env.extend(identity.iter().cloned());
    env.insert("ORBYN_WORKER_ID".into(), worker_id.to_owned());
    env
}

/// The argv that puts the restriction in front of a command: the tool and its arguments up to
/// where the program goes. The restriction wraps a placeholder and the placeholder is cut off,
/// so the platform's own rules (and its refusal) stay the one source.
pub fn argv_prefix(confine: &Confinement) -> Result<Vec<String>> {
    let marker = PathBuf::from("orbyn-confinement-placeholder");
    let mut command = ResolvedCommand {
        program: marker.clone(),
        prefix: Vec::new(),
        path: marker.clone(),
    };
    confine.wrap(&mut command)?;
    let mut argv: Vec<OsString> = vec![command.program.into_os_string()];
    argv.extend(command.prefix);
    if argv.last().map(OsString::as_os_str) != Some(OsStr::new("orbyn-confinement-placeholder")) {
        return Err(CoreError::EngineUnavailable(
            "A crew worker could not be restricted: the restriction did not take the usual shape."
                .into(),
        ));
    }
    argv.pop();
    argv.into_iter()
        .map(|part| {
            part.into_string().map_err(|_| {
                CoreError::EngineUnavailable(
                    "A crew worker could not be restricted: a path is not text.".into(),
                )
            })
        })
        .collect()
}

/// The `confinement` of a run, or `None` when the run is not restricted.
pub fn build(spec: &RunSpec, family: &str) -> Result<Option<Value>> {
    let Some(confine) = &spec.confine else {
        return Ok(None);
    };
    let identity = spec.crew_task.as_deref().map_or_else(Vec::new, |task| {
        worker_env(task, family, spec.model.as_deref())
    });
    let inherited: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let env = allowlist(
        &inherited,
        crate::host::child_env::prefers_saved_login(),
        &identity,
        &spec.thread_id,
    );
    let mut roots: Vec<String> = Vec::new();
    for root in confine.allow_write.iter().chain([&spec.checkout]) {
        let root = root.to_string_lossy().into_owned();
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    Ok(Some(json!({
        "argvPrefix": argv_prefix(confine)?,
        "env": env,
        "cwd": spec.cwd,
        "writeRoots": roots,
    })))
}

#[cfg(test)]
#[path = "confinement_tests.rs"]
mod tests;
