//! What a spawned program inherits. Orbyn can start from places that leave their own
//! markers behind: inside a coding agent's session, or from an AppImage. Those change
//! how a child behaves without any sign, so every spawn goes through this file. Only
//! variable names are ever reported; a value is compared, never kept or logged.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(unix)]
use std::time::Duration;

/// Set by Claude Code in everything it starts. A CLI that sees it treats itself as a
/// nested session.
const NESTED_MARKER: &str = "CLAUDECODE";
/// Under the marker, the rest of the host session's variables go too, including ones
/// that switch a child into a reduced mode. These stay: a person sets them.
const CLAUDE_SETTINGS: &[&str] = &[
    "CLAUDE_CONFIG_DIR",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_GIT_BASH_PATH",
];
/// Switches that point Claude at a development login service. Inherited from a Claude
/// app or a session it started, they make `claude -p` load no claude.ai connectors at
/// all, with no message. An engine child never needs them; a person who does sets them
/// in the provider's own environment, which is applied after this plan.
const OAUTH_DEV_SWITCHES: &[&str] = &["USE_LOCAL_OAUTH", "USE_STAGING_OAUTH"];
/// What a Codex session leaves for the commands it runs.
const CODEX_SESSION: &[&str] = &[
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "CODEX_THREAD_ID",
];
/// Set by an AppImage's launcher. They name a mount that a child has no use for.
const APPIMAGE_ONLY: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
    "PYTHONHOME",
    "GDK_PIXBUF_MODULE_FILE",
];
/// Lists that an AppImage extends with folders inside its mount.
const APPIMAGE_LISTS: &[&str] = &["LD_LIBRARY_PATH", "XDG_DATA_DIRS", "PATH"];

const CLAUDE_CREDENTIALS: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
];
const CODEX_CREDENTIALS: &[&str] = &["OPENAI_API_KEY", "CODEX_API_KEY"];
const ENDPOINTS: &[&str] = &["ANTHROPIC_BASE_URL", "OPENAI_BASE_URL"];

/// Prefixes that belong to the account: a sign-in, this device's keys, the keychain. Nothing Orbyn
/// starts may carry a name under one of them, and no child may inherit one, whatever it holds
/// (docs/account-threat-model.md, control 2). Names are compared without regard to case on every
/// platform: Windows ignores case, and a child that reads `orbyn_account_x` should not either.
const ACCOUNT_PREFIXES: &[&str] = &["ORBYN_ACCOUNT_", "ORBYN_DEVICE_", "ORBYN_KEYSTORE_"];
/// Names outside those prefixes that would hold account material if anyone ever set them: the
/// vault key, the session and service tokens, the password, the Engine's sign-in setup secret.
const ACCOUNT_NAMES: &[&str] = &[
    "ORBYN_VAULT_KEY",
    "ORBYN_SESSION_TOKEN",
    "ORBYN_SERVICE_TOKEN",
    "ORBYN_PASSWORD",
    "ORBYN_AUTH_SETUP",
];

/// Whether `name` is reserved for account material, and so never reaches a child.
pub fn is_account_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    ACCOUNT_PREFIXES
        .iter()
        .any(|prefix| upper.starts_with(prefix))
        || ACCOUNT_NAMES.contains(&upper.as_str())
}

static PREFER_SAVED_LOGIN: AtomicBool = AtomicBool::new(false);

/// The `prefer_saved_login` preference, kept here because spawning has no store.
pub fn set_prefer_saved_login(prefer: bool) {
    PREFER_SAVED_LOGIN.store(prefer, Ordering::Relaxed);
}

/// The current `prefer_saved_login` value, which changes what an engine inherits.
pub fn prefers_saved_login() -> bool {
    PREFER_SAVED_LOGIN.load(Ordering::Relaxed)
}

/// Changes to apply to an inherited environment.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnvPlan {
    pub remove: Vec<OsString>,
    pub set: Vec<(OsString, OsString)>,
}

fn name_of(name: &OsStr) -> String {
    name.to_string_lossy().into_owned()
}

/// Whether an inherited `ANTHROPIC_BASE_URL` belongs to the host Claude session rather
/// than to the person: it is nested, and no key or token for a gateway is set beside it.
fn drops_host_endpoint(nested: bool, set: impl Fn(&str) -> bool) -> bool {
    nested && !set("ANTHROPIC_AUTH_TOKEN") && !set("ANTHROPIC_API_KEY")
}

/// Works out what to change. `engine` is true for a spawn that runs a coding engine,
/// the only one a login preference applies to.
pub fn plan(vars: &[(OsString, OsString)], engine: bool, prefer_saved: bool) -> EnvPlan {
    let has = |wanted: &str| vars.iter().any(|(name, _)| name == wanted);
    let nested = has(NESTED_MARKER);
    let app_dir = vars
        .iter()
        .find(|(name, _)| name == "APPDIR")
        .filter(|_| has("APPIMAGE"))
        .map(|(_, value)| PathBuf::from(value));
    let mut changes = EnvPlan::default();
    for (name, value) in vars {
        let text = name_of(name);
        let host_session =
            nested && text.starts_with("CLAUDE") && !CLAUDE_SETTINGS.contains(&text.as_str());
        let scrubbed_login = engine
            && prefer_saved
            && (CLAUDE_CREDENTIALS.contains(&text.as_str())
                || CODEX_CREDENTIALS.contains(&text.as_str())
                || ENDPOINTS.contains(&text.as_str()));
        // The endpoint under the marker was set by the host session, not by the person,
        // unless a credential for it travels with it: that is a gateway of their own, and
        // dropping the URL would send its token to the default host.
        let host_endpoint =
            engine && text == "ANTHROPIC_BASE_URL" && drops_host_endpoint(nested, |name| has(name));
        let dev_login = engine && OAUTH_DEV_SWITCHES.contains(&text.as_str());
        // Account material goes from every child: an engine, a tool, a terminal.
        let account = is_account_name(&text);
        if account
            || host_session
            || host_endpoint
            || dev_login
            || scrubbed_login
            || CODEX_SESSION.contains(&text.as_str())
        {
            changes.remove.push(name.clone());
            continue;
        }
        let Some(mount) = &app_dir else { continue };
        if APPIMAGE_ONLY.contains(&text.as_str()) {
            changes.remove.push(name.clone());
        } else if APPIMAGE_LISTS.contains(&text.as_str()) {
            let kept: Vec<PathBuf> = std::env::split_paths(value)
                .filter(|entry| !entry.starts_with(mount))
                .collect();
            if kept.is_empty() {
                changes.remove.push(name.clone());
            } else if let Ok(joined) = std::env::join_paths(&kept)
                && joined != *value
            {
                changes.set.push((name.clone(), joined));
            }
        }
    }
    changes
}

