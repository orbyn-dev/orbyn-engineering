//! macOS: `sandbox-exec` with a fixed profile. The profile text never holds a path; every
//! path travels as a `-D NAME=value` parameter, so a folder name cannot change a rule.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::{Confinement, ResolvedCommand};
use crate::error::{CoreError, Result};

const TOOL: &str = "/usr/bin/sandbox-exec";

/// Later rules win, so the allowances after the data folder's denial open only their own
/// folders. The task port and Apple events stop a worker driving another process; `lsopen`
/// stops it asking LaunchServices to start a program it wrote, which would run from launchd
/// outside this profile; `signal` leaves a worker able to signal itself and its own children
/// only. `process-info*` is kept, but it does not stop a worker reading other processes'
/// command lines or environment (`docs/architecture.md` lists this as a limit).
const HEAD: &str = "(version 1)\n(allow default)\n\
    (deny file-read* file-write* (subpath (param \"DATA\")))\n\
    (allow file-read* file-write* (subpath (param \"RUN\")))\n";
const TAIL: &str = "(deny mach-priv-task-port)\n\
    (deny process-info* (target others))\n\
    (deny appleevent-send)\n\
    (deny lsopen)\n\
    (deny signal)\n\
    (allow signal (target self) (target children) (target same-sandbox))\n";

/// The profile for `confinement`: the head, one rule per allowance named by parameter
/// (`R0`, `W0`, `X0`...), and the tail.
pub(super) fn profile(confinement: &Confinement) -> String {
    let mut text = String::from(HEAD);
    let rules = [
        ("R", confinement.allow_read.len(), "file-read*"),
        ("W", confinement.allow_write.len(), "file-read* file-write*"),
        ("X", confinement.allow_exec.len(), "file-read* process-exec"),
    ];
    for (prefix, count, operations) in rules {
        for index in 0..count {
            text.push_str(&format!(
                "(allow {operations} (subpath (param \"{prefix}{index}\")))\n"
            ));
        }
    }
    // The folders on the way down to an allowance may be looked at (not listed or read), so
    // a path through them can be walked, such as `cd` into the checkout.
    for index in 0..route(confinement).len() {
        text.push_str(&format!(
            "(allow file-read-metadata (literal (param \"M{index}\")))\n"
        ));
    }
    // Moving the data folder, or any folder above it, would put its files under a name the
    // path rules do not cover, so none of them may be renamed or removed.
    for index in 0..above(confinement).len() {
        text.push_str(&format!(
            "(deny file-write-unlink (literal (param \"A{index}\")))\n"
        ));
    }
    text.push_str(TAIL);
    text
}

/// The data folder and every folder above it, up to and including `/`.
fn above(confinement: &Confinement) -> Vec<PathBuf> {
    confinement
        .data
        .ancestors()
        .map(Path::to_path_buf)
        .collect()
}

/// The data folder and the folders inside it that lead to an allowance.
fn route(confinement: &Confinement) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let allowed = std::iter::once(&confinement.run_dir)
        .chain(&confinement.allow_read)
        .chain(&confinement.allow_write)
        .chain(&confinement.allow_exec);
    for path in allowed {
        for folder in path.ancestors().skip(1) {
            if folder.starts_with(&confinement.data) && !found.iter().any(|f| f == folder) {
                found.push(folder.to_path_buf());
            }
        }
    }
    found
}

fn parameters(confinement: &Confinement) -> Vec<(String, PathBuf)> {
    let mut all = vec![
        ("DATA".to_owned(), confinement.data.clone()),
        ("RUN".to_owned(), confinement.run_dir.clone()),
    ];
    for (prefix, paths) in [
        ("R", &confinement.allow_read),
        ("W", &confinement.allow_write),
        ("X", &confinement.allow_exec),
    ] {
        all.extend(
            paths
                .iter()
                .enumerate()
                .map(|(index, path)| (format!("{prefix}{index}"), path.clone())),
        );
    }
    for (prefix, paths) in [("M", route(confinement)), ("A", above(confinement))] {
        all.extend(
            paths
                .into_iter()
                .enumerate()
                .map(|(index, path)| (format!("{prefix}{index}"), path)),
        );
    }
    all
}

pub fn wrap(confinement: &Confinement, command: &mut ResolvedCommand) -> Result<()> {
    if let Some(reason) = unavailable() {
        return Err(CoreError::EngineUnavailable(reason));
    }
    let mut prefix: Vec<OsString> = vec!["-p".into(), profile(confinement).into()];
    for (name, path) in parameters(confinement) {
        let mut pair = OsString::from(format!("{name}="));
        pair.push(path);
        prefix.push("-D".into());
        prefix.push(pair);
    }
    prefix.push(std::mem::take(&mut command.program).into_os_string());
    prefix.append(&mut command.prefix);
    command.program = TOOL.into();
    command.prefix = prefix;
    Ok(())
}

pub fn unavailable() -> Option<String> {
    static CHECKED: OnceLock<Option<String>> = OnceLock::new();
    CHECKED.get_or_init(|| check(Path::new(TOOL))).clone()
}

/// Runs a harmless program under the tool. A missing tool, or a refusal to start (exit 71,
/// which is what a process already under a profile gets), means no worker can be restricted.
pub(super) fn check(tool: &Path) -> Option<String> {
    if !tool.is_file() {
        return Some(
            "Crew workers could not start: this Mac's process restriction tool is missing."
                .to_owned(),
        );
    }
    let run = std::process::Command::new(tool)
        .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
        .output();
    match run {
        Ok(output) if output.status.success() => None,
        _ => Some(
            "Crew workers could not start: Orbyn itself runs under a macOS restriction, \
             so it cannot restrict a worker."
                .to_owned(),
        ),
    }
}

#[cfg(test)]
pub(super) fn above_len(confinement: &Confinement) -> usize {
    above(confinement).len()
}
