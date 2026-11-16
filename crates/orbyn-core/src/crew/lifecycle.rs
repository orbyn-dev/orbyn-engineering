//! The task lifecycle and the loop guard (SPEC.md section 6), as pure functions. The store
//! applies them in a transaction; nothing here touches a file, a process or a clock.

/// Every state a task can be in, as stored.
#[cfg(test)]
pub const STATES: [&str; 10] = [
    "briefed",
    "routed",
    "running",
    "verifying",
    "failed",
    "changed_approach",
    "needs_person",
    "passed",
    "stopped",
    "cancelled",
];

/// The states a task can move to from `from`. A final state has none.
pub fn next_states(from: &str) -> &'static [&'static str] {
    match from {
        "briefed" => &["routed", "needs_person", "cancelled"],
        "routed" => &["running", "needs_person", "cancelled"],
        "running" => &["verifying", "needs_person", "stopped", "cancelled"],
        "verifying" => &["passed", "failed", "needs_person", "stopped", "cancelled"],
        "failed" => &["running", "changed_approach", "stopped", "cancelled"],
        "changed_approach" => &["running", "stopped", "cancelled"],
        "needs_person" => &["running", "routed", "stopped", "cancelled"],
        _ => &[],
    }
}

pub fn allowed(from: &str, to: &str) -> bool {
    next_states(from).contains(&to)
}

/// `passed`, `stopped` and `cancelled` end the run. `failed` and `needs_person` do not.
pub fn is_final(state: &str) -> bool {
    matches!(state, "passed" | "stopped" | "cancelled")
}

/// What the guard remembers between failures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Guard {
    /// Consecutive failures with the same fingerprint.
    pub streak: u32,
    pub fingerprint: Option<String>,
    /// The one change of approach has been used.
    pub approach_changed: bool,
}

/// The state the guard names after a failed verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// Back to `running`, with the failure as the reason.
    Retry,
    /// `changed_approach`: the worker is told to try another way, once.
    ChangeApproach,
    /// `stopped` with `repeated_failure`.
    Stop,
}

impl Next {
    pub fn state(self) -> &'static str {
        match self {
            Self::Retry => "running",
            Self::ChangeApproach => "changed_approach",
            Self::Stop => "stopped",
        }
    }
}

impl Guard {
    /// Applies one failed verification: a different fingerprint starts the count again at
    /// one; at one the worker retries; at two or more the approach changes once; at two or
    /// more after that the run stops.
    pub fn after_failure(&self, fingerprint: &str) -> (Self, Next) {
        let streak = if self.fingerprint.as_deref() == Some(fingerprint) {
            self.streak + 1
        } else {
            1
        };
        let (approach_changed, next) = if streak < 2 {
            (self.approach_changed, Next::Retry)
        } else if !self.approach_changed {
            (true, Next::ChangeApproach)
        } else {
            (true, Next::Stop)
        };
        (
            Self {
                streak,
                fingerprint: Some(fingerprint.to_owned()),
                approach_changed,
            },
            next,
        )
    }
}
