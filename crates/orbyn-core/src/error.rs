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
    /// A model, effort or option the chosen model does not offer.
    #[error("{0}")]
    UnsupportedOption(String),
    #[error("{0}")]
    UpdateUnavailable(String),
    #[error("{0}")]
    UpdateFailed(String),
    #[error("{0}")]
    PolicyDenied(String),
    /// The engine does not have the session a run tried to resume.
    #[error("the engine no longer has this session")]
    SessionLost,
    /// A write from a process that no longer owns the run.
    #[error("run is owned by another process")]
    StaleOwner,
    /// A write against a run that already reached a terminal state.
    #[error("run has ended")]
    RunClosed,
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
    #[error("{0}")]
    Internal(String),
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
            Self::FolderUntrusted => "folder_untrusted",
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
            Self::SharedDirectory(_) => "shared_directory",
            Self::RunsActive => "runs_active",
            Self::Updating => "updating",
            Self::ProjectHasWork { .. } => "project_has_work",
            Self::PullHeadChanged => "pull_head_changed",
            Self::Unsupported(_) => "unsupported",
            Self::UnsupportedOption(_) => "unsupported_option",
            Self::UpdateUnavailable(_) => "update_unavailable",
            Self::UpdateFailed(_) => "update_failed",
            Self::PolicyDenied(_) => "policy_denied",
            Self::StaleOwner => "stale_owner",
            Self::RunClosed => "run_closed",
            Self::InvalidTransition { .. } => "invalid_transition",
            Self::DatabaseTooNew { .. } => "database_too_new",
            Self::DataDirLocked => "data_dir_locked",
            Self::PackTampered(_) => "pack_tampered",
            Self::AccountDisabled => "account_disabled",
            Self::ReauthRequired => "reauth_required",
            Self::Account { code, .. } | Self::PatternRefused { code, .. } => code,
            Self::ProjectExists(_) => "project_exists",
            Self::SessionLost => "session_lost",
            Self::Computer(_) => "computer_failed",
            Self::Git(_) => "git_failed",
            Self::Store(_) | Self::Io(_) | Self::Json(_) | Self::Internal(_) => "internal",
        }
    }

    pub fn status(&self) -> u16 {
        match self {
            Self::NotFound(_) => 404,
            Self::Updating => 503,
            Self::Invalid(_) => 400,
            Self::FileTooLarge { .. } => 413,
            Self::FolderUntrusted
            | Self::PolicyDenied(_)
            | Self::WorkspaceRefused(_)
            | Self::AccountDisabled
            | Self::ReauthRequired => 403,
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
            Self::SettingsConflict
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
            | Self::Git(_)
            | Self::ProjectHasWork { .. }
            | Self::EngineBusy(_)
            | Self::StaleOwner
            | Self::RunClosed
            | Self::InvalidTransition { .. } => 409,
            Self::UpdateUnavailable(_)
            | Self::Unsupported(_)
            | Self::UnsupportedOption(_)
            | Self::PatternRefused { .. } => 422,
            Self::EngineUnavailable(_) | Self::Computer(_) | Self::UpdateFailed(_) => 502,
            _ => 500,
        }
    }

    /// A second copy of this error, for callers that shared one read. Variants that hold a
    /// source error (storage, i/o, encoding) become `Internal` with the same text, which
    /// clients see the same way.
    pub(crate) fn duplicate(&self) -> Self {
        match self {
            Self::NotFound(s) => Self::NotFound(s.clone()),
            Self::Invalid(s) => Self::Invalid(s.clone()),
            Self::SettingsConflict => Self::SettingsConflict,
            Self::SettingsScopeUnavailable => Self::SettingsScopeUnavailable,
            Self::ThreadBusy => Self::ThreadBusy,
            Self::ConsoleOpen => Self::ConsoleOpen,
            Self::FolderUntrusted => Self::FolderUntrusted,
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
            Self::SharedDirectory(n) => Self::SharedDirectory(*n),
            Self::RunsActive => Self::RunsActive,
            Self::Updating => Self::Updating,
            Self::ProjectHasWork { message, detail } => Self::ProjectHasWork {
                message: message.clone(),
                detail: detail.clone(),
            },
            Self::PullHeadChanged => Self::PullHeadChanged,
            Self::Unsupported(s) => Self::Unsupported(s.clone()),
            Self::UnsupportedOption(s) => Self::UnsupportedOption(s.clone()),
            Self::UpdateUnavailable(s) => Self::UpdateUnavailable(s.clone()),
            Self::UpdateFailed(s) => Self::UpdateFailed(s.clone()),
            Self::PolicyDenied(s) => Self::PolicyDenied(s.clone()),
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
            Self::PackTampered(s) => Self::PackTampered(s.clone()),
            Self::Computer(s) => Self::Computer(s.clone()),
            Self::Git(s) => Self::Git(s.clone()),
            Self::Internal(s) => Self::Internal(s.clone()),
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
