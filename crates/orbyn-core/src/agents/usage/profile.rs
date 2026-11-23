//! Which launch profile owns this process, and the history folder it may read.
//!
//! A person's own profile is not enough to decide the answer: a build can inherit another
//! launcher's environment, and the packaged smoke harness deliberately relocates every
//! profile path beneath its sandbox. Only a launch that declares the profile reads the
//! paths that profile relocated; anything else reads the person's own home (#1105).

use std::path::{Path, PathBuf};

use super::Engine;

/// The launch profile that owns this process. The packaged smoke harness declares
/// [`PROFILE_ENV`] for the app it starts; every other launch is a person's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryProfile {
    Personal,
    Smoke,
}

/// What a launcher sets to declare the profile it starts: `smoke`, and nothing else.
pub(crate) const PROFILE_ENV: &str = "ORBYN_LAUNCH_PROFILE";
/// The sandbox root the smoke harness declares for the profile it hands out.
pub(crate) const SANDBOX_ENV: &str = "ORBYN_SMOKE_SANDBOX";

impl HistoryProfile {
    fn declared(value: Option<&std::ffi::OsStr>) -> Self {
        match value.and_then(std::ffi::OsStr::to_str) {
            Some("smoke") => Self::Smoke,
            _ => Self::Personal,
        }
    }

    fn current() -> Self {
        Self::declared(std::env::var_os(PROFILE_ENV).as_deref())
    }
}

/// The environment one history root is resolved from, so both launch profiles are
/// verifiable without changing this process's own variables.
pub(crate) struct HistoryEnv {
    pub(crate) profile: HistoryProfile,
    pub(crate) sandbox: Option<PathBuf>,
    pub(crate) claude_config_dir: Option<PathBuf>,
    pub(crate) codex_home: Option<PathBuf>,
    pub(crate) xdg_data_home: Option<PathBuf>,
    pub(crate) home: Option<PathBuf>,
    pub(crate) account_home: Option<PathBuf>,
}

impl HistoryEnv {
    fn from_process() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Self {
            profile: HistoryProfile::current(),
            sandbox: var(SANDBOX_ENV),
            claude_config_dir: var("CLAUDE_CONFIG_DIR"),
            codex_home: var("CODEX_HOME"),
            xdg_data_home: var("XDG_DATA_HOME"),
            home: crate::util::home_dir(),
            account_home: crate::util::account_home(),
        }
    }

    /// One engine's history folder, resolved for the profile that owns this process.
    fn root(&self, engine: Engine) -> Option<PathBuf> {
        match engine {
            Engine::Claude => self
                .base(self.claude_config_dir.as_deref(), ".claude")
                .map(|root| root.join("projects")),
            Engine::Codex => self
                .base(self.codex_home.as_deref(), ".codex")
                .map(|root| root.join("sessions")),
            // Muse resolves its own home when it starts, and no Muse driver is here to
            // ask. Its recorded logs sit under the XDG data directory
            // (`~/.local/share/muse/sessions/2026/10/11/<session>/session.jsonl`).
            Engine::Muse => self
                .base(self.xdg_data_home.as_deref(), ".local/share")
                .map(|root| root.join("muse").join("sessions")),
        }
    }

    /// The folder an engine keeps under its own profile: the variable that names it,
    /// but only while that path is this launch's own, else the person's own home.
    fn base(&self, named: Option<&Path>, under_home: &str) -> Option<PathBuf> {
        if let Some(named) = named.filter(|named| self.owns(named)) {
            return Some(named.to_path_buf());
        }
        self.own_home().map(|home| home.join(under_home))
    }

    /// Whether a relocated path is this launch's own. The smoke profile owns its
    /// sandbox; a personal launch owns nothing inside a sandbox it did not declare, so a
    /// harness profile inherited from a launcher never becomes the person's history.
    fn owns(&self, path: &Path) -> bool {
        self.profile == HistoryProfile::Smoke
            || self
                .sandbox
                .as_deref()
                .is_none_or(|sandbox| !path.starts_with(sandbox))
    }

    /// The home this launch reads: the declared sandbox for the smoke profile; for a
    /// person, the home the environment names while it is this launch's own, else the
    /// account's own home from the user database.
    fn own_home(&self) -> Option<PathBuf> {
        if self.profile == HistoryProfile::Smoke {
            return self.home.clone();
        }
        match self.home.clone().filter(|home| self.owns(home)) {
            Some(home) => Some(home),
            // The inherited home is the harness's, so nothing in it is this launch's to
            // read. Without an account home the root is unknown rather than the sandbox.
            None => self.account_home.clone(),
        }
    }
}

