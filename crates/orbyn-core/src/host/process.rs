//! Child processes as owned resources. Each engine runs in its own process tree
//! and the tree dies with its owner on every exit path. Unix uses a process
//! group plus a walk of the descendants, which also reaches a child that left the
//! group with `setsid`; Linux also makes the leader a subreaper, so a double fork
//! whose middle process exits is adopted back under the leader instead of `init`
//! and the same walk finds it. Windows uses a job object, which holds every
//! descendant (no breakaway is allowed), with `taskkill /T` as the fallback.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::{Child, Command};

use crate::error::{CoreError, Result};

#[cfg(all(test, unix))]
#[path = "process/unix_tests.rs"]
mod unix_tests;
#[cfg(unix)]
mod unix_tree;
#[cfg(all(test, windows))]
#[path = "process/windows_tests.rs"]
mod windows_tests;

/// A program ready to spawn. Windows shims resolve to their real entry point.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedCommand {
    pub program: PathBuf,
    pub prefix: Vec<OsString>,
    /// The file found on PATH, before following any shim.
    pub path: PathBuf,
}

/// Resolves a bare name through PATH, or an explicit path, before a child changes cwd.
pub fn resolve(name: &str) -> Option<ResolvedCommand> {
    let search = crate::host::child_env::widened(std::env::var_os("PATH").as_deref());
    let path = find_on_path(name, Some(&search))?;
    shim_target(&path)
}

fn find_on_path(name: &str, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let direct = Path::new(name);
    let path = if direct.components().count() > 1 || direct.is_absolute() {
        candidates(direct).into_iter().find(|path| path.is_file())
    } else {
        std::env::split_paths(path_var?)
            .flat_map(|dir| candidates(&dir.join(name)))
            .find(|path| is_runnable(path))
    }?;
    // Keep the shim's location (including symlinks), but not its dependency on cwd.
    std::path::absolute(path).ok()
}

#[cfg(unix)]
fn candidates(base: &Path) -> Vec<PathBuf> {
    vec![base.to_path_buf()]
}

#[cfg(windows)]
fn candidates(base: &Path) -> Vec<PathBuf> {
    if base.extension().is_some() {
        return vec![base.to_path_buf()];
    }
    let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
    pathext
        .split(';')
        .filter(|ext| !ext.is_empty())
        .chain([".ps1"])
        .map(|ext| {
            let mut name = base.as_os_str().to_owned();
            name.push(ext);
            PathBuf::from(name)
        })
        .collect()
}

#[cfg(unix)]
fn is_runnable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_runnable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(unix)]
fn shim_target(path: &Path) -> Option<ResolvedCommand> {
    Some(ResolvedCommand {
        program: path.to_path_buf(),
        prefix: Vec::new(),
        path: path.to_path_buf(),
    })
}

/// Windows cannot `CreateProcess` a script. An npm `.cmd` shim is followed to
/// the script it wraps, so no argument ever passes through `cmd.exe` parsing;
/// a `.cmd` that cannot be followed is refused rather than quoted by hand.
#[cfg(windows)]
fn shim_target(path: &Path) -> Option<ResolvedCommand> {
    let extension = path
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    match extension.as_deref() {
        Some("cmd" | "bat") => {
            let text = std::fs::read_to_string(path).ok()?;
            let script = match parse_cmd_shim(&text) {
                Some(script) => path.parent()?.join(script),
                None => sibling_script(path)?,
            };
            if script
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
            {
                return Some(ResolvedCommand {
                    program: script,
                    prefix: Vec::new(),
                    path: path.to_path_buf(),
                });
            }
            let beside = path.parent()?.join("node.exe");
            let node = if beside.is_file() {
                beside
            } else {
                let search = crate::host::child_env::widened(std::env::var_os("PATH").as_deref());
                find_on_path("node", Some(&search))?
            };
            Some(ResolvedCommand {
                program: node,
                prefix: vec![script.into_os_string()],
                path: path.to_path_buf(),
            })
        }
        Some("ps1") => Some(ResolvedCommand {
            program: PathBuf::from("powershell.exe"),
            prefix: [
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ]
            .iter()
            .map(OsString::from)
            .chain([path.as_os_str().to_owned()])
            .collect(),
            path: path.to_path_buf(),
        }),
        _ => Some(ResolvedCommand {
            program: path.to_path_buf(),
            prefix: Vec::new(),
            path: path.to_path_buf(),
        }),
    }
}

/// A standalone installer can put its launcher beside the `.cmd` under the same name.
#[cfg(windows)]
fn sibling_script(shim: &Path) -> Option<PathBuf> {
    ["js", "cjs", "mjs"]
        .iter()
        .map(|extension| shim.with_extension(extension))
        .find(|script| script.is_file())
}

