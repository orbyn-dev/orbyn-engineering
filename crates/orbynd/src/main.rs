//! `orbynd`: transport and CLI over `orbyn-core`.

mod http;
mod recent_events;
mod routes;

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use http::AppState;
use http::access::Access;
use orbyn_core::agents::engines::EngineConfig;
use orbyn_core::util::{random_token, sha256_hex};
use orbyn_core::{Core, CoreConfig, CoreError};
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

const REMOTE_KEY_SETTING: &str = "remote_key_sha256";
/// Where the `orbyn` command finds this service.
const SERVICE_FILE: &str = "service.json";
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const SANDBOX_MODES: &[&str] = &["read-only", "workspace-write", "danger-full-access"];

#[derive(Parser)]
#[command(name = "orbynd", version, about = "Orbyn Engineering core service")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the API and UI.
    Serve(ServeArgs),
}

#[derive(Args)]
struct ServeArgs {
    /// Port to bind; 0 picks a free one.
    #[arg(long, default_value_t = 0)]
    port: u16,
    #[arg(long)]
    data_dir: Option<PathBuf>,
    #[arg(long)]
    ui_dir: Option<PathBuf>,
    /// Replaced by the managed workspace. Accepted and ignored so older shells
    /// that still pass it keep starting.
    #[arg(long, hide = true)]
    skills_dir: Option<PathBuf>,
    /// Release manifest to compare against when no desktop shell is attached.
    #[arg(long, env = "ORBYN_UPDATE_MANIFEST")]
    update_manifest: Option<String>,
    /// Where the clone that replaces the shipped snapshot comes from.
    #[arg(long, env = "ORBYN_WORKSPACE_URL")]
    workspace_url: Option<String>,
    /// The folder holding the library snapshot that ships with the app.
    #[arg(long, env = "ORBYN_LIBRARY_BUNDLE")]
    library_bundle: Option<PathBuf>,
    /// Bind every interface. Requires `--tls-terminated`.
    #[arg(long)]
    remote: bool,
    /// Asserts that Tailscale, SSH or a proxy provides transport security.
    #[arg(long)]
    tls_terminated: bool,
    /// An extra accepted `Host` value. Repeatable.
    #[arg(long = "allowed-host")]
    allowed_hosts: Vec<String>,
    /// Exit when stdin closes, so a parent shell cannot orphan this service.
    #[arg(long)]
    parent_stdin: bool,
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "1" || value == "true")
}

fn known_engine_ids(raw: Option<&std::ffi::OsStr>) -> Result<Option<Vec<String>>, String> {
    raw.map(|value| {
        let value = value
            .to_str()
            .ok_or_else(|| "ORBYN_KNOWN_ENGINE_IDS must be UTF-8 JSON.".to_owned())?;
        serde_json::from_str(value)
            .map_err(|_| "ORBYN_KNOWN_ENGINE_IDS must be a JSON array of strings.".to_owned())
    })
    .transpose()
}

/// The background listeners' periodic pass, local startup policy only: 60 seconds to 24 hours, or unset for off:
/// with no interval no pass runs at all. Neither HTTP nor a tool can change it.
fn background_watch_interval(raw: Option<&std::ffi::OsStr>) -> Result<Option<Duration>, String> {
    raw.map(|raw| {
        let seconds = raw
            .to_str()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| (60..=86400).contains(n))
            .ok_or(
                "ORBYN_CREW_BACKGROUND_WATCH_INTERVAL_SECS must be 60 to 86400, or unset for off.",
            )?;
        Ok(Duration::from_secs(seconds))
    })
    .transpose()
}

