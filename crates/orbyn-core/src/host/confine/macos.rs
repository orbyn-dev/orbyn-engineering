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
/// A saved-conversations folder is denied whole, and so is the prompt history. A worker of
/// another engine gets nothing back. A Claude worker then gets the one session it owns,
/// allowed by pattern: `<id>.jsonl` or anything under `<id>/` in a project folder directly
/// under the root. The core has already made `projects` and the project folder for the
/// worker's start folder, so nothing may be created at either: a rename moves a prepared
/// tree in under a new name, which the profile cannot tell from a new directory, so no
/// create is allowed there at all. The pattern keeps working for a folder named in a way this
/// code does not repeat, and a session another thread creates later stays denied. The folder
/// and the pattern's own parts arrive as parameters (`TQ` is the folder with its pattern
/// characters escaped).
const TRANSCRIPTS: &str = r#"(deny file-read* file-write* (subpath (param "TR")))
(deny file-read* file-write* (literal (param "TH")))
"#;
const OWN_SESSION: &str = r#"(allow file-read-metadata (literal (param "TR")) (regex (string-append "^" (param "TQ") "/[^/]+$")))
(allow file-read* file-write* (regex (string-append "^" (param "TQ") "/[^/]+/" (param "TS") "(\\.jsonl|/.*)?$")))
"#;
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
    if confinement.transcripts.is_some() {
        text.push_str(TRANSCRIPTS);
        if confinement
            .transcripts
            .as_ref()
            .is_some_and(|t| t.session.is_some())
        {
            text.push_str(OWN_SESSION);
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

/// What the escape-check proof adds to every worker rule (it runs after them, so later rules win): no network, no creating a job
/// through launchd (`launchctl submit` runs a program from launchd, outside this profile), no writing of any application's preferences
/// (`defaults write` goes through cfprefsd, and another app's settings can later start a program outside this profile), no clipboard,
/// and file writes only in the
/// proof's own folder (`RUN`) and the two device files a process needs. The data folder's denial is repeated last, with the proof's
/// own folder and the folders on the way to it opened again after it.
const PROOF_EXTRAS: &str = "(deny network*)\n\
    (deny job-creation)\n\
    (deny user-preference-write)\n\
    (deny mach-lookup (global-name \"com.apple.pasteboard.1\"))\n\
    (deny file-write* (subpath \"/\"))\n\
    (allow file-write* (literal \"/dev/null\") (literal \"/dev/dtracehelper\"))\n\
    (deny file-read* file-write* (subpath (param \"DATA\")))\n\
    (allow file-read* file-write* (subpath (param \"RUN\")))\n";

/// The profile of the escape-check proof: the profile a crew worker would get for `confinement` (every worker rule, the transcripts'
/// denial and `(deny process-info* (target others))` included), then [`PROOF_EXTRAS`], then the folders on the way to the proof's own
/// folder opened for a look again, since the repeated data-folder denial would otherwise close them.
fn proof_profile(confinement: &Confinement, secrets: usize) -> String {
    let mut text = profile(confinement);
    text.push_str(PROOF_EXTRAS);
    for index in 0..secrets {
        text.push_str(&format!(
            "(deny file-read* (subpath (param \"S{index}\")))\n"
        ));
    }
    for index in 0..route(confinement).len() {
        text.push_str(&format!(
            "(allow file-read-metadata (literal (param \"M{index}\")))\n"
        ));
    }
    text
}

/// The data folder and every folder above it, up to and including `/`, then the same for the
/// saved-conversations folder: renaming `~/.claude` would put the transcripts under a name
/// the rules do not cover.
fn above(confinement: &Confinement) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let roots =
        std::iter::once(&confinement.data).chain(confinement.transcripts.iter().map(|t| &t.root));
    for root in roots {
        for folder in root.ancestors() {
            if !found.iter().any(|f| f == folder) {
                found.push(folder.to_path_buf());
            }
        }
    }
    found
}

/// `path` as text for a pattern: every character the pattern language reads as an operator
/// gets a backslash, so a folder named `a+b (1)` matches itself and nothing else.
fn pattern_text(path: &Path) -> OsString {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let mut out = Vec::new();
    for &byte in path.as_os_str().as_bytes() {
        if b".^$*+?()[]{}|\\".contains(&byte) {
            out.push(b'\\');
        }
        out.push(byte);
    }
    OsString::from_vec(out)
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
    if let Some(transcripts) = &confinement.transcripts {
        all.push(("TR".to_owned(), transcripts.root.clone()));
        if let Some(session) = &transcripts.session {
            all.push((
                "TQ".to_owned(),
                PathBuf::from(pattern_text(&transcripts.root)),
            ));
            all.push(("TS".to_owned(), PathBuf::from(session)));
        }
        all.push(("TH".to_owned(), transcripts.history.clone()));
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
    wrap_with(confinement, command, profile(confinement), Vec::new())
}

fn wrap_with(
    confinement: &Confinement,
    command: &mut ResolvedCommand,
    profile: String,
    extra: Vec<(String, PathBuf)>,
) -> Result<()> {
    if let Some(reason) = unavailable() {
        return Err(CoreError::EngineUnavailable(reason));
    }
    let mut prefix: Vec<OsString> = vec!["-p".into(), profile.into()];
    for (name, path) in parameters(confinement).into_iter().chain(extra) {
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

/// The credential folders under the home, relative to it. Toolchain folders (`~/.cargo`, `~/.rustup`, npm's) are not listed: a test
/// needs them.
const SECRET_FOLDERS: [&str; 8] = [
    ".ssh",
    ".aws",
    ".gnupg",
    ".config/gh",
    ".netrc",
    ".docker",
    ".kube",
    "Library/Keychains",
];

/// The person's credential folders that the proof may not read, named by parameter `S0`, `S1`..., never in the profile text. Under
/// each home (`HOME`, and the account's own home from the user database when it differs, since the service may start with another
/// `HOME`) a folder is denied as written, under the home's real path, at its own real path when it is a link (a dotfiles `~/.ssh`),
/// and at the real path of every link inside it (`~/.ssh/id_ed25519 -> dotfiles/...`). Fails closed: an unset, empty or relative
/// `HOME`, or a home that does not resolve (either one), is an error, never an empty list, because the proof would then run with no
/// credential denials at all.
fn proof_secrets(home: Option<OsString>, account_home: Option<PathBuf>) -> Result<Vec<PathBuf>> {
    let refuse = |why: &str| {
        CoreError::EngineUnavailable(format!(
            "The check cannot be restricted from the credential folders: {why}."
        ))
    };
    let home = home
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| refuse("HOME is not set"))?;
    if !home.is_absolute() {
        return Err(refuse("HOME is not an absolute path"));
    }
    let account = account_home
        .filter(|path| path.is_absolute())
        .ok_or_else(|| refuse("the account's home folder is unknown"))?;
    let mut found: Vec<PathBuf> = Vec::new();
    for base in [home, account] {
        let real_base = std::fs::canonicalize(&base)
            .map_err(|_| refuse(&format!("{} does not resolve", base.display())))?;
        for part in SECRET_FOLDERS {
            for named in [base.join(part), real_base.join(part)] {
                let resolved = std::fs::canonicalize(&named).ok();
                for path in std::iter::once(named).chain(resolved.clone()) {
                    push_unique(&mut found, path);
                }
                if let Some(real) = resolved {
                    let targets = linked_targets(&real, &real_base).ok_or_else(|| {
                        refuse("a credential folder holds too many entries to check its links")
                    })?;
                    for target in targets {
                        push_unique(&mut found, target);
                    }
                }
            }
        }
    }
    Ok(found)
}

fn push_unique(found: &mut Vec<PathBuf>, path: PathBuf) {
    if !found.contains(&path) {
        found.push(path);
    }
}

/// The real path of each link found inside `folder`, left out when it is the home or holds the home, which a rule would turn into a
/// denial of everything. `None` when the folder is bigger or deeper than the bound: a walk that stops early would leave links
/// unchecked, so the caller refuses (fails closed) instead.
fn linked_targets(folder: &Path, home: &Path) -> Option<Vec<PathBuf>> {
    const DEPTH: usize = 8;
    const ENTRIES: usize = 2000;
    let mut targets = Vec::new();
    let mut seen = 0;
    let mut pending = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            seen += 1;
            if seen > ENTRIES {
                return None;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                if let Ok(real) = std::fs::canonicalize(&path)
                    && !home.starts_with(&real)
                {
                    targets.push(real);
                }
            } else if kind.is_dir() {
                if depth + 1 >= DEPTH {
                    return None;
                }
                pending.push((path, depth + 1));
            }
        }
    }
    Some(targets)
}

/// The account's home folder from the user database (not the environment).
fn account_home() -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let mut buffer = vec![0u8; 16 * 1024];
    // SAFETY: `passwd` is plain data; the buffer outlives the call and its size is passed; `pw_dir` is read only on success, while
    // the buffer is alive, and copied out.
    unsafe {
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let status = libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        );
        if status != 0 || found.is_null() || entry.pw_dir.is_null() {
            return None;
        }
        let dir = std::ffi::CStr::from_ptr(entry.pw_dir).to_bytes().to_vec();
        (!dir.is_empty()).then(|| PathBuf::from(OsString::from_vec(dir)))
    }
}