/// A shim that hands off to a shell would put arguments back through its parser,
/// which is what following a shim exists to avoid.
fn is_shell_host(file: &str) -> bool {
    let name = file.rsplit(['\\', '/']).next().unwrap_or(file);
    let name = name.to_ascii_lowercase();
    let Some(stem) = name.strip_suffix(".exe") else {
        return false;
    };
    [
        "cmd",
        "powershell",
        "pwsh",
        "wscript",
        "cscript",
        "bash",
        "sh",
        "mshta",
    ]
    .contains(&stem)
}

/// Whether a shim path is a payload this resolver may start itself: a script or
/// program, never `node.exe` (the host it runs) or a shell that would parse the
/// arguments again.
fn is_launchable(file: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    [".js", ".cjs", ".mjs", ".exe"]
        .iter()
        .any(|ext| lower.ends_with(ext))
        && !lower.ends_with("node.exe")
        && !is_shell_host(&lower)
}

/// Finds the wrapped script in an npm-style `.cmd` shim. npm writes the path two
/// ways: quoted after `%dp0%\` or `%~dp0\` (pnpm's shape too, climbing into its
/// store), or into a variable first as `SET "<NAME>=%~dp0<path>"` and launched
/// as `"%NAME%"`. The result is relative to the shim's folder.
pub fn parse_cmd_shim(text: &str) -> Option<String> {
    for marker in ["\"%dp0%\\", "\"%~dp0\\", "\"%dp0%/", "\"%~dp0/"] {
        for (start, _) in text.match_indices(marker) {
            let rest = &text[start + marker.len()..];
            let end = rest.find('"')?;
            let candidate = &rest[..end];
            if is_launchable(candidate) {
                return Some(candidate.replace('\\', std::path::MAIN_SEPARATOR_STR));
            }
        }
    }
    assigned_script(text)
}

/// What a shim set a variable that a later command line expands to.
#[derive(Clone, Debug, PartialEq, Eq)]
enum AssignedValue {
    /// A local script or program this resolver may start itself.
    Script(String),
    /// The Node host that runs the payload argument, `node.exe` or `node`.
    Host,
    /// A value this resolver does not follow: a shell, an unsupported file, a
    /// path outside the shim's folder, or a name left ambiguous by a conditional
    /// assignment. A launch that depends on it is refused.
    Unfollowable,
}

/// The script a shim sets in a variable before launching it. npm's own npx.cmd
/// and npm.cmd write `SET "NODE_EXE=%~dp0\node.exe"` and a prefix helper before the
/// CLI, then launch the CLI in the file's last command line.
///
/// A `SET` outside an `IF`/`FOR` block replaces the variable's earlier value,
/// even when the new value is not launchable: a shim that supersedes `old.js`
/// with an unsupported path is refused rather than followed to the stale script.
/// A `SET` inside a block may not run, so the name keeps its earlier value only
/// while the conditional value would launch the same thing. A conditional
/// assignment naming a different payload, or the Node host in place of one,
/// leaves the name ambiguous and a launch of it is refused; a value this
/// resolver cannot follow at all does not replace the file's own payload (npm's
/// conditional prefix fallback relies on that). The launch is the last command
/// line that expands a variable the file set; its first reference names the
/// program, so a helper launched before the payload cannot win by assignment
/// order.
fn assigned_script(text: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut assigned: Vec<(String, AssignedValue)> = Vec::new();
    let mut launched: Option<Option<String>> = None;
    for line in text.lines() {
        let line = without_echo_prefix(line);
        if let Some((name, value)) = set_assignment(line) {
            apply_assignment(&mut assigned, name, value, depth > 0);
        } else {
            // The line loop reads a `SET` that starts its line; one written after
            // the guard's opening parenthesis is read here as conditional.
            for (command, conditional) in line_commands(line, depth) {
                if conditional && let Some((name, value)) = set_assignment(command) {
                    apply_assignment(&mut assigned, name, value, true);
                }
            }
            if let Some(target) = launch_target(line, &assigned) {
                launched = Some(target);
            }
        }
        depth = block_depth(depth, line);
    }
    launched?
}

/// Records what a `SET` assignment means for a later launch.
fn apply_assignment(
    assigned: &mut Vec<(String, AssignedValue)>,
    name: &str,
    value: &str,
    conditional: bool,
) {
    let name = name.to_ascii_lowercase();
    let value = assigned_value(value);
    match assigned.iter_mut().find(|(known, _)| *known == name) {
        Some((_, current)) if !conditional => *current = value,
        Some((_, current)) if followable(&value) && value != *current => {
            *current = AssignedValue::Unfollowable;
        }
        Some(_) => {}
        None if !conditional => assigned.push((name, value)),
        None => {}
    }
}