/// Local startup policy only. Neither HTTP nor a tool can change these values.
fn crew_merge_config(
    preview: bool,
    checks: Option<&std::ffi::OsStr>,
    interval: Option<&std::ffi::OsStr>,
) -> Result<(Vec<String>, Option<Duration>), String> {
    if !preview {
        return Ok((Vec::new(), None));
    }
    let required: Vec<String> = match checks {
        None => Vec::new(),
        Some(raw) => serde_json::from_str(
            raw.to_str()
                .ok_or("ORBYN_CREW_REQUIRED_CHECKS must be UTF-8 JSON.")?,
        )
        .map_err(|_| "ORBYN_CREW_REQUIRED_CHECKS must be a JSON array of check names.")?,
    };
    let unique: std::collections::BTreeSet<_> = required.iter().collect();
    if required.len() > 128
        || unique.len() != required.len()
        || required.iter().any(|s| {
            s.is_empty() || s.len() > 200 || s.trim() != s || s.chars().any(char::is_control)
        })
    {
        return Err("ORBYN_CREW_REQUIRED_CHECKS accepts at most 128 unique nonempty check names of at most 200 bytes.".into());
    }
    let every = interval
        .map(|raw| {
            let seconds = raw
                .to_str()
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|n| (60..=86400).contains(n))
                .ok_or(
                    "ORBYN_CREW_WATCH_INTERVAL_SECS must be 60 to 86400, or unset to disable.",
                )?;
            Ok::<_, String>(Duration::from_secs(seconds))
        })
        .transpose()?;
    Ok((required, every))
}

fn core_config(args: &ServeArgs, base_url: String) -> Result<CoreConfig, String> {
    let data_dir = args
        .data_dir
        .clone()
        .or_else(orbyn_core::app::default_data_dir)
        .ok_or("Set --data-dir: no app-data directory could be found.")?;
    let mut config = CoreConfig::new(data_dir, base_url);
    if args.skills_dir.is_some() {
        tracing::warn!("--skills-dir is ignored: skills come from the managed workspace");
    }
    config.update_manifest.clone_from(&args.update_manifest);
    if let Some(url) = &args.workspace_url {
        config.workspace_url.clone_from(url);
    }
    config.library_bundle.clone_from(&args.library_bundle);
    let defaults = EngineConfig::default();
    let sandbox = std::env::var("ORBYN_CODEX_SANDBOX").unwrap_or(defaults.codex_sandbox);
    if !SANDBOX_MODES.contains(&sandbox.as_str()) {
        return Err(format!(
            "ORBYN_CODEX_SANDBOX must be one of {SANDBOX_MODES:?}."
        ));
    }
    config.engines = EngineConfig {
        claude_program: std::env::var("ORBYN_CLAUDE_BIN").unwrap_or(defaults.claude_program),
        codex_program: std::env::var("ORBYN_CODEX_BIN").unwrap_or(defaults.codex_program),
        codex_sandbox: sandbox,
        claude_task_tools: env_flag("ORBYN_CLAUDE_TASK_TOOLS"),
        lookup_latest: !env_flag("ORBYN_OFFLINE"),
        update_timeout: defaults.update_timeout,
        known_engine_ids: known_engine_ids(std::env::var_os("ORBYN_KNOWN_ENGINE_IDS").as_deref())?,
        cache_dir: None,
        approval_timeout: defaults.approval_timeout,
        engine_host: orbyn_core::agents::driver::host::HostConfig::from_env(
            std::env::var_os("ORBYN_ENGINE_HOST"),
            std::env::var("ORBYN_ENGINE_HOST_CLAUDE").ok().as_deref(),
        ),
    };
    if let Ok(seconds) = std::env::var("ORBYN_APPROVAL_TIMEOUT_SECS") {
        let seconds: u64 = seconds
            .parse()
            .map_err(|_| "ORBYN_APPROVAL_TIMEOUT_SECS must be a number.".to_owned())?;
        config.approval_timeout = Duration::from_secs(seconds);
    }
    config.desktop_program = std::env::var("ORBYN_AGENT_DESKTOP").ok();
    config.browser_path = std::env::var_os("ORBYN_BROWSER").map(PathBuf::from);
    if let Ok(url) = std::env::var("ORBYN_ACP_REGISTRY_URL") {
        config.acp_registry_url = url;
    }
    config.browser_headless = env_flag("ORBYN_BROWSER_HEADLESS");
    config.browser_in_process_gpu = env_flag("ORBYN_BROWSER_IN_PROCESS_GPU");
    development_account(&mut config);
    // Crew is Preview and has no screen yet: a developer turns its tools on here.
    config.crew_preview = env_flag("ORBYN_CREW_PREVIEW");
    config.code_mode = env_flag("ORBYN_CODE_MODE");
    (
        config.crew_merge_required_checks,
        config.crew_merge_watch_interval,
    ) = crew_merge_config(
        config.crew_preview,
        std::env::var_os("ORBYN_CREW_REQUIRED_CHECKS").as_deref(),
        std::env::var_os("ORBYN_CREW_WATCH_INTERVAL_SECS").as_deref(),
    )?;
    config.crew_background_watch_interval = if config.crew_preview {
        background_watch_interval(
            std::env::var_os("ORBYN_CREW_BACKGROUND_WATCH_INTERVAL_SECS").as_deref(),
        )?
    } else {
        None
    };
    Ok(config)
}