pub(crate) fn history_root(engine: Engine) -> Option<PathBuf> {
    HistoryEnv::from_process().root(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An environment a smoke launcher handed out, inherited by another launch: every
    /// profile path sits inside the sandbox and the real account is elsewhere.
    fn inherited_smoke_env() -> HistoryEnv {
        let sandbox = PathBuf::from("/smoke/root");
        let home = sandbox.join("home");
        HistoryEnv {
            profile: HistoryProfile::Personal,
            sandbox: Some(sandbox),
            claude_config_dir: Some(home.join(".claude")),
            codex_home: Some(home.join(".codex")),
            xdg_data_home: Some(home.join(".local").join("share")),
            home: Some(home),
            account_home: Some(PathBuf::from("/people/ana")),
        }
    }

    fn engines() -> [(&'static str, Engine); 3] {
        [
            ("Claude", Engine::Claude),
            ("Codex", Engine::Codex),
            ("Muse", Engine::Muse),
        ]
    }

    #[test]
    fn only_a_declared_smoke_launch_is_the_smoke_profile() {
        use std::ffi::OsStr;
        assert_eq!(HistoryProfile::declared(None), HistoryProfile::Personal);
        assert_eq!(
            HistoryProfile::declared(Some(OsStr::new("personal"))),
            HistoryProfile::Personal
        );
        assert_eq!(
            HistoryProfile::declared(Some(OsStr::new("smoke"))),
            HistoryProfile::Smoke
        );
    }

    #[test]
    fn a_personal_launch_reads_the_accounts_own_history_not_an_inherited_smoke_profile() {
        let env = inherited_smoke_env();
        assert_eq!(
            env.root(Engine::Claude),
            Some(PathBuf::from("/people/ana/.claude/projects"))
        );
        assert_eq!(
            env.root(Engine::Codex),
            Some(PathBuf::from("/people/ana/.codex/sessions"))
        );
        assert_eq!(
            env.root(Engine::Muse),
            Some(PathBuf::from("/people/ana/.local/share/muse/sessions"))
        );
        // The negative case: no engine reads a path the harness owns.
        let sandbox = env.sandbox.clone().expect("the smoke's sandbox");
        for (name, engine) in engines() {
            let root = env.root(engine).expect("a root");
            assert!(!root.starts_with(&sandbox), "{name} got {root:?}");
        }
    }

    #[test]
    fn a_declared_smoke_launch_keeps_reading_its_own_sandbox_history() {
        let mut env = inherited_smoke_env();
        env.profile = HistoryProfile::Smoke;
        assert_eq!(
            env.root(Engine::Claude),
            Some(PathBuf::from("/smoke/root/home/.claude/projects"))
        );
        assert_eq!(
            env.root(Engine::Codex),
            Some(PathBuf::from("/smoke/root/home/.codex/sessions"))
        );
        assert_eq!(
            env.root(Engine::Muse),
            Some(PathBuf::from("/smoke/root/home/.local/share/muse/sessions"))
        );
        // The same environment resolves to a different path per profile.
        let personal = inherited_smoke_env();
        for (name, engine) in engines() {
            assert_ne!(env.root(engine), personal.root(engine), "{name}");
        }
    }

    #[test]
    fn a_personal_launch_keeps_a_deliberate_config_override() {
        // Without a declared sandbox, a person's own relocation stays authoritative.
        let mut env = inherited_smoke_env();
        env.sandbox = None;
        assert_eq!(
            env.root(Engine::Codex),
            Some(PathBuf::from("/smoke/root/home/.codex/sessions"))
        );
        assert_eq!(
            env.root(Engine::Claude),
            Some(PathBuf::from("/smoke/root/home/.claude/projects"))
        );
    }

    #[test]
    fn a_personal_launch_reports_no_history_rather_than_the_sandbox_it_inherited() {
        // The account home could not be resolved: the inherited sandbox is still refused.
        let mut env = inherited_smoke_env();
        env.account_home = None;
        for (name, engine) in engines() {
            assert_eq!(env.root(engine), None, "{name}");
        }
    }
}
