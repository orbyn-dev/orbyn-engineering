//! Expected failures and their stable client codes. Transport maps `status` and
//! `code` directly to `ApiError`, so a client can branch without parsing prose.

use orbyn_contracts::{Approval, Project, QuestionRequest};

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("{0} not found")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Settings changed. Reload this scope before saving.")]
    SettingsConflict,
    #[error("This settings scope is unavailable. Choose an available scope.")]
    SettingsScopeUnavailable,
    #[error("this thread already has an active run")]
    ThreadBusy,
    #[error("Close this thread's engine console first. It is using the same session.")]
    ConsoleOpen,
    #[error("trust this folder before starting an engine in it")]
    FolderUntrusted,
    /// A route that writes into a folder needs the person's trust in that folder, with its own words.
    #[error("Trust this folder before initialising Git in it.")]
    FolderUntrustedInit,
    #[error("this approval was already decided")]
    AlreadyDecided(Box<Approval>),
    #[error("This folder is already a project.")]
    ProjectExists(Box<Project>),
    #[error("this question was already closed")]
    QuestionClosed(Box<QuestionRequest>),
    #[error("{0} has an active run or update")]
    EngineBusy(String),
    #[error("{0}")]
    EngineUnavailable(String),
    #[error("this thread is archived")]
    ThreadArchived,
    #[error("The project \"{0}\" was removed. Add its folder back to use it again.")]
    ProjectRemoved(String),
    #[error("set up the workspace first")]
    WorkspaceRequired,
    #[error("{0}")]
    WorkspaceRefused(String),
    #[error("{0}")]
    LibraryDirty(String),
    #[error("{0}")]
    NotFastForward(String),
    #[error("This file changed since you opened it.")]
    FileConflict { revision: Option<String> },
    #[error("This file is too large to open here: {size} bytes, over the {limit} byte limit.")]
    FileTooLarge { size: u64, limit: u64 },
    #[error("The Snap Shot's {field} is over the {limit} character limit.")]
    SourceTooLarge { field: &'static str, limit: usize },
    #[error("{0} other thread(s) work in this folder, so restoring could lose their work.")]
    SharedDirectory(usize),
    #[error("finish or stop active runs first")]
    RunsActive,
    #[error("Orbyn is installing an update")]
    Updating,
    #[error("{message}")]
    ProjectHasWork {
        message: String,
        detail: serde_json::Value,
    },
    /// The forge refused a merge because the pull request gained a commit the person had not seen.
    #[error("This pull request changed. Review it again before merging.")]
    PullHeadChanged,
    #[error("{0}")]
    Unsupported(String),
    /// A permission mode the engine does not offer.
    #[error("{0}")]
    PermissionModeUnsupported(String),
    /// A folder that is already a repository of a project.
    #[error("{0}")]
    RepoExists(String),
    /// An untrusted repository was asked to do anything but be trusted.
    #[error("{0}")]
    RepoUntrusted(String),
    /// More repositories than a thread may work in.
    #[error("{0}")]
    RepoLimit(String),
    /// A clone source the service will not run.
    #[error("{0}")]
    CloneRefused(String),
    /// An action that needs Git, on a folder that has none.
    #[error("{0}")]
    NotGit(String),
    /// An issue tracker with no connection, or whose tool is missing or signed out.
    #[error("{0}")]
    TrackerNotConnected(String),
    /// A tracker asked us to slow down; the message says for how long when it told us.
    #[error("{0}")]
    TrackerRateLimited(String),
    /// A tracker refused the token or the repository.
    #[error("{0}")]
    TrackerForbidden(String),
    /// A tracker did not answer, or answered something we cannot read.
    #[error("{0}")]
    TrackerUnavailable(String),
    /// A service limit or the operating system's file watching is busy or failing right
    /// now; the same request can succeed later.
    #[error("{0}")]
    WatchUnavailable(String),
    /// A model, effort or option the chosen model does not offer.
    #[error("{0}")]
    UnsupportedOption(String),
    #[error("{0}")]
    UpdateUnavailable(String),
    #[error("{0}")]
    UpdateFailed(String),
    #[error("{0}")]
    PolicyDenied(String),
    /// A Crew start the disk guard refused, kept apart from a plain denial so a background listener records `disk_low`
    /// instead of the generic word. The person still reads the guard's own sentence, and a route still answers
    /// `policy_denied` with 403 as it did before.
    #[error("{0}")]
    DiskLow(String),
    /// The engine does not have the session a run tried to resume.
    #[error("the engine no longer has this session")]
    SessionLost,
    /// A write from a process that no longer owns the run.
    #[error("run is owned by another process")]
    StaleOwner,
    /// A write against a run that already reached a terminal state.
    #[error("run has ended")]
    RunClosed,
    /// A Crew task is busy or in a state that refuses this request: another re-run of its checks, a verification, a worker.
    #[error("{0}")]
    CrewTaskBusy(String),
    #[error("run cannot move from {from} to {to}")]
    InvalidTransition { from: String, to: String },
    #[error("database schema {found} is newer than supported schema {supported}")]
    DatabaseTooNew { found: i64, supported: i64 },
    #[error("another orbynd owns this data directory")]
    DataDirLocked,
    #[error("skill pack failed verification: {0}")]
    PackTampered(String),
    #[error("{0}")]
    Computer(String),
    #[error("{0}")]
    Git(String),
    #[error("storage error: {0}")]
    Store(#[from] rusqlite::Error),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("encoding error: {0}")]
    Json(#[from] serde_json::Error),
    /// The sign-in gate is `Off`: no account route works and no request goes to any Engine.
    #[error("Accounts are not available yet.")]
    AccountDisabled,
    /// An account action that needs the password typed again in the last 10 minutes.
    #[error("Confirm it's you to do this.")]
    ReauthRequired,
    /// The Engine, or the development stand-in for it, refused. `code` is one of the
    /// device client's error names; `retry_after_secs` says how long a rate limit lasts.
    #[error("{message}")]
    Account {
        code: &'static str,
        message: String,
        retry_after_secs: Option<u64>,
        /// Wrong two-factor codes left before the sign-in closes.
        tries_left: Option<u8>,
    },
    /// A pattern for Settings › Secrets the service will not save: `code` is `invalid_pattern`
    /// (it cannot be used) or `pattern_too_broad` (it would hide nearly everything; the
    /// person can still save it). `detail` carries the position or the numbers.
    #[error("{message}")]
    PatternRefused {
        code: &'static str,
        message: String,
        detail: serde_json::Value,
    },
    /// A run start an engine's own rules refuse, with the fields a screen acts on (docs/cursor-thread-contract.md): `code` is
    /// `engine_mode_refused` or `engine_notice_required`, and `fields` are sent beside `code` and `message`.
    #[error("{message}")]
    EngineRefused {
        code: &'static str,
        message: String,
        fields: serde_json::Value,
    },
    /// An action that only a person at the desktop app may give: the service has no desktop shell (it was started without one, or
    /// as a remote service), so nothing can be approved.
    #[error("Approving a command needs the Orbyn desktop app on this computer.")]
    DesktopRequired,
    /// The desktop app has not recorded a person's approval for this hold, or it is used, expired or for something else.
    #[error("Approve this command in the Orbyn desktop app first.")]
    PresenceRequired,
    /// The thing existed and is over: a held command that expired or whose task ended.
    #[error("{0}")]
    Gone(String),
    /// The person chose on a record that has since changed.
    #[error("{0}")]
    Stale(String),
    /// A Crew task that already reached `passed` or `stopped` cannot be cancelled.
    #[error("This task already ended.")]
    TaskEnded,
    /// A stored record that fails its own checks, or a read that cannot finish: the answer is an error, never a guess (rule 7).
    #[error("{0}")]
    Unreadable(String),
    /// A refusal that has its own status and stable code, such as an execution profile's `confirm_required`. `detail`
    /// is `null` or a small object a client reads without parsing the message; it never holds a secret or a value
    /// the person typed.
    #[error("{message}")]
    Refused {
        status: u16,
        code: &'static str,
        message: String,
        detail: serde_json::Value,
    },
    #[error("{0}")]
    Internal(String),
    /// A stored row a person can repair (reset it, then enter it again). Server-side data, so `internal` and 500, but unlike
    /// `Internal` the text is public and names the next step; it never carries the row's content.
    #[error("{0}")]
    StoredDataDamaged(String),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_input",
            Self::SettingsConflict => "settings_conflict",
            Self::SettingsScopeUnavailable => "settings_scope_unavailable",
            Self::ThreadBusy => "thread_busy",
            Self::ConsoleOpen => "console_open",
            Self::FolderUntrusted | Self::FolderUntrustedInit => "folder_untrusted",
            Self::AlreadyDecided(_) | Self::QuestionClosed(_) => "already_decided",
            Self::EngineBusy(_) => "engine_busy",
            Self::EngineUnavailable(_) => "engine_unavailable",
            Self::ThreadArchived => "thread_archived",
            Self::ProjectRemoved(_) => "project_removed",
            Self::WorkspaceRequired => "workspace_required",
            Self::WorkspaceRefused(_) => "workspace_refused",
            Self::LibraryDirty(_) => "library_dirty",
            Self::NotFastForward(_) => "not_fast_forward",
            Self::FileConflict { .. } => "file_conflict",
            Self::FileTooLarge { .. } => "file_too_large",
            Self::SourceTooLarge { .. } => "source_too_large",
            Self::SharedDirectory(_) => "shared_directory",
            Self::RunsActive => "runs_active",
            Self::Updating => "updating",
            Self::ProjectHasWork { .. } => "project_has_work",
            Self::PullHeadChanged => "pull_head_changed",
            Self::Unsupported(_) => "unsupported",
            Self::PermissionModeUnsupported(_) => "permission_mode_unsupported",
            Self::RepoExists(_) => "repo_exists",
            Self::RepoUntrusted(_) => "repo_untrusted",
            Self::RepoLimit(_) => "repo_limit",
            Self::CloneRefused(_) => "clone_refused",
            Self::NotGit(_) => "not_git",
            Self::TrackerNotConnected(_) => "tracker_not_connected",
            Self::TrackerRateLimited(_) => "rate_limited",
            Self::TrackerForbidden(_) => "forbidden",
            Self::TrackerUnavailable(_) => "tracker_unavailable",
            Self::WatchUnavailable(_) => "watch_unavailable",
            Self::UnsupportedOption(_) => "unsupported_option",
            Self::UpdateUnavailable(_) => "update_unavailable",
            Self::UpdateFailed(_) => "update_failed",
            Self::PolicyDenied(_) | Self::DiskLow(_) => "policy_denied",
            Self::StaleOwner => "stale_owner",
            Self::RunClosed => "run_closed",
            Self::CrewTaskBusy(_) => "task_busy",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::DatabaseTooNew { .. } => "database_too_new",
            Self::DataDirLocked => "data_dir_locked",
            Self::PackTampered(_) => "pack_tampered",
            Self::AccountDisabled => "account_disabled",
            Self::ReauthRequired => "reauth_required",
            Self::DesktopRequired => "desktop_required",
            Self::PresenceRequired => "presence_required",
            Self::Gone(_) => "gone",
            Self::Stale(_) => "stale",
            Self::TaskEnded => "task_ended",
            Self::Account { code, .. }
            | Self::PatternRefused { code, .. }
            | Self::EngineRefused { code, .. }
            | Self::Refused { code, .. } => code,
            Self::ProjectExists(_) => "project_exists",
            Self::SessionLost => "session_lost",
            Self::Computer(_) => "computer_failed",
            Self::Git(_) => "git_failed",
            Self::Unreadable(_) => "unreadable",
            Self::Store(_)
            | Self::Io(_)
            | Self::Json(_)
            | Self::Internal(_)
            | Self::StoredDataDamaged(_) => "internal",
        }
    }

    pub fn status(&self) -> u16 {
        match self {
            Self::NotFound(_) => 404,
            Self::Updating => 503,
            Self::Invalid(_) | Self::CloneRefused(_) | Self::PermissionModeUnsupported(_) => 400,
            Self::FileTooLarge { .. } | Self::SourceTooLarge { .. } => 413,
            Self::FolderUntrusted
            | Self::FolderUntrustedInit
            | Self::PolicyDenied(_)
            | Self::DiskLow(_)
            | Self::WorkspaceRefused(_)
            | Self::AccountDisabled
            | Self::ReauthRequired
            | Self::DesktopRequired
            | Self::PresenceRequired => 403,
            Self::Account { code, .. } => match *code {
                "account_offline" => 503,
                "rate_limited" => 429,
                "wrong_password" | "invite_required" => 403,
                "invalid_code" | "invalid_invite" | "sign_in_closed" | "weak_password"
                | "invalid_digits" | "wrong_digits" => 422,
                "deletion_scheduled"
                | "session_ended"
                | "account_gone"
                | "request_not_allowed"
                | "digits_locked" => 409,
                _ => 502,
            },
            Self::CrewTaskBusy(_)
            | Self::SettingsConflict
            | Self::SettingsScopeUnavailable
            | Self::ThreadBusy
            | Self::ConsoleOpen
            | Self::AlreadyDecided(_)
            | Self::QuestionClosed(_)
            | Self::ProjectExists(_)
            | Self::ThreadArchived
            | Self::ProjectRemoved(_)
            | Self::WorkspaceRequired
            | Self::LibraryDirty(_)
            | Self::NotFastForward(_)
            | Self::FileConflict { .. }
            | Self::SharedDirectory(_)
            | Self::RunsActive
            | Self::PullHeadChanged
            | Self::TrackerNotConnected(_)
            | Self::RepoExists(_)
            | Self::RepoUntrusted(_)
            | Self::RepoLimit(_)
            | Self::NotGit(_)
            | Self::Git(_)
            | Self::ProjectHasWork { .. }
            | Self::EngineBusy(_)
            | Self::StaleOwner
            | Self::RunClosed
            | Self::TaskEnded
            | Self::Stale(_)
            | Self::EngineRefused { .. }
            | Self::InvalidTransition { .. } => 409,
            Self::Gone(_) => 410,
            Self::Refused { status, .. } => *status,
            Self::UpdateUnavailable(_)
            | Self::Unsupported(_)
            | Self::UnsupportedOption(_)
            | Self::PatternRefused { .. } => 422,
            Self::WatchUnavailable(_) => 503,
            Self::TrackerRateLimited(_) => 429,
            Self::TrackerForbidden(_) => 403,
            Self::EngineUnavailable(_)
            | Self::Computer(_)
            | Self::UpdateFailed(_)
            | Self::TrackerUnavailable(_) => 502,
            _ => 500,
        }
    }

    /// A second copy of this error, for callers that shared one read. Variants that hold a
    /// source error (storage, i/o, encoding) become `Internal` with the same text, which
    /// clients see the same way.
    pub(crate) fn duplicate(&self) -> Self {
        match self {
            Self::NotFound(s) => Self::NotFound(s.clone()),
            Self::Gone(s) => Self::Gone(s.clone()),
            Self::Unreadable(s) => Self::Unreadable(s.clone()),
            Self::Stale(s) => Self::Stale(s.clone()),
            Self::TaskEnded => Self::TaskEnded,
            Self::Invalid(s) => Self::Invalid(s.clone()),
            Self::SettingsConflict => Self::SettingsConflict,
            Self::SettingsScopeUnavailable => Self::SettingsScopeUnavailable,
            Self::ThreadBusy => Self::ThreadBusy,
            Self::CrewTaskBusy(s) => Self::CrewTaskBusy(s.clone()),
            Self::ConsoleOpen => Self::ConsoleOpen,
            Self::FolderUntrusted => Self::FolderUntrusted,
            Self::FolderUntrustedInit => Self::FolderUntrustedInit,
            Self::AlreadyDecided(a) => Self::AlreadyDecided(a.clone()),
            Self::ProjectExists(p) => Self::ProjectExists(p.clone()),
            Self::QuestionClosed(q) => Self::QuestionClosed(q.clone()),
            Self::EngineBusy(s) => Self::EngineBusy(s.clone()),
            Self::EngineUnavailable(s) => Self::EngineUnavailable(s.clone()),
            Self::ThreadArchived => Self::ThreadArchived,
            Self::ProjectRemoved(s) => Self::ProjectRemoved(s.clone()),
            Self::WorkspaceRequired => Self::WorkspaceRequired,
            Self::WorkspaceRefused(s) => Self::WorkspaceRefused(s.clone()),
            Self::LibraryDirty(s) => Self::LibraryDirty(s.clone()),
            Self::NotFastForward(s) => Self::NotFastForward(s.clone()),
            Self::FileConflict { revision } => Self::FileConflict {
                revision: revision.clone(),
            },
            Self::FileTooLarge { size, limit } => Self::FileTooLarge {
                size: *size,
                limit: *limit,
            },
            Self::SourceTooLarge { field, limit } => Self::SourceTooLarge {
                field,
                limit: *limit,
            },
            Self::SharedDirectory(n) => Self::SharedDirectory(*n),
            Self::RunsActive => Self::RunsActive,
            Self::Updating => Self::Updating,
            Self::ProjectHasWork { message, detail } => Self::ProjectHasWork {
                message: message.clone(),
                detail: detail.clone(),
            },
            Self::PullHeadChanged => Self::PullHeadChanged,
            Self::Unsupported(s) => Self::Unsupported(s.clone()),
            Self::PermissionModeUnsupported(s) => Self::PermissionModeUnsupported(s.clone()),
            Self::RepoExists(s) => Self::RepoExists(s.clone()),
            Self::RepoUntrusted(s) => Self::RepoUntrusted(s.clone()),
            Self::RepoLimit(s) => Self::RepoLimit(s.clone()),
            Self::CloneRefused(s) => Self::CloneRefused(s.clone()),
            Self::NotGit(s) => Self::NotGit(s.clone()),
            Self::TrackerNotConnected(s) => Self::TrackerNotConnected(s.clone()),
            Self::TrackerRateLimited(s) => Self::TrackerRateLimited(s.clone()),
            Self::TrackerForbidden(s) => Self::TrackerForbidden(s.clone()),
            Self::TrackerUnavailable(s) => Self::TrackerUnavailable(s.clone()),
            Self::WatchUnavailable(s) => Self::WatchUnavailable(s.clone()),
            Self::UnsupportedOption(s) => Self::UnsupportedOption(s.clone()),
            Self::UpdateUnavailable(s) => Self::UpdateUnavailable(s.clone()),
            Self::UpdateFailed(s) => Self::UpdateFailed(s.clone()),
            Self::PolicyDenied(s) => Self::PolicyDenied(s.clone()),
            Self::DiskLow(s) => Self::DiskLow(s.clone()),
            Self::SessionLost => Self::SessionLost,
            Self::StaleOwner => Self::StaleOwner,
            Self::RunClosed => Self::RunClosed,
            Self::InvalidTransition { from, to } => Self::InvalidTransition {
                from: from.clone(),
                to: to.clone(),
            },
            Self::DatabaseTooNew { found, supported } => Self::DatabaseTooNew {
                found: *found,
                supported: *supported,
            },
            Self::DataDirLocked => Self::DataDirLocked,
            Self::AccountDisabled => Self::AccountDisabled,
            Self::ReauthRequired => Self::ReauthRequired,
            Self::DesktopRequired => Self::DesktopRequired,
            Self::PresenceRequired => Self::PresenceRequired,
            Self::Account {
                code,
                message,
                retry_after_secs,
                tries_left,
            } => Self::Account {
                code,
                message: message.clone(),
                retry_after_secs: *retry_after_secs,
                tries_left: *tries_left,
            },
            Self::PatternRefused {
                code,
                message,
                detail,
            } => Self::PatternRefused {
                code,
                message: message.clone(),
                detail: detail.clone(),
            },
            Self::EngineRefused {
                code,
                message,
                fields,
            } => Self::EngineRefused {
                code,
                message: message.clone(),
                fields: fields.clone(),
            },
            Self::Refused {
                status,
                code,
                message,
                detail,
            } => Self::Refused {
                status: *status,
                code,
                message: message.clone(),
                detail: detail.clone(),
            },
            Self::PackTampered(s) => Self::PackTampered(s.clone()),
            Self::Computer(s) => Self::Computer(s.clone()),
            Self::Git(s) => Self::Git(s.clone()),
            Self::Internal(s) => Self::Internal(s.clone()),
            Self::StoredDataDamaged(s) => Self::StoredDataDamaged(s.clone()),
            Self::Store(_) | Self::Io(_) | Self::Json(_) => Self::Internal(self.to_string()),
        }
    }

    /// Message safe to show a client. Storage and I/O detail stays in the log.
    pub fn public_message(&self) -> String {
        match self {
            Self::Store(_) | Self::Io(_) | Self::Json(_) | Self::Internal(_) => {
                "Orbyn hit an internal error. Check the service log.".to_owned()
            }
            other => other.to_string(),
        }
    }
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;