/// Turns on the account's development stand-in, for debug builds only: the gate
/// `LocalOnly` and an in-memory backend with throwaway accounts (see
/// `orbyn_core::account::dev`). A release build has neither the stand-in nor this
/// flag, so the account stays `Off` there. Real sign-in is `Off` until every condition
/// in docs/account.md, "Sign-in gate", holds.
#[cfg(debug_assertions)]
fn development_account(config: &mut CoreConfig) {
    if env_flag("ORBYN_ACCOUNT_DEV") {
        config.account_gate = orbyn_core::account::AccountGate::LocalOnly;
        config.account_backend = orbyn_core::account::AccountBackendKind::Development;
    }
}

#[cfg(not(debug_assertions))]
fn development_account(_config: &mut CoreConfig) {}

/// Returns the stored key digest, creating a key on first remote start. The key
/// itself is shown once and never stored.
fn remote_key(core: &Core) -> Result<(String, Option<String>), CoreError> {
    if let Some(digest) = core.store.setting(REMOTE_KEY_SETTING)? {
        return Ok((digest, None));
    }
    let key = random_token()?;
    let digest = sha256_hex(key.as_bytes());
    core.store.set_setting(REMOTE_KEY_SETTING, &digest)?;
    Ok((digest, Some(key)))
}

/// Reads newline JSON from the desktop shell. EOF means the shell is gone.
fn read_parent(core: Arc<Core>, closed: oneshot::Sender<()>) {
    use std::io::BufRead;
    // A plain thread: tokio's stdin would hold the runtime open at exit.
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            match line {
                Ok(line) => {
                    if !core.shell.on_line(&line) && !core.presence_line(&line) {
                        core.updates.on_shell_line(&line);
                    }
                }
                // Not UTF-8 is a bad line, not a closed pipe.
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
                Err(_) => break,
            }
        }
        let _ = closed.send(());
    });
}

async fn wait_for_stop(
    parent: Option<oneshot::Receiver<()>>,
    stop: CancellationToken,
    termination: impl std::future::Future<Output = ()> + Send,
) {
    let parent = async {
        match parent {
            Some(closed) => {
                let _ = closed.await;
            }
            None => std::future::pending::<()>().await,
        }
    };
    tokio::select! {
        () = parent => {}
        () = termination => {}
    }
    stop.cancel();
}

#[cfg(unix)]
fn termination() -> Result<impl std::future::Future<Output = ()> + Send, String> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = signal(SignalKind::terminate())
        .map_err(|error| format!("Could not listen for termination: {error}"))?;
    let mut interrupt = signal(SignalKind::interrupt())
        .map_err(|error| format!("Could not listen for interrupts: {error}"))?;
    Ok(async move {
        tokio::select! {
            _ = terminate.recv() => {}
            _ = interrupt.recv() => {}
        }
    })
}

#[cfg(windows)]
fn termination() -> Result<impl std::future::Future<Output = ()> + Send, String> {
    let mut interrupt = tokio::signal::windows::ctrl_c()
        .map_err(|error| format!("Could not listen for interrupts: {error}"))?;
    Ok(async move {
        let _ = interrupt.recv().await;
    })
}