/// What a `SET` value means for a launch that expands it.
fn assigned_value(value: &str) -> AssignedValue {
    if let Some(script) = dp0_script(value) {
        return AssignedValue::Script(script);
    }
    if is_node_host(value) {
        return AssignedValue::Host;
    }
    AssignedValue::Unfollowable
}

/// Whether a `SET` value names the Node host, which runs the next argument
/// instead of being a payload itself.
fn is_node_host(value: &str) -> bool {
    let name = value.rsplit(['\\', '/']).next().unwrap_or(value);
    name.eq_ignore_ascii_case("node") || name.eq_ignore_ascii_case("node.exe")
}

/// Whether a `SET` value names something a launch can be followed through. A
/// value this resolver cannot follow is never allowed to replace one it knows.
fn followable(value: &AssignedValue) -> bool {
    !matches!(value, AssignedValue::Unfollowable)
}

/// The script a command line starts, when the line names a program through
/// variables the file set. `None` when the line starts no such command,
/// `Some(None)` when the command is ambiguous or names a value this resolver
/// does not follow, so the shim is refused rather than guessed.
fn launch_target(line: &str, assigned: &[(String, AssignedValue)]) -> Option<Option<String>> {
    if !starts_command(line) {
        return None;
    }
    let mut references = cmd_tokens(line)
        .into_iter()
        .filter_map(variable_name)
        .filter_map(|name| {
            assigned
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
        });
    // The command's executable is its first reference. The Node host is the
    // executable only when the next reference is the payload it runs, so an
    // executable-valued argument after the program is not the program.
    match references.next()? {
        AssignedValue::Script(script) => Some(Some(script.clone())),
        AssignedValue::Host => match references.next() {
            Some(AssignedValue::Script(script)) => Some(Some(script.clone())),
            _ => Some(None),
        },
        AssignedValue::Unfollowable => Some(None),
    }
}

/// The commands a line runs in order, with whether an `IF`/`FOR` block guards
/// each one. The first command keeps the line's depth; one after an opening
/// parenthesis on the same line is conditional, as in
/// `IF <condition> (SET "NAME=value")`.
fn line_commands(line: &str, depth: usize) -> Vec<(&str, bool)> {
    let mut depth = depth;
    let mut commands = Vec::new();
    let mut start = 0usize;
    let mut quoted = false;
    for (index, character) in line.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => {
                push_command(&mut commands, &line[start..index], depth);
                depth += 1;
                start = index + 1;
            }
            ')' if !quoted => {
                push_command(&mut commands, &line[start..index], depth);
                depth = depth.saturating_sub(1);
                start = index + 1;
            }
            _ => {}
        }
    }
    push_command(&mut commands, &line[start..], depth);
    commands
}

fn push_command<'a>(commands: &mut Vec<(&'a str, bool)>, command: &'a str, depth: usize) {
    if !command.trim().is_empty() {
        commands.push((command, depth > 0));
    }
}

/// A command line without its leading echo-suppression prefix. `@` only hides
/// the command from the echo stream; it changes nothing about the program, so
/// classification and tokenization both see the bare line.
fn without_echo_prefix(line: &str) -> &str {
    line.trim_start().trim_start_matches('@').trim_start()
}

/// Whether a line starts a program rather than a batch keyword, a label or the
/// closing line of a block. `SET` lines are handled before this is called, and
/// the line arrives without its echo-suppression prefix.
fn starts_command(line: &str) -> bool {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with(':') || line.starts_with(')') {
        return false;
    }
    let word = line.split_ascii_whitespace().next().unwrap_or_default();
    if word.starts_with('"') || word.starts_with('%') {
        return true;
    }
    !matches!(
        word.to_ascii_lowercase().as_str(),
        "if" | "for"
            | "set"
            | "setlocal"
            | "endlocal"
            | "echo"
            | "rem"
            | "goto"
            | "exit"
            | "pause"
            | "title"
            | "cls"
            | "color"
            | "cd"
            | "chdir"
            | "pushd"
            | "popd"
            | "shift"
            | "verify"
            | "del"
            | "erase"
            | "rd"
            | "rmdir"
            | "md"
            | "mkdir"
            | "copy"
            | "xcopy"
            | "move"
            | "ren"
            | "rename"
            | "type"
            | "dir"
            | "break"
    )
}

/// The words of a command line, quotes kept. Batch operators (`&`, `|`, `<`, `>`,
/// parentheses) separate words too, so a variable matches only as a whole word
/// and never inside a larger argument.
fn cmd_tokens(line: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    let mut quoted = false;
    for (index, character) in line.char_indices() {
        if character == '"' {
            quoted = !quoted;
            start.get_or_insert(index);
        } else if !quoted && (character.is_ascii_whitespace() || "&|<>()".contains(character)) {
            if let Some(begin) = start.take() {
                tokens.push(&line[begin..index]);
            }
        } else {
            start.get_or_insert(index);
        }
    }
    if let Some(begin) = start {
        tokens.push(&line[begin..]);
    }
    tokens
}