/// Whether the proof's credential denials can be built on this machine (`None`), or why not. Checked before a proof is allowed to
/// count as confined, so an unresolvable `HOME` leaves the check waiting for a person's approval instead of running without them.
pub fn proof_unavailable() -> Option<String> {
    proof_secrets(std::env::var_os("HOME"), account_home())
        .err()
        .map(|error| error.to_string())
}

/// [`wrap`] with the proof's profile ([`proof_profile`]) and the credential folders as extra parameters.
pub fn wrap_proof(confinement: &Confinement, command: &mut ResolvedCommand) -> Result<()> {
    wrap_proof_at(
        confinement,
        command,
        std::env::var_os("HOME"),
        account_home(),
    )
}

fn wrap_proof_at(
    confinement: &Confinement,
    command: &mut ResolvedCommand,
    home: Option<OsString>,
    account: Option<PathBuf>,
) -> Result<()> {
    let secrets = proof_secrets(home, account)?;
    let profile = proof_profile(confinement, secrets.len());
    let extra = secrets
        .into_iter()
        .enumerate()
        .map(|(index, path)| (format!("S{index}"), path))
        .collect();
    wrap_with(confinement, command, profile, extra)
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
    let mut command = std::process::Command::new(tool);
    command.args(["-p", "(version 1)(allow default)", "/usr/bin/true"]);
    // The probe runs the system restriction tool; it too goes through the one gate.
    crate::host::child_env::for_tool(&mut command);
    let run = command.output();
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

#[cfg(test)]
mod proof_tests {
    use super::*;
    use crate::testing::TempDir;

    fn run_under(
        home: Option<OsString>,
        account: Option<PathBuf>,
        script: &str,
    ) -> Result<Option<bool>> {
        let dir = TempDir::new();
        let data = std::fs::canonicalize(dir.path()).unwrap().join("data");
        let run = data.join("runs/p");
        std::fs::create_dir_all(&run).unwrap();
        let rules = Confinement::new(&data, &run, &[], &[]).unwrap();
        let mut command = ResolvedCommand {
            program: "/bin/sh".into(),
            prefix: vec!["-c".into(), script.into()],
            path: "/bin/sh".into(),
        };
        wrap_proof_at(&rules, &mut command, home, account)?;
        let status = std::process::Command::new(&command.program)
            .args(&command.prefix)
            .current_dir(&run)
            .status()
            .unwrap();
        Ok(Some(status.success()))
    }

    #[test]
    fn a_home_that_is_unset_empty_relative_or_missing_is_refused() {
        let dir = TempDir::new();
        let here = Some(dir.path().to_path_buf());
        for home in [None, Some(OsString::new()), Some("relative".into())] {
            assert!(proof_secrets(home, here.clone()).is_err());
        }
        let missing = dir.path().join("missing");
        assert!(proof_secrets(Some(missing.clone().into_os_string()), here.clone()).is_err());
        // The account's own home must resolve too, whatever HOME says.
        let home = Some(dir.path().as_os_str().to_owned());
        assert!(proof_secrets(home.clone(), None).is_err());
        assert!(proof_secrets(home.clone(), Some(missing)).is_err());
        assert!(proof_secrets(home, here).is_ok());
        let mut command = ResolvedCommand {
            program: "/usr/bin/true".into(),
            prefix: Vec::new(),
            path: "/usr/bin/true".into(),
        };
        let data = std::fs::canonicalize(dir.path()).unwrap();
        let run = data.join("run");
        std::fs::create_dir_all(&run).unwrap();
        let rules = Confinement::new(&data, &run, &[], &[]).unwrap();
        assert!(wrap_proof_at(&rules, &mut command, None, Some(dir.path().to_path_buf())).is_err());
        assert_eq!(
            command.program,
            Path::new("/usr/bin/true"),
            "left as it was"
        );
    }

    #[test]
    fn a_linked_credential_folder_is_denied_by_both_names() {
        if unavailable().is_some() {
            return;
        }
        let dir = TempDir::new();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let home = base.join("home");
        let dotfiles = base.join("dotfiles");
        for folder in [".ssh", ".cargo"] {
            std::fs::create_dir_all(dotfiles.join(folder)).unwrap();
            std::fs::write(dotfiles.join(folder).join("file"), "key").unwrap();
        }
        std::fs::create_dir_all(&home).unwrap();
        for folder in [".ssh", ".cargo"] {
            std::os::unix::fs::symlink(dotfiles.join(folder), home.join(folder)).unwrap();
        }
        let through_link = home.join(".ssh/file");
        let through_target = dotfiles.join(".ssh/file");
        let at = |path: &Path| format!("/bin/cat '{}'", path.display());
        let run = |script: &str| {
            let account = Some(home.clone());
            run_under(Some(home.clone().into_os_string()), account, script).unwrap()
        };
        // The probe reads the same files unrestricted first, so a denial is the restriction and not a broken command.
        assert!(std::fs::read(&through_target).is_ok());
        for path in [&through_link, &through_target] {
            assert_eq!(run(&at(path)), Some(false), "{}", path.display());
        }
        // A toolchain folder at the same kind of link stays readable.
        assert_eq!(run(&at(&home.join(".cargo/file"))), Some(true));
    }

    /// A credential folder too big to check for links makes the proof unconfined (a person approves it) rather than half checked.
    #[test]
    fn a_credential_folder_too_big_to_check_fails_closed() {
        let dir = TempDir::new();
        let home = dir.path().join("home");
        std::fs::create_dir_all(home.join(".kube")).unwrap();
        for index in 0..2001 {
            std::fs::write(home.join(".kube").join(format!("c{index}")), "").unwrap();
        }
        let home = Some(home.into_os_string());
        let here = Some(dir.path().to_path_buf());
        assert!(proof_secrets(home, here).is_err());
    }

    /// A single key file that is a link out of a real credential folder is denied at its target too.
    #[test]
    fn a_linked_file_inside_a_credential_folder_is_denied_at_its_target() {
        if unavailable().is_some() {
            return;
        }
        let dir = TempDir::new();
        let home = dir.path().join("home");
        let dotfiles = dir.path().join("dotfiles");
        std::fs::create_dir_all(home.join(".gnupg")).unwrap();
        std::fs::create_dir_all(&dotfiles).unwrap();
        std::fs::write(dotfiles.join("secret"), "key").unwrap();
        std::os::unix::fs::symlink(dotfiles.join("secret"), home.join(".gnupg/key")).unwrap();
        let target = dotfiles.join("secret");
        let account = Some(home.clone());
        assert!(std::fs::read(&target).is_ok());
        let script = format!("/bin/cat '{}'", target.display());
        let denied = run_under(Some(home.into_os_string()), account, &script).unwrap();
        assert_eq!(denied, Some(false));
    }

    /// The service may start with another `HOME`: the account's own home folder, from the user database, is denied as well.
    #[test]
    fn the_accounts_own_home_is_denied_when_home_points_elsewhere() {
        if unavailable().is_some() {
            return;
        }
        let dir = TempDir::new();
        let (home, account) = (dir.path().join("elsewhere"), dir.path().join("account"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(account.join(".aws")).unwrap();
        std::fs::write(account.join(".aws/credentials"), "key").unwrap();
        let script = format!("/bin/cat '{}'", account.join(".aws/credentials").display());
        let denied = run_under(Some(home.into_os_string()), Some(account), &script).unwrap();
        assert_eq!(denied, Some(false));
    }
}
