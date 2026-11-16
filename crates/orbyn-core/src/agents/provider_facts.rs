//! The state facts of one provider, read from its status in one place. The router's hard
//! filters (docs/crew.md, Router v0 step 1) and the Providers list both ask this type for
//! not found, signed out, Can't start (a broken version) and Unmanaged. A sign-in that could
//! not be confirmed has no label among the designed states, so the list shows none for it
//! while the router still refuses. Nothing here starts a process or reads a file.

use orbyn_contracts::{
    AuthState, CompatibilityStatus, EngineCapabilities, EngineStatus, ProviderLabel,
};

/// Why an engine cannot be used at all, in the order the router checks them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unavailable {
    NotInstalled,
    SignedOut,
    SignInUnconfirmed,
    /// The signed manifest names the installed version as known-incompatible.
    Broken(String),
}

/// Why an engine cannot take a writing brief.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteBlock {
    /// Orbyn cannot gate its actions.
    NotManaged,
    /// It cannot ask for approval.
    NoApprovals,
}

#[derive(Clone, Debug)]
pub struct ProviderFacts {
    installed: bool,
    auth: AuthState,
    broken: Option<String>,
    limited: Option<String>,
    managed: bool,
    approvals: bool,
}

impl ProviderFacts {
    pub fn of(status: &EngineStatus) -> Self {
        let warning = |wanted: fn(CompatibilityStatus) -> bool| {
            status
                .compatibility
                .as_ref()
                .filter(|found| wanted(found.status))
                .map(|found| found.message.clone())
        };
        Self::from_parts(
            status.installed,
            status.auth,
            warning(|state| state == CompatibilityStatus::Broken),
            warning(|state| state != CompatibilityStatus::Broken),
            &status.capabilities,
        )
    }

    pub fn from_parts(
        installed: bool,
        auth: AuthState,
        broken: Option<String>,
        limited: Option<String>,
        capabilities: &EngineCapabilities,
    ) -> Self {
        Self {
            installed,
            auth,
            broken,
            limited,
            managed: capabilities.managed,
            approvals: capabilities.approvals,
        }
    }

    pub fn installed(&self) -> bool {
        self.installed
    }

    /// The manifest's reason, when the installed version is known to be broken.
    pub fn broken_note(&self) -> Option<&str> {
        self.broken.as_deref()
    }

    /// The first reason this engine cannot be used, or `None`.
    pub fn unavailable(&self) -> Option<Unavailable> {
        if !self.installed {
            return Some(Unavailable::NotInstalled);
        }
        match self.auth {
            AuthState::LoggedIn | AuthState::NotApplicable => {}
            AuthState::LoggedOut => return Some(Unavailable::SignedOut),
            AuthState::Unknown => return Some(Unavailable::SignInUnconfirmed),
        }
        self.broken.clone().map(Unavailable::Broken)
    }

    /// Why a brief that writes cannot go to this engine, or `None`.
    pub fn write_block(&self) -> Option<WriteBlock> {
        if !self.managed {
            Some(WriteBlock::NotManaged)
        } else if !self.approvals {
            Some(WriteBlock::NoApprovals)
        } else {
            None
        }
    }

    /// The manifest's note on a version that runs with less than Orbyn offers. It never
    /// keeps the router from choosing the engine.
    pub fn limited_note(&self) -> Option<&str> {
        self.limited.as_deref()
    }

    /// The labels these facts alone give, in the order the list draws them. A label that
    /// needs more than a status, such as Not verified, is added by the list.
    pub fn labels(&self) -> Vec<ProviderLabel> {
        let mut labels = Vec::new();
        if !self.installed {
            labels.push(ProviderLabel::NotFound);
        }
        if self.installed && self.broken.is_some() {
            labels.push(ProviderLabel::CantStart);
        }
        if self.installed && self.write_block().is_some() {
            labels.push(ProviderLabel::Unmanaged);
        }
        if self.installed && self.broken.is_none() && self.limited.is_some() {
            labels.push(ProviderLabel::LimitedSupport);
        }
        labels
    }
}