/// The variable a word expands, from `%NAME%` or `"%NAME%"`; a path or a partial
/// expansion is not a variable reference.
fn variable_name(word: &str) -> Option<&str> {
    let name = word
        .trim_matches('"')
        .strip_prefix('%')?
        .strip_suffix('%')?;
    (!name.is_empty() && !name.contains('%')).then_some(name)
}

/// The `IF`/`FOR` block depth after a line, counting parentheses outside quotes.
/// A `SET` inside a block is conditional and may not run.
fn block_depth(depth: usize, line: &str) -> usize {
    let mut depth = depth;
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// The name and value of a `SET` line, written `SET "NAME=value"` or
/// `SET NAME=value`. Every other `SET` form (`/A`, `/P`) names no variable.
fn set_assignment(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    if !line.get(..3)?.eq_ignore_ascii_case("set") {
        return None;
    }
    let rest = line[3..].trim_start();
    let assignment = match rest.strip_prefix('"') {
        Some(quoted) => &quoted[..quoted.rfind('"')?],
        None => rest.trim_end(),
    };
    let (name, value) = assignment.split_once('=')?;
    let plain = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    (plain && !value.contains('"')).then_some((name, value))
}

/// The path a `SET` value names inside the shim's folder, from `%~dp0` (or
/// `%dp0%`) and a relative path to a launchable file. A drive or UNC path here
/// would leave the folder, so it is refused rather than followed by accident.
fn dp0_script(value: &str) -> Option<String> {
    let rest = value
        .strip_prefix("%~dp0")
        .or_else(|| value.strip_prefix("%dp0%"))?;
    let relative = rest
        .strip_prefix('\\')
        .or_else(|| rest.strip_prefix('/'))
        .unwrap_or(rest);
    let drive = relative
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphabetic)
        && relative.as_bytes().get(1) == Some(&b':');
    let rooted = matches!(relative.chars().next(), Some('\\' | '/')) || drive;
    let relative = relative.replace('\\', std::path::MAIN_SEPARATOR_STR);
    (!rooted && is_launchable(&relative)).then_some(relative)
}

/// Builds a piped command in its own process tree.
pub fn engine_command(command: &ResolvedCommand, args: &[OsString], cwd: &Path) -> Command {
    let mut builder = Command::new(&command.program);
    builder
        .args(&command.prefix)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    crate::host::child_env::for_engine(builder.as_std_mut());
    builder
}

/// Its own process group, which the group kill addresses. Linux additionally makes the leader the subreaper of its own descendants: a
/// double fork whose middle process exits orphans the grandchild before any walk can see the parent link, and the kernel reparents it to
/// the nearest subreaper. The leader is that subreaper, so the link survives and the walk below finds it. macOS has no equivalent flag;
/// there the orphan is out of reach, and Windows holds it in the job object.
#[cfg(target_os = "linux")]
fn isolate(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
    // SAFETY: `pre_exec` runs in the child between `fork` and `exec`, where the closure calls one syscall and allocates nothing.
    unsafe {
        command.as_std_mut().pre_exec(subreaper);
    }
}

#[cfg(all(unix, not(target_os = "linux")))]
fn isolate(command: &mut Command) {
    command.process_group(0);
}

/// `PR_SET_CHILD_SUBREAPER` for the leader. Best effort: a kernel without the flag keeps the process-group behaviour rather than
/// refusing to start the program. Runs between `fork` and `exec`, so it stays async-signal-safe.
#[cfg(target_os = "linux")]
fn subreaper() -> std::io::Result<()> {
    // SAFETY: `prctl` takes plain integers and touches no memory of the caller.
    unsafe {
        libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0);
    }
    Ok(())
}