fn apply(command: &mut Command, engine: bool) {
    let vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let changes = plan(&vars, engine, PREFER_SAVED_LOGIN.load(Ordering::Relaxed));
    for name in &changes.remove {
        command.env_remove(name);
    }
    for (name, value) in &changes.set {
        if name != "PATH" {
            command.env(name, value);
        }
    }
    let base = changes
        .set
        .iter()
        .find(|(name, _)| name == "PATH")
        .map(|(_, value)| value.clone())
        .or_else(|| std::env::var_os("PATH"));
    command.env("PATH", widened(base.as_deref()));
}

/// For git, forge CLIs, browsers and anything else that is not an engine.
pub fn for_tool(command: &mut Command) {
    apply(command, false);
}

/// For a coding engine.
pub fn for_engine(command: &mut Command) {
    apply(command, true);
}

/// Names to drop and values to set on a terminal. A shell is the person's own, so
/// the login preference does not reach it; an engine's console (`engine`) is an
/// engine, so it does.
pub fn for_terminal(engine: bool) -> EnvPlan {
    let vars: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    terminal_plan(
        &vars,
        engine,
        engine && PREFER_SAVED_LOGIN.load(Ordering::Relaxed),
    )
}

/// The terminal's plan for the given variables, so a test can feed it any set.
fn terminal_plan(vars: &[(OsString, OsString)], engine: bool, prefer_saved: bool) -> EnvPlan {
    let mut changes = plan(vars, engine, prefer_saved);
    changes.set.retain(|(name, _)| name != "PATH");
    let base = vars
        .iter()
        .find(|(name, _)| name == "PATH")
        .map(|(_, value)| value.clone());
    changes.set.push(("PATH".into(), widened(base.as_deref())));
    // A Windows shell writes PowerShell's module analysis cache on startup, and that file must
    // not land in the checkout the shell opens in (issue 1788).
    #[cfg(windows)]
    powershell_cache(vars, &mut changes);
    changes
}

/// PowerShell's module analysis cache file. Left alone, PowerShell builds it from the
/// profile's local app-data folder, and a folder that comes back empty makes the path it
/// builds relative: resolved from the shell's working directory, so the cache lands in the
/// person's checkout and `git status` reports `?? Microsoft/`.
#[cfg(windows)]
const POWERSHELL_CACHE: &str = "PSModuleAnalysisCachePath";

/// Points the cache at Orbyn's data folder; a person's own absolute path is left alone.
/// The value is always absolute, so no shell can resolve it under its working directory,
/// and PowerShell creates the folder itself. Paths beside it (the file name) stay its own.
#[cfg(windows)]
fn powershell_cache(vars: &[(OsString, OsString)], changes: &mut EnvPlan) {
    let inherited = vars
        .iter()
        .find(|(name, _)| crate::app::is_named(name, POWERSHELL_CACHE));
    // A relative path is exactly what the shell resolves from its working directory.
    if inherited.is_some_and(|(_, value)| Path::new(value).is_absolute()) {
        return;
    }
    let Some(folder) = cache_folder(vars) else {
        return;
    };
    // The inherited spelling goes, so the child never holds two names for one setting.
    if let Some((name, _)) = inherited {
        changes.remove.push(name.clone());
    }
    changes.set.push((
        POWERSHELL_CACHE.into(),
        folder
            .join("powershell")
            .join("ModuleAnalysisCache")
            .into_os_string(),
    ));
}

/// Orbyn's data folder when the variables name an absolute one, then a temp folder: never a
/// relative one, which the shell would resolve from its working directory like any other.
#[cfg(windows)]
fn cache_folder(vars: &[(OsString, OsString)]) -> Option<PathBuf> {
    let absolute = |folder: PathBuf| folder.is_absolute().then_some(folder);
    let named = |name: &str| {
        vars.iter()
            .find(|(key, _)| crate::app::is_named(key, name))
            .map(|(_, value)| PathBuf::from(value))
    };
    crate::app::data_dir_from(vars)
        .and_then(absolute)
        .or_else(|| {
            ["TEMP", "TMP"]
                .into_iter()
                .find_map(named)
                .and_then(absolute)
        })
        .or_else(|| absolute(std::env::temp_dir()))
}

/// Which credential variables an engine would inherit, by name, and the origin of
/// any endpoint override. The origin drops a path and any user info, which is where
/// a secret could sit.
pub fn inherited_login(engine: &str) -> (Vec<String>, Option<String>) {
    let (names, endpoint): (&[&str], &str) = match engine {
        "claude" => (CLAUDE_CREDENTIALS, "ANTHROPIC_BASE_URL"),
        "codex" => (CODEX_CREDENTIALS, "OPENAI_BASE_URL"),
        _ => return (Vec::new(), None),
    };
    if PREFER_SAVED_LOGIN.load(Ordering::Relaxed) {
        return (Vec::new(), None);
    }
    let set = |name: &&&str| std::env::var_os(**name).is_some_and(|value| !value.is_empty());
    let found = names.iter().filter(set).map(|name| (*name).to_owned());
    // The origin an engine really inherits: a host session's endpoint is removed from it.
    let dropped = engine == "claude"
        && drops_host_endpoint(std::env::var_os(NESTED_MARKER).is_some(), |name| {
            std::env::var_os(name).is_some()
        });
    let origin = std::env::var(endpoint)
        .ok()
        .filter(|_| !dropped)
        .and_then(|url| url_origin(&url));
    (found.collect(), origin)
}

/// `scheme://host[:port]`, or `None` for anything that does not parse as a URL.
pub fn url_origin(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let port = parsed
        .port()
        .map_or_else(String::new, |port| format!(":{port}"));
    Some(format!("{}://{host}{port}", parsed.scheme()))
}

/// The sentinel the login shell prints around its `PATH`, so rc-file noise can be
/// told apart from the answer.
#[cfg(unix)]
const LOGIN_PATH_SENTINEL: &str = "__ORBYN_PATH__";

/// The child runs with only what a login shell needs to start; the service's own
/// variables (tokens, `ORBYN_*` settings, keys) must not reach a person's rc files.
#[cfg(unix)]
const LOGIN_SHELL_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