async fn serve(args: ServeArgs) -> Result<(), String> {
    if args.remote && !args.tls_terminated {
        return Err(
            "--remote needs --tls-terminated: confirm that Tailscale, SSH or a proxy secures the connection."
                .into(),
        );
    }
    let ip = if args.remote {
        Ipv4Addr::UNSPECIFIED
    } else {
        Ipv4Addr::LOCALHOST
    };
    let listener = TcpListener::bind(SocketAddr::from((ip, args.port)))
        .await
        .map_err(|error| format!("Could not bind port {}: {error}", args.port))?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    let base_url = format!("http://127.0.0.1:{port}");
    let core = Arc::new(
        Core::open(core_config(&args, base_url.clone())?).map_err(|error| error.to_string())?,
    );
    core.mcp.bind_host(&core);
    core.engines.bind_host_core(&core);
    core.set_crew_engine_approvers();
    orbyn_core::operations::diagnostics::start_sampling();
    core.start_telemetry_export(telemetry_interval()?)
        .map_err(|error| error.to_string())?;
    // Parent EOF and termination own startup too: provisioning may be waiting
    // on a helper before any ready line or service handle exists.
    // Install handlers synchronously before any cancellable provisioning work.
    let termination = termination()?;
    let stop = CancellationToken::new();
    let parent = args.parent_stdin.then(|| {
        let (closed, wait) = oneshot::channel();
        read_parent(core.clone(), closed);
        wait
    });
    tokio::spawn(wait_for_stop(parent, stop.clone(), termination));
    // Before the service answers anything. If the disk refuses, the background
    // attempt tries again, and the service still serves what needs no library.
    let library = tokio::select! {
        biased;
        () = stop.cancelled() => {
            // Dropping provision first releases its staging and helper owners.
            core.shutdown().await;
            return Ok(());
        }
        result = core.workspace.provision() => result.and_then(|()| core.ensure_library_project()),
    };
    if let Err(error) = library {
        tracing::error!("the skills library is not in place: {error}");
    }
    if stop.is_cancelled() {
        core.shutdown().await;
        return Ok(());
    }
    let token = random_token().map_err(|error| error.to_string())?;
    let (key_digest, new_key) = if args.remote {
        let (digest, key) = remote_key(&core).map_err(|error| error.to_string())?;
        (Some(digest), key)
    } else {
        (None, None)
    };
    // The person key exists only when a desktop shell owns this service, and it leaves this function once: on the
    // ready line, to the pipe the shell spawned. Access keeps its digest; nothing writes the key to disk, env, argv,
    // a URL or a log (docs/worker-token-isolation.md).
    let person_key = if args.parent_stdin {
        Some(random_token().map_err(|error| error.to_string())?)
    } else {
        None
    };
    let access = Access::new(&token, args.allowed_hosts.clone(), key_digest)
        .with_person_key(person_key.as_deref());
    #[cfg(debug_assertions)]
    let access = {
        let open = env_flag("ORBYN_DEV_DISABLE_PERSON_KEY");
        if open {
            // Plain stderr, not a tracing event: a warning event is also exported to the logs endpoint a person
            // configured, and a development flag is no record to send there.
            eprintln!(
                "ORBYN_DEV_DISABLE_PERSON_KEY is on: person routes need no key. Development builds only."
            );
        }
        access.with_dev_person_override(open)
    };
    let access = Arc::new(
        access
            .with_desktop(args.parent_stdin)
            // A frame URL carries a capability for one published page instead of the
            // service token, and only core knows which render a token belongs to.
            .with_frames({
                let core = core.clone();
                Arc::new(move |thread: &str, render: &str, token: &str| {
                    core.render_frame_token_matches(thread, render, token)
                })
            }),
    );
    let app = routes::router(AppState {
        core: core.clone(),
        access,
        remote: args.remote,
        ui_dir: args.ui_dir.clone(),
        images: Arc::new(orbyn_core::browser::remote_images::ImageProxy::default()),
    });

    // Attach before readiness so the early parent reader cannot drop a reply
    // sent immediately after the shell receives the ready line. Forward later,
    // keeping readiness first on stdout.
    let shell_lines = args.parent_stdin.then(|| {
        let lines = core.updates.attach_shell();
        if let Some(sender) = core.updates.shell_lines() {
            core.shell.attach(sender);
        }
        lines
    });
    let mut start = json!({ "url": base_url, "token": token });
    if let Some(key) = new_key {
        start["access_key"] = json!(key);
    }
    if let Some(key) = person_key {
        start["person_key"] = json!(key);
    }
    let handle = core.config.data_dir.join(SERVICE_FILE);
    let found = json!({ "url": base_url, "token": token, "pid": std::process::id() });
    if let Err(error) =
        orbyn_core::util::write_private_atomic(&handle, found.to_string().as_bytes())
    {
        tracing::warn!(%error, "the orbyn command will not find this service");
    }
    println!("{start}");

    if let Some(mut lines) = shell_lines {
        // Every line after the first ready line is for the shell.
        tokio::spawn(async move {
            while let Some(line) = lines.recv().await {
                println!("{line}");
            }
        });
    }
    let library_upgrades = {
        let (core, stop) = (core.clone(), stop.clone());
        tokio::spawn(async move { core.run_library_upgrades(stop).await })
    };
    let checks = {
        let (updates, stop) = (core.updates.clone(), stop.clone());
        tokio::spawn(async move { updates.run_checks(stop).await })
    };
    let lifecycle = {
        let (core, stop) = (core.clone(), stop.clone());
        tokio::spawn(async move { core.run_lifecycle(stop).await })
    };
    let crew_watch = if core.config.crew_preview {
        let (core, watch_stop) = (core.clone(), stop.child_token());
        Some(tokio::spawn(async move {
            core.run_crew_merge_watch(watch_stop).await
        }))
    } else {
        None
    };
    let crew_escape_scan = if core.config.crew_preview {
        let (core, scan_stop) = (core.clone(), stop.child_token());
        Some(tokio::spawn(async move {
            core.run_crew_escape_scan(scan_stop).await
        }))
    } else {
        None
    };
    let crew_background =
        if core.config.crew_preview && core.config.crew_background_watch_interval.is_some() {
            let (core, background_stop) = (core.clone(), stop.child_token());
            Some(tokio::spawn(async move {
                core.run_crew_background_listeners(background_stop).await
            }))
        } else {
            None
        };
    if args.parent_stdin {
        let (activity, quit) = (core.clone(), stop.clone());
        tokio::spawn(async move { activity.run_shell_activity(quit).await });
        let (links, quit) = (core.clone(), stop.clone());
        tokio::spawn(async move { links.run_web_links(quit).await });
        // The first usage read would otherwise scan all history while a person waits.
        // Only someone who has opened Usage once pays for the scan at launch.
        let (usage, quit) = (core.clone(), stop.clone());
        if usage
            .store
            .setting(routes::usage::USAGE_OPENED)
            .ok()
            .flatten()
            .is_some()
        {
            tokio::spawn(async move {
                tokio::select! {
                    () = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                        usage.usage.warm(quit.clone()).await;
                    }
                    () = quit.cancelled() => {}
                }
            });
        }
    }
    let shutdown = stop.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { shutdown.cancelled().await })
            .await
    });
    stop.cancelled().await;
    tracing::info!("stopping");
    let _ = lifecycle.await;
    if let Some(watch) = crew_watch {
        match watch.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "Crew watch stopped with an error"),
            Err(error) => tracing::warn!(%error, "Crew watch task did not complete"),
        }
    }
    if let Some(scan) = crew_escape_scan {
        match scan.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(%error, "Crew escape scan stopped with an error"),
            Err(error) => tracing::warn!(%error, "Crew escape scan task did not complete"),
        }
    }
    if let Some(background) = crew_background {
        match background.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(%error, "Crew background listeners stopped with an error")
            }
            Err(error) => tracing::warn!(%error, "Crew background listeners task did not complete"),
        }
    }
    checks.abort();
    let _ = checks.await;
    let _ = library_upgrades.await;
    core.shutdown().await;
    let _ = std::fs::remove_file(handle);
    // Open streams never finish on their own; they end with the process.
    let _ = tokio::time::timeout(SHUTDOWN_GRACE, server).await;
    Ok(())
}