#[cfg(windows)]
fn isolate(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

/// Owns a spawned child. Dropping it kills the whole tree.
pub struct ManagedChild {
    child: Child,
    tree: ProcessTree,
}

impl ManagedChild {
    pub fn spawn(mut command: Command) -> Result<Self> {
        isolate(&mut command);
        let child = command.spawn()?;
        let pid = child
            .id()
            .ok_or_else(|| CoreError::Internal("child exited before it had a pid".into()))?;
        let tree = ProcessTree::attach(&child, pid);
        Ok(Self { child, tree })
    }

    pub fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Asks the tree to stop. Windows has no such signal, so callers use the
    /// engine's own protocol first and this is a forced stop there.
    pub fn terminate(&self) {
        self.tree.terminate_owned(self.child.id().is_some());
    }

    pub async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.wait().await
    }

    /// [`Self::wait`], and every `every` while it runs the descendants are recorded, so one that outlives a leader which exits
    /// (a `setsid` child left running) is still reaped when this owner is dropped. A leader that exits between two records, or before
    /// the first, leaves its descendants out of reach.
    pub async fn wait_tracking(
        &mut self,
        every: std::time::Duration,
    ) -> std::io::Result<std::process::ExitStatus> {
        loop {
            tokio::select! {
                status = self.child.wait() => return status,
                () = tokio::time::sleep(every) => self.tree.track_owned(self.child.id().is_some()),
            }
        }
    }

    /// [`Self::output`] with a bound on each stream. A stream over its bound ends
    /// the whole tree and returns [`CappedOutputError::TooLarge`], so a tool cannot
    /// make the service buffer without limit. Cancellation drops this owner too.
    pub async fn output_capped(
        mut self,
        stdout_limit: usize,
        stderr_limit: usize,
    ) -> std::result::Result<CappedOutput, CappedOutputError> {
        let stdout = self.child.stdout.take();
        let stderr = self.child.stderr.take();
        drop(self.child.stdin.take());
        let wait = async {
            let status = self.child.wait().await.map_err(CappedOutputError::Io)?;
            // A descendant may retain an output pipe after the leader exits. End
            // the owned tree now so collecting that pipe cannot wait forever.
            self.tree.kill_owned(self.child.id().is_some());
            Ok::<_, CappedOutputError>(status)
        };
        let (status, stdout, stderr) = tokio::try_join!(
            wait,
            read_capped(stdout, stdout_limit),
            read_capped(stderr, stderr_limit)
        )?;
        Ok(CappedOutput {
            status,
            stdout,
            stderr,
        })
    }

    /// Captures a helper's output without surrendering its process-tree owner.
    /// Cancellation drops this owner and stops descendants as well as the leader.
    pub async fn output(mut self) -> Result<std::process::Output> {
        use tokio::io::AsyncReadExt;

        async fn read(
            stream: Option<impl tokio::io::AsyncRead + Unpin>,
        ) -> std::io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            if let Some(mut stream) = stream {
                stream.read_to_end(&mut bytes).await?;
            }
            Ok(bytes)
        }

        let stdout = self.child.stdout.take();
        let stderr = self.child.stderr.take();
        drop(self.child.stdin.take());
        let wait = async {
            let status = self.child.wait().await?;
            // A descendant may retain an output pipe after the leader exits. End
            // the owned tree now so collecting that pipe cannot wait on it forever.
            self.tree.kill_owned(self.child.id().is_some());
            Ok::<_, std::io::Error>(status)
        };
        let (status, stdout, stderr) = tokio::try_join!(wait, read(stdout), read(stderr))?;
        Ok(std::process::Output {
            status,
            stdout,
            stderr,
        })
    }
}

/// What one capped collection produced.
pub struct CappedOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Why a capped collection ended before the tool did.
#[derive(Debug)]
pub enum CappedOutputError {
    /// One stream passed its limit. The tree was ended.
    TooLarge,
    Io(std::io::Error),
}