/// A login shell that takes longer than this is killed and the answer is refused.
#[cfg(all(unix, not(test)))]
const LOGIN_PATH_TIMEOUT: Duration = Duration::from_secs(3);

/// The person's login-shell `PATH`, read once per process and kept for every spawn.
#[cfg(all(unix, not(test)))]
static LOGIN_PATH: OnceLock<Option<Vec<PathBuf>>> = OnceLock::new();

/// Asks the person's login shell for its `PATH` now, in the calling thread. The
/// answer is cached for every later spawn; the service calls this in the background.
pub fn warm_login_path() {
    let _ = login_path();
}

/// The cached login-shell `PATH`. Tests never run a shell.
fn login_path() -> Option<&'static [PathBuf]> {
    #[cfg(all(unix, not(test)))]
    {
        LOGIN_PATH
            .get_or_init(|| {
                login_shell().and_then(|shell| login_shell_path(&shell, LOGIN_PATH_TIMEOUT))
            })
            .as_deref()
    }
    #[cfg(not(all(unix, not(test))))]
    {
        None
    }
}

/// The shell to ask: `SHELL` when it names an existing absolute file, otherwise the
/// platform's login shell. `ORBYN_NO_LOGIN_SHELL=1` skips the question entirely.
#[cfg(all(unix, not(test)))]
fn login_shell() -> Option<PathBuf> {
    if std::env::var_os("ORBYN_NO_LOGIN_SHELL").as_deref() == Some(OsStr::new("1")) {
        return None;
    }
    let chosen = std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file());
    chosen.or_else(|| {
        let fallback = PathBuf::from(if cfg!(target_os = "macos") {
            "/bin/zsh"
        } else {
            "/bin/sh"
        });
        fallback.is_file().then_some(fallback)
    })
}

/// Runs one interactive login shell with a cleared environment and reads its `PATH`
/// between the sentinels. The child is killed if it outlives `timeout`.
#[cfg(unix)]
fn login_shell_path(shell: &Path, timeout: Duration) -> Option<Vec<PathBuf>> {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Instant;

    let script = format!("printf '{LOGIN_PATH_SENTINEL}%s{LOGIN_PATH_SENTINEL}' \"$PATH\"");
    let mut command = Command::new(shell);
    command
        .arg("-i")
        .arg("-l")
        .arg("-c")
        .arg(&script)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    for name in ["HOME", "USER", "LOGNAME"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command.env("SHELL", shell).env("PATH", LOGIN_SHELL_PATH);
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    let bytes = receiver.recv_timeout(remaining).ok()?;
    parse_login_path(&String::from_utf8_lossy(&bytes))
}

/// The entries between the sentinel pair, without the relative ones a broken profile
/// can leave in `PATH`. No pair, or no absolute entry, means no answer.
#[cfg(unix)]
fn parse_login_path(output: &str) -> Option<Vec<PathBuf>> {
    let start = output.find(LOGIN_PATH_SENTINEL)? + LOGIN_PATH_SENTINEL.len();
    let end = output[start..].find(LOGIN_PATH_SENTINEL)? + start;
    let entries: Vec<PathBuf> = std::env::split_paths(OsStr::new(&output[start..end]))
        .filter(|entry| entry.is_absolute())
        .collect();
    (!entries.is_empty()).then_some(entries)
}

/// PATH with the person's login-shell entries and the usual install folders added
/// after what is already there. On Unix the service runs the person's login shell
/// once, interactive login (`$SHELL -ilc`), with one fixed command that contains no
/// user input; it reads back only `PATH`, runs with a cleared environment and a
/// 3 second limit, falls back silently on any failure, and can be switched off with
/// `ORBYN_NO_LOGIN_SHELL=1`. An app started from Finder, the Dock or the Start menu
/// gets a minimal PATH; the person's shell knows which of their installs comes first.
pub fn widened(path: Option<&OsStr>) -> OsString {
    static DIRS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    let dirs = DIRS.get_or_init(|| {
        tool_dirs(&Install::from_process(), cfg!(windows))
            .into_iter()
            .filter(|dir| dir.is_dir())
            .collect()
    });
    widened_with(path, login_path(), dirs)
}

/// The ordering rule on its own, so a test can inject the login and install folders.
fn widened_with(path: Option<&OsStr>, login: Option<&[PathBuf]>, dirs: &[PathBuf]) -> OsString {
    let mut entries: Vec<PathBuf> = path.map_or_else(Vec::new, |path| {
        std::env::split_paths(path).collect::<Vec<_>>()
    });
    for dir in login.unwrap_or_default().iter().chain(dirs) {
        if !entries.contains(dir) {
            entries.push(dir.clone());
        }
    }
    std::env::join_paths(entries).unwrap_or_else(|_| path.unwrap_or_default().to_owned())
}

/// What the search is built from, so both platforms' rules run under test anywhere.
#[derive(Debug, Default)]
pub struct Install {
    pub home: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub system_root: Option<PathBuf>,
    pub program_files: Option<PathBuf>,
    pub nvm_symlink: Option<PathBuf>,
    pub n_prefix: Option<PathBuf>,
    /// `prefix=` from the npm config, or `NPM_CONFIG_PREFIX`.
    pub npm_prefix: Option<PathBuf>,
    /// Installed node versions under nvm or `n`, each `.../bin`.
    pub node_versions: Vec<PathBuf>,
}

impl Install {
    fn from_process() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let home = var("USERPROFILE")
            .filter(|_| cfg!(windows))
            .or_else(|| var("HOME"));
        let n_prefix = var("N_PREFIX");
        let mut node_versions = Vec::new();
        for root in [
            home.as_ref().map(|home| home.join(".nvm/versions/node")),
            Some(
                n_prefix
                    .clone()
                    .unwrap_or_else(|| "/usr/local".into())
                    .join("n/versions/node"),
            ),
        ]
        .into_iter()
        .flatten()
        {
            node_versions.extend(newest_version(&root).map(|dir| dir.join("bin")));
        }
        let npmrc = home
            .as_ref()
            .and_then(|home| std::fs::read_to_string(home.join(".npmrc")).ok());
        Self {
            npm_prefix: var("NPM_CONFIG_PREFIX")
                .or_else(|| npmrc.as_deref().and_then(npm_prefix).map(PathBuf::from)),
            app_data: var("APPDATA"),
            local_app_data: var("LOCALAPPDATA"),
            system_root: var("SystemRoot"),
            program_files: var("ProgramFiles"),
            nvm_symlink: var("NVM_SYMLINK"),
            n_prefix,
            node_versions,
            home,
        }
    }
}