fn telemetry_interval() -> Result<std::time::Duration, String> {
    match std::env::var("ORBYN_TELEMETRY_INTERVAL_MS") {
        Ok(text) => match text.parse::<u64>() {
            Ok(millis) if millis > 0 => Ok(std::time::Duration::from_millis(millis)),
            _ => Err("ORBYN_TELEMETRY_INTERVAL_MS must be a positive number.".to_owned()),
        },
        Err(_) => Ok(orbyn_core::operations::telemetry_export::DEFAULT_INTERVAL),
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(
                    EnvFilter::try_from_env("ORBYN_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
                ),
        )
        .with(recent_events::RecentEvents.with_filter(LevelFilter::WARN))
        .init();
    let cli = Cli::parse();
    let outcome = match cli.command {
        Command::Serve(args) => serve(args).await,
    };
    match outcome {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod known_engine_config_tests {
    use super::known_engine_ids;
    #[test]
    fn known_discovery_config_is_optional_and_rejects_invalid_json() {
        assert_eq!(known_engine_ids(None).unwrap(), None);
        assert_eq!(
            known_engine_ids(Some(std::ffi::OsStr::new("[]"))).unwrap(),
            Some(Vec::new())
        );
        assert_eq!(
            known_engine_ids(Some(std::ffi::OsStr::new(r#"["opencode"]"#))).unwrap(),
            Some(vec!["opencode".into()])
        );
        for value in ["", "broken", "null", "{}", "[1]", r#""opencode""#] {
            assert!(
                known_engine_ids(Some(std::ffi::OsStr::new(value))).is_err(),
                "{value}"
            );
        }
    }
}

#[cfg(test)]
mod crew_merge_config_tests {
    use super::crew_merge_config;
    use std::ffi::OsStr;

    #[test]
    fn merge_policy_is_local_bounded_and_off_without_preview() {
        assert_eq!(
            crew_merge_config(false, Some(OsStr::new("bad")), Some(OsStr::new("1"))).unwrap(),
            (Vec::new(), None)
        );
        assert_eq!(
            crew_merge_config(true, None, None).unwrap(),
            (Vec::new(), None)
        );
        let (checks, interval) = crew_merge_config(
            true,
            Some(OsStr::new(r#"["Rust (windows-2022)","Web"]"#)),
            Some(OsStr::new("60")),
        )
        .unwrap();
        assert_eq!(checks, ["Rust (windows-2022)", "Web"]);
        assert_eq!(interval, Some(std::time::Duration::from_secs(60)));
        for bad in ["null", "{}", r#"[""]"#, r#"["ci","ci"]"#, r#"[" ci"]"#] {
            assert!(crew_merge_config(true, Some(OsStr::new(bad)), None).is_err());
        }
        for bad in ["0", "59", "86401", "invalid", "-1"] {
            assert!(crew_merge_config(true, None, Some(OsStr::new(bad))).is_err());
        }
    }
}

#[cfg(test)]
mod background_watch_interval_tests {
    use super::background_watch_interval;
    use std::ffi::OsStr;

    #[test]
    fn background_interval_is_off_by_default_and_bounded() {
        assert_eq!(background_watch_interval(None).unwrap(), None);
        assert_eq!(
            background_watch_interval(Some(OsStr::new("60"))).unwrap(),
            Some(std::time::Duration::from_secs(60))
        );
        assert_eq!(
            background_watch_interval(Some(OsStr::new("86400"))).unwrap(),
            Some(std::time::Duration::from_secs(86400))
        );
        for bad in ["0", "59", "86401", "invalid", "-1"] {
            assert!(background_watch_interval(Some(OsStr::new(bad))).is_err());
        }
    }
}