/// Reads at most `limit` bytes, then stops the child by returning an error: the
/// owner drops every future here, and [`ManagedChild`]'s `Drop` ends the tree.
async fn read_capped<R: tokio::io::AsyncRead + Unpin>(
    stream: Option<R>,
    limit: usize,
) -> std::result::Result<Vec<u8>, CappedOutputError> {
    use tokio::io::AsyncReadExt;

    let mut bytes = Vec::new();
    let Some(mut stream) = stream else {
        return Ok(bytes);
    };
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream
            .read(&mut buffer)
            .await
            .map_err(CappedOutputError::Io)?;
        if read == 0 {
            return Ok(bytes);
        }
        if bytes.len().saturating_add(read) > limit {
            return Err(CappedOutputError::TooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        // `id()` is `None` once the leader was waited for: its pid may be reused, so then nothing is walked.
        self.tree.kill_owned(self.child.id().is_some());
    }
}

/// One process group. [`ProcessTree::terminate`] and [`ProcessTree::kill`] signal that group only, which is what a person's terminal
/// tab needs (a `nohup make &` or a disowned job outlives the tab, as it always did). A [`ManagedChild`] also reaps descendants that
/// left the group (`unix_tree`), through the owned variants below.
#[cfg(unix)]
pub struct ProcessTree {
    leader: i32,
    seen: std::sync::Mutex<std::collections::HashSet<unix_tree::Member>>,
}

#[cfg(unix)]
impl ProcessTree {
    fn attach(_child: &Child, pid: u32) -> Self {
        Self::from_group(pid as i32)
    }

    /// `leader` must lead its own process group.
    pub fn from_group(leader: i32) -> Self {
        Self {
            leader,
            seen: std::sync::Mutex::new(std::collections::HashSet::new()),
        }
    }

    /// SIGTERM to the group only.
    pub fn terminate(&self) {
        self.signal(libc::SIGTERM);
    }

    /// SIGKILL to the group only.
    pub fn kill(&self) {
        self.signal(libc::SIGKILL);
    }

    /// SIGTERM to the group and to the descendants found now, which are recorded so a later [`Self::kill_owned`] still finishes them
    /// after the leader is gone. `leader_unreaped` is whether the leader has not been waited for: only then is the pid certainly its own.
    fn terminate_owned(&self, leader_unreaped: bool) {
        if self.leader <= 1 {
            return;
        }
        if leader_unreaped {
            let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            unix_tree::terminate(self.leader, &mut seen);
        }
        self.signal(libc::SIGTERM);
    }

    /// Records the descendants found now (no signal), for a kill that comes after the leader has exited.
    fn track_owned(&self, leader_unreaped: bool) {
        if leader_unreaped && self.leader > 1 {
            let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
            unix_tree::track(self.leader, &mut seen);
        }
    }

    /// Kills the group, and every descendant ever recorded or found now (while the leader is unreaped), `setsid` children included.
    fn kill_owned(&self, leader_unreaped: bool) {
        if self.leader <= 1 {
            return;
        }
        let mut seen = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        unix_tree::kill(self.leader, leader_unreaped, &mut seen);
    }

    fn signal(&self, signal: i32) {
        if self.leader <= 1 {
            return;
        }
        // SAFETY: `kill` takes plain integers and touches no memory. A negative
        // pid addresses the group this type owns; ESRCH after exit is harmless.
        unsafe {
            libc::kill(-self.leader, signal);
        }
    }
}

#[cfg(windows)]
pub struct ProcessTree {
    pid: u32,
    job: Option<windows_job::Job>,
}

#[cfg(windows)]
impl ProcessTree {
    fn attach(child: &Child, pid: u32) -> Self {
        let job = child.raw_handle().and_then(windows_job::Job::assign);
        Self { pid, job }
    }

    pub fn from_pid(pid: u32) -> Self {
        Self { pid, job: None }
    }

    fn terminate_owned(&self, _leader_unreaped: bool) {
        self.terminate();
    }

    fn track_owned(&self, _leader_unreaped: bool) {}

    fn kill_owned(&self, _leader_unreaped: bool) {
        self.kill();
    }

    pub fn terminate(&self) {
        self.kill();
    }

    pub fn kill(&self) {
        if let Some(job) = &self.job {
            job.terminate();
            return;
        }
        let mut command = std::process::Command::new("taskkill");
        command
            .args(["/PID", &self.pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // A short-lived helper, but every spawn goes through the one gate: it never sees account secrets.
        crate::host::child_env::for_tool(&mut command);
        let _ = command.status();
    }
}

#[cfg(windows)]
mod windows_job {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::ptr::null;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };

    /// A kill-on-close job that contains one engine's process tree.
    pub struct Job(HANDLE);

    // SAFETY: a job handle is a process-wide kernel handle with no thread
    // affinity, and it is closed exactly once in `Drop`.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        pub fn assign(process: std::os::windows::io::RawHandle) -> Option<Self> {
            // SAFETY: all pointers are valid for the duration of each call,
            // `info` is the structure the information class names, and
            // `process` is a live handle owned by the caller's `Child`.
            unsafe {
                let job = CreateJobObjectW(null(), null());
                if job.is_null() {
                    return None;
                }
                let guard = Self(job);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let limited = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&raw const info).cast::<c_void>(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if limited == 0 || AssignProcessToJobObject(job, process.cast()) == 0 {
                    return None;
                }
                Some(guard)
            }
        }

        pub fn terminate(&self) {
            // SAFETY: the handle stays open for the life of `self`.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: this type owns the handle and closes it once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pnpm_shim_is_followed_into_its_store() {
        let shim = "@SETLOCAL\r\n\"%~dp0\\node.exe\" \"%~dp0\\..\\.pnpm\\tool@1\\node_modules\\tool\\cli.mjs\" %*\r\n";
        let script = parse_cmd_shim(shim).unwrap();
        assert!(script.ends_with("cli.mjs"), "{script}");
    }

    #[test]
    fn a_shim_that_hands_off_to_a_shell_is_refused() {
        for shim in [
            "\"%dp0%\\cmd.exe\" /c evil %*",
            "\"%~dp0\\powershell.exe\" -File x %*",
            "\"%dp0%\\sub\\PWSH.EXE\" %*",
        ] {
            assert_eq!(parse_cmd_shim(shim), None, "{shim}");
        }
    }

    #[test]
    fn follows_npm_cmd_shim_to_its_script() {
        let shim = "@ECHO off\r\nSETLOCAL\r\nSET dp0=%~dp0\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n)\r\nendLocal & goto #_undefined_# 2>NUL || title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\r\n";
        let expected = ["node_modules", "@openai", "codex", "bin", "codex.js"]
            .join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(parse_cmd_shim(shim), Some(expected));
    }

    /// npm 10's own npx.cmd: the node host and a prefix helper are set first,
    /// the CLI last, and only the CLI is launched at the end.
    #[test]
    fn follows_npm_set_shim_to_its_cli_script() {
        let shim = concat!(
            "@ECHO OFF\r\n\r\nSETLOCAL\r\n\r\n",
            "SET \"NODE_EXE=%~dp0\\node.exe\"\r\n",
            "IF NOT EXIST \"%NODE_EXE%\" (\r\n  SET \"NODE_EXE=node\"\r\n)\r\n\r\n",
            "SET \"NPM_PREFIX_JS=%~dp0\\node_modules\\npm\\bin\\npm-prefix.js\"\r\n",
            "SET \"NPX_CLI_JS=%~dp0\\node_modules\\npm\\bin\\npx-cli.js\"\r\n",
            "FOR /F \"delims=\" %%F IN ('CALL \"%NODE_EXE%\" \"%NPM_PREFIX_JS%\"') DO (\r\n",
            "  SET \"NPM_PREFIX_NPX_CLI_JS=%%F\\node_modules\\npm\\bin\\npx-cli.js\"\r\n)\r\n",
            "IF EXIST \"%NPM_PREFIX_NPX_CLI_JS%\" (\r\n  SET \"NPX_CLI_JS=%NPM_PREFIX_NPX_CLI_JS%\"\r\n)\r\n\r\n",
            "\"%NODE_EXE%\" \"%NPX_CLI_JS%\" %*\r\n",
        );
        let expected =
            ["node_modules", "npm", "bin", "npx-cli.js"].join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(parse_cmd_shim(shim), Some(expected));
    }

    /// A later top-level `SET` replaces the earlier value, so a superseded script
    /// is never launched: replacing a launchable script with an unsupported path
    /// is refused, and replacing it with another script follows the new one.
    #[test]
    fn an_unconditional_reassignment_supersedes_the_earlier_script() {
        let shim = concat!(
            "SET \"TOOL=%~dp0\\old.js\"\r\n",
            "SET \"TOOL=%~dp0\\replacement.ps1\"\r\n",
            "\"%TOOL%\" %*\r\n",
        );
        assert_eq!(parse_cmd_shim(shim), None);
        let shim = concat!(
            "SET \"TOOL=%~dp0\\old.js\"\r\n",
            "SET \"TOOL=%~dp0\\new.js\"\r\n",
            "\"%TOOL%\" %*\r\n",
        );
        assert_eq!(parse_cmd_shim(shim), Some("new.js".to_owned()));
    }

    /// The last command line decides what runs: setting the CLI first and a helper
    /// after it cannot make the helper the payload when the file launches the
    /// helper first and the CLI last.
    #[test]
    fn the_final_launch_selects_the_script_not_the_last_assignment() {
        let shim = concat!(
            "SET \"CLI=%~dp0\\lib\\cli.js\"\r\n",
            "SET \"HELPER=%~dp0\\lib\\helper.js\"\r\n",
            "\"%HELPER%\" prepare\r\n",
            "\"%CLI%\" %*\r\n",
        );
        let expected = ["lib", "cli.js"].join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(parse_cmd_shim(shim), Some(expected));
    }

    /// A conditional `SET` may run, so assigning a different payload to a name the
    /// file already set leaves a later launch of it ambiguous: the shim is refused
    /// instead of following the earlier payload, in a block or on the guard's line.
    #[test]
    fn a_conditional_reassignment_makes_the_launch_ambiguous() {
        for guarded in [
            "IF 1==1 (\r\n  SET \"TOOL=%~dp0\\new.exe\"\r\n)\r\n",
            "IF 1==1 (SET \"TOOL=%~dp0\\new.exe\")\r\n",
        ] {
            let shim = format!("SET \"TOOL=%~dp0\\old.exe\"\r\n{guarded}\"%TOOL%\" %*\r\n");
            assert_eq!(parse_cmd_shim(&shim), None, "{shim}");
        }
        // Reassigning the same payload conditionally changes nothing.
        let shim = concat!(
            "SET \"TOOL=%~dp0\\tool.exe\"\r\n",
            "IF 1==1 (SET \"TOOL=%~dp0\\tool.exe\")\r\n",
            "\"%TOOL%\" %*\r\n",
        );
        assert_eq!(parse_cmd_shim(shim), Some("tool.exe".to_owned()));
    }

    /// The launched program is the command's executable, not its last reference:
    /// an executable-valued argument is never followed as the payload.
    #[test]
    fn an_executable_argument_is_not_the_launched_script() {
        let shim = concat!(
            "SET \"CLI=%~dp0\\tool.exe\"\r\n",
            "SET \"INPUT=%~dp0\\fixtures\\sample.exe\"\r\n",
            "\"%CLI%\" \"%INPUT%\" %*\r\n",
        );
        assert_eq!(parse_cmd_shim(shim), Some("tool.exe".to_owned()));
        // npm launches the Node host with the script as its first argument: the
        // host is not the payload, and a later executable argument is not either.
        let shim = concat!(
            "SET \"NODE_EXE=%~dp0\\node.exe\"\r\n",
            "SET \"CLI_JS=%~dp0\\lib\\cli.js\"\r\n",
            "SET \"INPUT=%~dp0\\fixtures\\sample.exe\"\r\n",
            "\"%NODE_EXE%\" \"%CLI_JS%\" \"%INPUT%\" %*\r\n",
        );
        let expected = ["lib", "cli.js"].join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(parse_cmd_shim(shim), Some(expected));
    }

    /// A leading `@` only suppresses echo; the command it prefixes is still the
    /// `SET` and the launch the resolver follows.
    #[test]
    fn an_echo_suppressed_command_is_followed() {
        let shim = concat!(
            "@SET \"TOOL=%~dp0\\bin\\tool.exe\"\r\n",
            "@ \"%TOOL%\" %*\r\n",
        );
        let expected = ["bin", "tool.exe"].join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(parse_cmd_shim(shim), Some(expected));
    }

    #[test]
    fn follows_a_generic_set_shim_to_its_script() {
        let shim = "SET \"TOOL_JS=%~dp0\\lib\\tool.js\"\r\n\"%NODE_EXE%\" \"%TOOL_JS%\" %*\r\n";
        let expected = ["lib", "tool.js"].join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(parse_cmd_shim(shim), Some(expected));
        // A value the file does not expand by name is still the script it sets.
        let shim = "set TOOL=%~dp0\\bin\\tool.mjs\r\nnode %TOOL% %*\r\n";
        let expected = ["bin", "tool.mjs"].join(std::path::MAIN_SEPARATOR_STR);
        assert_eq!(parse_cmd_shim(shim), Some(expected));
    }

    #[test]
    fn a_set_shim_with_no_launchable_script_is_refused() {
        for shim in [
            "SET \"NODE_EXE=%~dp0\\node.exe\"\r\n\"%NODE_EXE%\" %*",
            "SET \"TOOL=%~dp0\\cmd.exe\"\r\n\"%TOOL%\" /c evil %*",
            "SET \"TOOL=%~dp0\\sub\\PWSH.EXE\"\r\n\"%TOOL%\" %*",
            "SET \"TOOL=%~dp0\\..\\..\\cmd.exe\"\r\n\"%TOOL%\" /c evil",
            "SET \"TOOL=C:\\Windows\\notepad.exe\"\r\n\"%TOOL%\"",
            "SET \"TOOL=%~dp0D:\\evil.exe\"\r\n\"%TOOL%\"",
            "SET \"TOOL=%~dp0\\\\server\\share\\evil.exe\"\r\n\"%TOOL%\"",
            "SET \"TOOL=%~dp0\\evil.dll\"\r\n\"%TOOL%\"",
        ] {
            assert_eq!(parse_cmd_shim(shim), None, "{shim}");
        }
    }

    #[test]
    fn unfollowable_shim_is_refused() {
        assert_eq!(parse_cmd_shim("@echo off\r\nsomething.exe %*\r\n"), None);
    }

    #[test]
    fn finds_on_explicit_path_only_when_runnable() {
        let dir = crate::testing::TempDir::new();
        assert!(find_on_path("no-such-tool-xyz", Some(dir.path().as_os_str())).is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dropping_the_owner_kills_descendants() {
        let dir = crate::testing::TempDir::new();
        let pid_file = dir.path().join("pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
        let command = ResolvedCommand {
            program: "/bin/sh".into(),
            prefix: Vec::new(),
            path: "/bin/sh".into(),
        };
        let child = ManagedChild::spawn(engine_command(
            &command,
            &["-c".into(), script.into()],
            dir.path(),
        ))
        .unwrap();
        let grandchild: i32 = loop {
            if let Ok(text) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = text.trim().parse()
            {
                break pid;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        };
        drop(child);
        for _ in 0..200 {
            // SAFETY: signal 0 only probes for existence.
            if unsafe { libc::kill(grandchild, 0) } != 0 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("grandchild survived its owner");
    }
}