/// The highest `vN.N.N` folder, by number rather than by name.
fn newest_version(root: &Path) -> Option<PathBuf> {
    let parse = |name: &str| -> Option<Vec<u64>> {
        name.trim_start_matches('v')
            .split('.')
            .map(|part| part.parse().ok())
            .collect()
    };
    std::fs::read_dir(root)
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            Some((parse(&entry.file_name().to_string_lossy())?, entry.path()))
        })
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, path)| path)
}

/// The `prefix` line of an `.npmrc`. A value with a variable in it is left alone,
/// because expanding it is npm's business.
pub fn npm_prefix(text: &str) -> Option<&str> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        let value = value.trim().trim_matches('"');
        (key.trim() == "prefix" && !value.is_empty() && !value.contains('$')).then_some(value)
    })
}

/// Folders where tools are usually installed, most likely first.
pub fn tool_dirs(install: &Install, windows: bool) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let in_home = |paths: &[&str], dirs: &mut Vec<PathBuf>| {
        if let Some(home) = &install.home {
            dirs.extend(paths.iter().map(|path| home.join(path)));
        }
    };
    if windows {
        dirs.extend(install.app_data.iter().map(|dir| dir.join("npm")));
        if let Some(local) = &install.local_app_data {
            dirs.push(local.join("pnpm"));
            dirs.push(local.join("Volta").join("bin"));
            dirs.push(local.join("fnm_multishells"));
            dirs.push(local.join("Programs").join("nodejs"));
        }
        dirs.extend(install.nvm_symlink.iter().cloned());
        dirs.extend(install.program_files.iter().map(|dir| dir.join("nodejs")));
        dirs.extend(
            install
                .program_files
                .iter()
                .map(|dir| dir.join("Git").join("cmd")),
        );
        in_home(&[".cargo/bin", ".bun/bin", ".local/bin"], &mut dirs);
        dirs.extend(install.npm_prefix.iter().cloned());
        if let Some(root) = &install.system_root {
            dirs.push(root.join("System32").join("WindowsPowerShell").join("v1.0"));
        }
        return dirs;
    }
    dirs.extend(["/opt/homebrew/bin", "/opt/homebrew/sbin", "/usr/local/bin"].map(PathBuf::from));
    in_home(
        &[
            ".local/bin",
            ".cargo/bin",
            ".bun/bin",
            ".volta/bin",
            ".asdf/shims",
            ".local/share/mise/shims",
            ".npm-global/bin",
            "Library/pnpm",
            ".local/share/pnpm",
            ".fnm/aliases/default/bin",
            ".local/share/fnm/aliases/default/bin",
            "Library/Application Support/fnm/aliases/default/bin",
        ],
        &mut dirs,
    );
    dirs.extend(install.npm_prefix.iter().map(|prefix| prefix.join("bin")));
    dirs.extend(install.n_prefix.iter().map(|prefix| prefix.join("bin")));
    dirs.extend(install.node_versions.iter().cloned());
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(list: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        list.iter()
            .map(|(name, value)| ((*name).into(), (*value).into()))
            .collect()
    }

    fn removed(changes: &EnvPlan) -> Vec<String> {
        changes.remove.iter().map(|name| name_of(name)).collect()
    }

    #[test]
    fn a_nested_session_loses_its_host_variables_and_keeps_what_a_person_set() {
        let inherited = vars(&[
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_SIMPLE", "1"),
            ("CLAUDE_CODE_SESSION_ID", "s"),
            ("CLAUDE_CONFIG_DIR", "/mine"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("HOME", "/h"),
        ]);
        let gone = removed(&plan(&inherited, false, false));
        assert_eq!(
            gone,
            ["CLAUDECODE", "CLAUDE_CODE_SIMPLE", "CLAUDE_CODE_SESSION_ID"]
        );
    }

    #[test]
    fn a_host_session_endpoint_and_dev_login_switches_never_reach_an_engine() {
        // Under the marker the endpoint is the host's. Dev login switches go from any engine child.
        let nested = vars(&[
            ("CLAUDECODE", "1"),
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9"),
            ("USE_LOCAL_OAUTH", "1"),
            ("USE_STAGING_OAUTH", "1"),
        ]);
        let gone = removed(&plan(&nested, true, false));
        for name in ["ANTHROPIC_BASE_URL", "USE_LOCAL_OAUTH", "USE_STAGING_OAUTH"] {
            assert!(gone.contains(&name.to_owned()), "{name} stays: {gone:?}");
        }
        // Not nested: the endpoint is the person's own, and a tool child keeps every switch.
        let plain = vars(&[
            ("ANTHROPIC_BASE_URL", "http://proxy"),
            ("USE_LOCAL_OAUTH", "1"),
        ]);
        assert_eq!(removed(&plan(&plain, true, false)), ["USE_LOCAL_OAUTH"]);
        assert_eq!(plan(&plain, false, false), EnvPlan::default());
    }

    #[test]
    fn a_gateway_url_survives_when_its_key_or_token_is_set_beside_it() {
        for credential in ["ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY"] {
            let inherited = vars(&[
                ("CLAUDECODE", "1"),
                ("ANTHROPIC_BASE_URL", "https://litellm.example"),
                (credential, "k"),
            ]);
            let gone = removed(&plan(&inherited, true, false));
            assert!(
                !gone.contains(&"ANTHROPIC_BASE_URL".to_owned()),
                "{credential}: {gone:?}"
            );
        }
    }

    #[test]
    fn the_host_endpoint_never_leaves_git_forge_tools_or_a_terminal() {
        let nested = vars(&[
            ("CLAUDECODE", "1"),
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9"),
        ]);
        let gone = removed(&plan(&nested, false, false));
        assert!(!gone.contains(&"ANTHROPIC_BASE_URL".to_owned()), "{gone:?}");
    }

    #[test]
    fn the_endpoint_rule_is_one_decision_for_the_plan_and_the_login_report() {
        assert!(drops_host_endpoint(true, |_| false));
        assert!(!drops_host_endpoint(false, |_| false));
        assert!(!drops_host_endpoint(true, |name| name == "ANTHROPIC_API_KEY"));
        assert!(!drops_host_endpoint(true, |name| name == "ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn without_the_marker_claude_settings_are_left_alone() {
        let inherited = vars(&[
            ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "1"),
            ("ANTHROPIC_API_KEY", "k"),
        ]);
        assert_eq!(plan(&inherited, true, false), EnvPlan::default());
    }

    #[test]
    fn a_codex_session_marker_never_reaches_a_child() {
        let gone = removed(&plan(&vars(&[("CODEX_SANDBOX", "seatbelt")]), false, false));
        assert_eq!(gone, ["CODEX_SANDBOX"]);
    }

    #[test]
    fn an_appimage_is_removed_only_from_what_it_added() {
        let data_dirs = std::env::join_paths(["/tmp/.mount/share", "/usr/share"]).unwrap();
        let inherited = vars(&[
            ("APPIMAGE", "/a/app.AppImage"),
            ("APPDIR", "/tmp/.mount"),
            ("ARGV0", "app"),
            ("LD_LIBRARY_PATH", "/tmp/.mount/usr/lib"),
            ("XDG_DATA_DIRS", data_dirs.to_str().unwrap()),
        ]);
        let changes = plan(&inherited, false, false);
        let gone = removed(&changes);
        assert!(gone.contains(&"ARGV0".to_owned()), "{gone:?}");
        assert!(gone.contains(&"LD_LIBRARY_PATH".to_owned()), "{gone:?}");
        assert_eq!(
            changes.set,
            vec![("XDG_DATA_DIRS".into(), "/usr/share".into())]
        );
    }

    #[test]
    fn argv0_alone_is_not_an_appimage() {
        let inherited = vars(&[("ARGV0", "zsh"), ("APPDIR", "/x")]);
        assert_eq!(plan(&inherited, false, false), EnvPlan::default());
    }

    #[test]
    fn saved_login_applies_to_engines_only_and_only_when_asked() {
        let inherited = vars(&[
            ("ANTHROPIC_API_KEY", "k"),
            ("OPENAI_BASE_URL", "https://x"),
            ("PATH", "/bin"),
        ]);
        assert_eq!(plan(&inherited, true, false), EnvPlan::default());
        assert_eq!(plan(&inherited, false, true), EnvPlan::default());
        assert_eq!(
            removed(&plan(&inherited, true, true)),
            ["ANTHROPIC_API_KEY", "OPENAI_BASE_URL"]
        );
    }

    #[test]
    fn an_origin_never_carries_a_path_or_user_info() {
        assert_eq!(
            url_origin("https://me:secret@proxy.example:8443/v1/key").as_deref(),
            Some("https://proxy.example:8443")
        );
        assert_eq!(url_origin("not a url"), None);
    }

    #[test]
    fn existing_path_entries_keep_their_order_and_come_first() {
        let path = std::env::join_paths(["/first", "/second"]).unwrap();
        let wide = widened(Some(&path));
        let entries: Vec<PathBuf> = std::env::split_paths(&wide).collect();
        assert_eq!(
            entries[..2],
            [PathBuf::from("/first"), PathBuf::from("/second")]
        );
        let unique: std::collections::HashSet<_> = entries.iter().collect();
        assert_eq!(unique.len(), entries.len());
    }

    #[test]
    fn install_folders_follow_the_platform() {
        let install = Install {
            home: Some("/h".into()),
            app_data: Some("/roaming".into()),
            local_app_data: Some("/local".into()),
            system_root: Some("/win".into()),
            npm_prefix: Some("/prefix".into()),
            node_versions: vec!["/h/.nvm/versions/node/v22.1.0/bin".into()],
            ..Install::default()
        };
        let unix = tool_dirs(&install, false);
        assert!(unix.contains(&PathBuf::from("/opt/homebrew/bin")));
        assert!(unix.contains(&PathBuf::from("/h/.cargo/bin")));
        assert!(unix.contains(&PathBuf::from("/prefix/bin")));
        assert!(unix.contains(&PathBuf::from("/h/.nvm/versions/node/v22.1.0/bin")));
        let windows = tool_dirs(&install, true);
        assert_eq!(windows[0], PathBuf::from("/roaming/npm"));
        assert!(windows.contains(&PathBuf::from("/local/pnpm")));
        assert!(!windows.contains(&PathBuf::from("/opt/homebrew/bin")));
    }

    #[test]
    fn a_newer_version_wins_by_number_and_other_folders_are_ignored() {
        let dir = crate::testing::TempDir::new();
        for name in ["v9.11.0", "v22.1.0", "v22.10.0", "latest"] {
            std::fs::create_dir_all(dir.path().join(name)).unwrap();
        }
        assert_eq!(
            newest_version(dir.path()).unwrap().file_name().unwrap(),
            "v22.10.0"
        );
    }

    #[test]
    fn the_npm_prefix_line_is_found_and_a_variable_is_left_to_npm() {
        assert_eq!(npm_prefix("a=b\nprefix = \"/opt/n\"\n"), Some("/opt/n"));
        assert_eq!(npm_prefix("prefix=${HOME}/n\n"), None);
        assert_eq!(npm_prefix("registry=x\n"), None);
    }

    // ---- account material never reaches a child (account-threat-model control 2) ----

    /// A value nobody could guess and no source file holds.
    fn canary(label: &str) -> String {
        format!("{label}-{}", ulid::Ulid::generate())
    }

    /// Every reserved name and prefix, as a name a person could have set, with the spellings a case-blind
    /// system (Windows) or a careless script would give it.
    fn reserved() -> Vec<String> {
        let mut names = Vec::new();
        for base in [
            "ORBYN_ACCOUNT_PASSWORD",
            "ORBYN_ACCOUNT_",
            "ORBYN_ACCOUNT_SESSION_TOKEN",
            "ORBYN_DEVICE_KEYS",
            "ORBYN_DEVICE_",
            "ORBYN_KEYSTORE_URL",
            "ORBYN_KEYSTORE_",
            "ORBYN_VAULT_KEY",
            "ORBYN_SESSION_TOKEN",
            "ORBYN_SERVICE_TOKEN",
            "ORBYN_PASSWORD",
            "ORBYN_AUTH_SETUP",
        ] {
            names.push(base.to_owned());
            names.push(base.to_ascii_lowercase());
            let mixed: String = base
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    if i % 2 == 0 {
                        c
                    } else {
                        c.to_ascii_lowercase()
                    }
                })
                .collect();
            names.push(mixed);
        }
        names
    }

    /// What must stay: a person's own variables, names that only look like a reserved one, and the
    /// ones Orbyn itself passes on.
    const KEPT: &[&str] = &[
        "HOME",
        "ORBYN_DATA_DIR",
        "ORBYN_ENGINE_URL",
        "ORBYN_ACCOUNTS",
        "ORBYN_ACCOUNT",
        "ORBYN_DEVICE",
        "ORBYN_DEVICES_SEEN",
        "ORBYN_KEYSTORES",
        "MY_ORBYN_ACCOUNT_NAME",
        "PRE_ORBYN_DEVICE_ID",
        "ORBYN_VAULT",
        "ORBYN_TOKEN_COUNT",
    ];

    #[test]
    fn every_reserved_name_and_prefix_is_removed_for_a_tool_an_engine_and_a_terminal() {
        let mut inherited: Vec<(OsString, OsString)> = reserved()
            .into_iter()
            .map(|name| (name.into(), canary("reserved").into()))
            .collect();
        inherited.extend(
            KEPT.iter()
                .map(|name| ((*name).into(), canary("kept").into())),
        );
        for (kind, changes) in [
            ("tool", plan(&inherited, false, false)),
            ("engine", plan(&inherited, true, false)),
            ("engine, saved login", plan(&inherited, true, true)),
            ("terminal", terminal_plan(&inherited, false, false)),
            ("engine terminal", terminal_plan(&inherited, true, false)),
        ] {
            let gone = removed(&changes);
            for name in reserved() {
                assert!(gone.contains(&name), "{kind}: {name} stays");
            }
            for name in KEPT {
                assert!(!gone.contains(&(*name).to_owned()), "{kind}: {name} goes");
            }
            // Nothing is set from a reserved name either.
            assert!(
                changes
                    .set
                    .iter()
                    .all(|(name, _)| !is_account_name(&name_of(name))),
                "{kind}"
            );
        }
    }

    #[test]
    fn a_name_is_reserved_without_regard_to_case_and_only_by_prefix_or_exact_name() {
        for name in reserved() {
            assert!(is_account_name(&name), "{name}");
        }
        for name in KEPT {
            assert!(!is_account_name(name), "{name}");
        }
        // The prefixes are whole words: one more or fewer character is not reserved.
        for name in [
            "ORBYN_ACCOUN_X",
            "ORBYN_ACCOUNT",
            "ORBYN_DEVIC_X",
            "ORBYN_KEYSTOR_X",
            "",
        ] {
            assert!(!is_account_name(name), "{name:?}");
        }
    }

    #[test]
    fn the_scrub_leaves_every_existing_rule_as_it_was() {
        // A reserved name beside the host-session and login names: each rule still applies by itself.
        let inherited = vars(&[
            ("CLAUDECODE", "1"),
            ("CLAUDE_CODE_SESSION_ID", "s"),
            ("OPENAI_API_KEY", "k"),
            ("ORBYN_ACCOUNT_PASSWORD", "p"),
            ("HOME", "/h"),
        ]);
        let gone = removed(&plan(&inherited, true, true));
        assert_eq!(
            gone,
            [
                "CLAUDECODE",
                "CLAUDE_CODE_SESSION_ID",
                "OPENAI_API_KEY",
                "ORBYN_ACCOUNT_PASSWORD"
            ]
        );
    }

    /// Set on the helper process only, so the helper test knows it is the one to run.
    const HELPER: &str = "ORBYN_TEST_CHILD_ENV_HELPER";
    const KEEP_NAME: &str = "ORBYN_TEST_KEEP";

    /// A real program that prints its environment, for the platform.
    fn env_printer() -> Command {
        if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/D", "/C", "set"]);
            command
        } else {
            Command::new("/usr/bin/env")
        }
    }

    /// Runs only inside the process the next test starts: it spawns real children through the
    /// gate, as a tool and as an engine, and prints what each of them sees.
    #[test]
    fn helper_prints_what_real_children_inherit() {
        if std::env::var_os(HELPER).is_none() {
            return;
        }
        for (kind, engine) in [("tool", false), ("engine", true)] {
            let mut command = env_printer();
            apply(&mut command, engine);
            let output = command.output().expect("the printer ran");
            assert!(output.status.success());
            println!("BEGIN {kind}");
            println!("{}", String::from_utf8_lossy(&output.stdout));
            println!("END {kind}");
        }
    }

    #[test]
    fn a_real_child_prints_its_environment_and_it_holds_no_account_name_or_value() {
        let secret = canary("account-secret");
        let kept = canary("kept");
        let mut helper = Command::new(std::env::current_exe().unwrap());
        helper
            .args([
                "--exact",
                "host::child_env::tests::helper_prints_what_real_children_inherit",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(HELPER, "1")
            .env(KEEP_NAME, &kept);
        // The helper process starts with every reserved name set, as an Orbyn started from a shell
        // that had them (a person's profile, a CI job) would.
        for name in reserved() {
            helper.env(name, &secret);
        }
        let output = helper.output().expect("the helper ran");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(output.status.success(), "{text}");
        for kind in ["tool", "engine"] {
            let begin = text
                .find(&format!("BEGIN {kind}"))
                .expect("the helper printed");
            let end = text
                .find(&format!("END {kind}"))
                .expect("the helper finished");
            let printed = &text[begin..end];
            // It really is the environment: a variable that must pass is there.
            assert!(
                printed.contains(&format!("{KEEP_NAME}={kept}")),
                "{kind}: {printed}"
            );
            // And nothing of the account is, by name or by value.
            assert!(!printed.contains(&secret), "{kind}: a value leaked");
            for line in printed.lines() {
                let name = line.split('=').next().unwrap_or_default();
                assert!(!is_account_name(name), "{kind}: {name}");
            }
        }
    }

    // ---- the person's login shell knows which install folder comes first ----

    fn widened_entries(path: &[&str], login: &[&str], dirs: &[&str]) -> Vec<PathBuf> {
        let path = (!path.is_empty()).then(|| std::env::join_paths(path.iter().copied()).unwrap());
        let login: Vec<PathBuf> = login.iter().map(|entry| PathBuf::from(*entry)).collect();
        let dirs: Vec<PathBuf> = dirs.iter().map(|entry| PathBuf::from(*entry)).collect();
        std::env::split_paths(&widened_with(path.as_deref(), Some(&login), &dirs)).collect()
    }

    #[cfg(unix)]
    #[test]
    fn login_path_output_is_read_only_between_the_sentinels() {
        let output =
            "rc noise before\n__ORBYN_PATH__/n/bin:/usr/local/bin__ORBYN_PATH__\nrc noise after\n";
        assert_eq!(
            parse_login_path(output),
            Some(vec![
                PathBuf::from("/n/bin"),
                PathBuf::from("/usr/local/bin")
            ])
        );
    }

    #[cfg(unix)]
    #[test]
    fn login_path_output_without_a_sentinel_pair_is_refused() {
        assert_eq!(parse_login_path("/n/bin:/usr/local/bin\n"), None);
        assert_eq!(parse_login_path("__ORBYN_PATH__/n/bin\n"), None);
    }

    #[cfg(unix)]
    #[test]
    fn login_path_output_drops_relative_entries_and_keeps_the_absolute_ones() {
        let output = "__ORBYN_PATH__relative:/n/bin:.:/usr/bin__ORBYN_PATH__";
        assert_eq!(
            parse_login_path(output),
            Some(vec![PathBuf::from("/n/bin"), PathBuf::from("/usr/bin")])
        );
    }

    #[cfg(unix)]
    #[test]
    fn login_path_output_that_leaves_no_absolute_entry_is_refused() {
        assert_eq!(parse_login_path("__ORBYN_PATH____ORBYN_PATH__"), None);
        assert_eq!(parse_login_path("__ORBYN_PATH__:__ORBYN_PATH__"), None);
        assert_eq!(
            parse_login_path("__ORBYN_PATH__relative:.__ORBYN_PATH__"),
            None
        );
    }

    #[test]
    fn login_entries_follow_the_existing_path_and_precede_tool_dirs() {
        let entries = widened_entries(
            &["/first"],
            &["/n/bin", "/usr/bin"],
            &["/opt/homebrew/bin", "/usr/local/bin"],
        );
        assert_eq!(
            entries,
            [
                PathBuf::from("/first"),
                PathBuf::from("/n/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]
        );
    }

    #[test]
    fn a_login_entry_equal_to_a_tool_dir_appears_once_at_the_login_position() {
        let entries = widened_entries(
            &["/first", "/n/bin"],
            &["/n/bin", "/usr/bin"],
            &["/usr/bin", "/opt/homebrew/bin", "/opt/homebrew/bin"],
        );
        assert_eq!(
            entries,
            [
                PathBuf::from("/first"),
                PathBuf::from("/n/bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/opt/homebrew/bin"),
            ]
        );
    }

    #[test]
    fn the_n_node_folder_comes_before_homebrew() {
        let entries = widened_entries(
            &[],
            &["/usr/local/n/versions/node/22.16.0/bin"],
            &["/opt/homebrew/bin"],
        );
        assert_eq!(
            entries,
            [
                PathBuf::from("/usr/local/n/versions/node/22.16.0/bin"),
                PathBuf::from("/opt/homebrew/bin"),
            ]
        );
    }

    #[test]
    fn homebrew_that_follows_n_in_the_login_shell_stays_after_n() {
        let entries = widened_entries(
            &[],
            &[
                "/usr/local/n/versions/node/22.16.0/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin",
            ],
            &["/opt/homebrew/bin", "/usr/local/bin"],
        );
        assert_eq!(
            entries,
            [
                PathBuf::from("/usr/local/n/versions/node/22.16.0/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]
        );
    }

    #[cfg(unix)]
    fn executable_script(dir: &crate::testing::TempDir, name: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.path().join(name);
        std::fs::write(&path, body).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn a_login_shell_that_does_not_finish_within_the_timeout_is_killed() {
        let dir = crate::testing::TempDir::new();
        let shell = executable_script(&dir, "slow-shell", "#!/bin/sh\nsleep 10\n");
        let started = std::time::Instant::now();
        assert_eq!(
            login_shell_path(&shell, std::time::Duration::from_millis(300)),
            None
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_login_shell_path_is_read_from_between_the_sentinels() {
        let dir = crate::testing::TempDir::new();
        let shell = executable_script(
            &dir,
            "path-shell",
            "#!/bin/sh\nprintf 'noise before\\n'\nprintf '__ORBYN_PATH__%s__ORBYN_PATH__' '/n/bin:/usr/local/bin'\nprintf 'noise after\\n'\n",
        );
        assert_eq!(
            login_shell_path(&shell, std::time::Duration::from_secs(3)),
            Some(vec![
                PathBuf::from("/n/bin"),
                PathBuf::from("/usr/local/bin")
            ])
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_login_shell_is_interactive_and_finds_the_n_node_folder() {
        let dir = crate::testing::TempDir::new();
        let shell = executable_script(
            &dir,
            "interactive-shell",
            "#!/bin/sh\ninteractive=0\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \"-i\" ]; then\n    interactive=1\n  fi\ndone\nif [ \"$interactive\" = \"1\" ]; then\n  . \"$(dirname \"$0\")/rc\"\nfi\nprintf '__ORBYN_PATH__%s__ORBYN_PATH__' \"$PATH\"\n",
        );
        std::fs::write(
            dir.path().join("rc"),
            "PATH=/usr/local/n/versions/node/22.16.0/bin:/opt/homebrew/bin:/usr/local/bin\n",
        )
        .unwrap();
        assert_eq!(
            login_shell_path(&shell, std::time::Duration::from_secs(3)),
            Some(vec![
                PathBuf::from("/usr/local/n/versions/node/22.16.0/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ])
        );
        // Without -i the fixture is never read, so the n folder is absent: the
        // interactive flag is what finds it, not the script or the environment.
        let without_interactive = Command::new(&shell)
            .args([
                "-l",
                "-c",
                "printf '__ORBYN_PATH__%s__ORBYN_PATH__' \"$PATH\"",
            ])
            .env_clear()
            .env("PATH", LOGIN_SHELL_PATH)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&without_interactive.stdout);
        assert!(text.contains("__ORBYN_PATH__"), "{text}");
        assert!(
            !text.contains("/usr/local/n/versions/node/22.16.0/bin"),
            "{text}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_login_shell_child_gets_a_cleared_environment() {
        let dir = crate::testing::TempDir::new();
        let secret = format!("/orbyn-login-shell-secret-{}", ulid::Ulid::generate());
        let shell = executable_script(
            &dir,
            "env-shell",
            "#!/bin/sh\nprintf '__ORBYN_PATH__%s__ORBYN_PATH__' \"${ORBYN_SECRET:-unset}:/usr/bin\"\n",
        );
        let previous = std::env::var_os("ORBYN_SECRET");
        // SAFETY: this is the only test that sets or reads ORBYN_SECRET.
        unsafe { std::env::set_var("ORBYN_SECRET", &secret) };
        let path = login_shell_path(&shell, std::time::Duration::from_secs(3));
        match previous {
            Some(value) => unsafe { std::env::set_var("ORBYN_SECRET", value) },
            None => unsafe { std::env::remove_var("ORBYN_SECRET") },
        }
        assert_eq!(path, Some(vec![PathBuf::from("/usr/bin")]), "{path:?}");
    }

    // ---- a Windows terminal never hands PowerShell a cache path the shell can resolve in the
    // checkout (issue 1788). PowerShell writes its module analysis cache to the path in
    // `PSModuleAnalysisCachePath`, and a value that is missing or relative is resolved from the
    // shell's working directory: the project, which `git status` then reports as `?? Microsoft/`.

    #[cfg(windows)]
    const POWERSHELL_CACHE_NAME: &str = "PSModuleAnalysisCachePath";

    /// Where the plan puts the cache file, when it sets one at all.
    #[cfg(windows)]
    fn cache_setting(changes: &EnvPlan) -> Option<PathBuf> {
        changes
            .set
            .iter()
            .find(|(name, _)| name_of(name) == POWERSHELL_CACHE_NAME)
            .map(|(_, value)| PathBuf::from(value))
    }

    /// The variables a Windows terminal starts from, with a profile folder of its own.
    #[cfg(windows)]
    fn windows_vars(
        dir: &crate::testing::TempDir,
        extra: &[(&str, OsString)],
    ) -> Vec<(OsString, OsString)> {
        let mut inherited: Vec<(OsString, OsString)> = vec![
            (
                "LOCALAPPDATA".into(),
                dir.path().join("appdata").into_os_string(),
            ),
            ("TEMP".into(), dir.path().join("temp").into_os_string()),
            ("PATH".into(), "C:\\Windows\\System32".into()),
        ];
        inherited.extend(
            extra
                .iter()
                .map(|(name, value)| ((*name).into(), value.clone())),
        );
        inherited
    }

    #[cfg(windows)]
    #[test]
    fn a_terminal_sends_powershells_module_cache_to_orbyns_data_folder() {
        let dir = crate::testing::TempDir::new();
        let changes = terminal_plan(&windows_vars(&dir, &[]), false, false);
        let cache = cache_setting(&changes).expect("a terminal sets the cache path");
        // Absolute, so no shell can resolve it from its own working directory, and not taken
        // back off the child by any other rule.
        assert!(cache.is_absolute(), "{cache:?}");
        assert_eq!(
            cache,
            dir.path()
                .join("appdata")
                .join("Orbyn")
                .join("powershell")
                .join("ModuleAnalysisCache")
        );
        assert!(!removed(&changes).contains(&POWERSHELL_CACHE_NAME.to_owned()));
    }

    #[cfg(windows)]
    #[test]
    fn a_relative_cache_path_is_replaced_before_the_shell_resolves_it_in_the_checkout() {
        let dir = crate::testing::TempDir::new();
        // What PowerShell builds from a profile folder that comes back empty: the path the gate saw.
        let inherited = windows_vars(
            &dir,
            &[(
                POWERSHELL_CACHE_NAME,
                "Microsoft\\Windows\\PowerShell\\ModuleAnalysisCache".into(),
            )],
        );
        let cache = cache_setting(&terminal_plan(&inherited, false, false))
            .expect("a relative cache path is replaced");
        assert!(cache.is_absolute(), "{cache:?}");
        // The checkout is the shell's working directory; an absolute cache path cannot land in it.
        let checkout = dir.path().join("my-repo");
        assert_eq!(
            checkout.join(&cache),
            cache,
            "{cache:?} resolves inside {checkout:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_persons_own_absolute_cache_path_is_left_alone() {
        let dir = crate::testing::TempDir::new();
        let own = dir.path().join("mine").join("ModuleAnalysisCache");
        let inherited = windows_vars(&dir, &[(POWERSHELL_CACHE_NAME, own.into_os_string())]);
        let changes = terminal_plan(&inherited, false, false);
        assert_eq!(cache_setting(&changes), None);
        assert!(!removed(&changes).contains(&POWERSHELL_CACHE_NAME.to_owned()));
    }

    #[cfg(windows)]
    #[test]
    fn a_profile_folder_that_is_missing_or_relative_falls_back_to_a_temp_folder() {
        let dir = crate::testing::TempDir::new();
        let temp = dir.path().join("temp");
        for app_data in ["", "appdata"] {
            let mut inherited = windows_vars(&dir, &[]);
            for (name, value) in &mut inherited {
                if name.as_os_str() == "LOCALAPPDATA" {
                    *value = app_data.into();
                }
            }
            let cache = cache_setting(&terminal_plan(&inherited, false, false))
                .unwrap_or_else(|| panic!("{app_data:?}: no cache path"));
            assert!(cache.is_absolute(), "{app_data:?}: {cache:?}");
            assert!(cache.starts_with(&temp), "{app_data:?}: {cache:?}");
        }
        // A relative data folder is no better than no folder: the shell would resolve it too.
        let inherited = windows_vars(&dir, &[("ORBYN_DATA_DIR", "relative-data".into())]);
        let cache = cache_setting(&terminal_plan(&inherited, false, false)).expect("still set");
        assert!(cache.is_absolute(), "relative data folder: {cache:?}");
        // And with no folder named at all, the system temp folder still keeps it absolute.
        let bare: Vec<(OsString, OsString)> = vec![("PATH".into(), "C:\\Windows".into())];
        let cache = cache_setting(&terminal_plan(&bare, false, false)).expect("still set");
        assert!(cache.is_absolute(), "no folder: {cache:?}");
    }

    #[cfg(windows)]
    #[test]
    fn a_tool_and_an_engine_keep_their_own_cache_setting() {
        let dir = crate::testing::TempDir::new();
        let inherited = windows_vars(&dir, &[]);
        for engine in [false, true] {
            assert_eq!(cache_setting(&plan(&inherited, engine, false)), None);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn a_terminal_on_another_platform_gets_only_the_widened_path() {
        let changes = terminal_plan(&vars(&[("PATH", "/bin")]), false, false);
        let set: Vec<String> = changes.set.iter().map(|(name, _)| name_of(name)).collect();
        assert_eq!(set, ["PATH"]);
    }
}
