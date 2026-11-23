//! `runDone` to `EngineOutcome`, and the sentence a person reads when the host stops.

use super::frames::{ErrorKind, HostError, Reason, RetryHint, RunDone};
use crate::agents::driver::{
    EngineEvent, EngineOutcome, Failure, RetryInfo, RetryWhy, ThreadDisposition,
};

/// What every run on a host that stopped is told.
pub const HOST_STOPPED: &str =
    "The engine host stopped. Your work in the project is kept; send your message again.";

/// The stable code a stopped host gives the run, so a client never has to match the sentence.
pub const HOST_STOPPED_CODE: &str = "engine_host_stopped";

fn disposition(text: Option<&str>) -> ThreadDisposition {
    match text {
        Some("reusable") => ThreadDisposition::Reusable,
        Some("broken") => ThreadDisposition::Broken,
        _ => ThreadDisposition::Unknown,
    }
}

fn retry(hint: &RetryHint) -> Option<RetryInfo> {
    let why = match hint.why.as_deref()? {
        "rate_limit" => RetryWhy::RateLimit,
        "network" => RetryWhy::Network,
        "overloaded" => RetryWhy::Overloaded,
        _ => return None,
    };
    Some(RetryInfo {
        after_ms: hint.after_ms,
        why,
    })
}

/// The failure of a run the host lost: the same outcome for every open run.
pub fn host_stopped() -> EngineOutcome {
    EngineOutcome::FailedWith(Failure {
        message: HOST_STOPPED.into(),
        retry: None,
        disposition: ThreadDisposition::Unknown,
        code: Some(HOST_STOPPED_CODE),
    })
}

/// A plain sentence for a failed run that has no message of its own.
fn sentence(reason: Option<Reason>, error: Option<&HostError>) -> String {
    if let Some(text) = error.and_then(HostError::message_text) {
        return text.to_owned();
    }
    match reason {
        Some(Reason::MaxTurns) => "The engine stopped because it reached its turn limit.".into(),
        Some(Reason::Refused) => "The engine declined to continue this request.".into(),
        Some(Reason::Other) => "The engine ended the turn in a way this app does not know.".into(),
        _ => "The engine could not finish this turn.".into(),
    }
}

/// The outcome of a `runDone` for a family, and the events that go with it. ACP's
/// `auth` failure is a sign-in to the agent, said in the core's own words.
pub fn map_family(family: &str, done: &RunDone) -> (EngineOutcome, Vec<EngineEvent>) {
    let (outcome, events) = map(done);
    if family != "acp" {
        return (outcome, events);
    }
    let signed_out = done
        .error
        .as_ref()
        .is_some_and(|error| error.effective_kind() == ErrorKind::Auth);
    match outcome {
        EngineOutcome::FailedWith(mut failure) if signed_out => {
            failure.message = super::acp::auth_sentence();
            (EngineOutcome::FailedWith(failure), events)
        }
        other => (other, events),
    }
}

/// The outcome of a `runDone`, and the events that go with it (a rate limit's reset time).
/// Called once per run: the caller drops the route after the first.
pub fn map(done: &RunDone) -> (EngineOutcome, Vec<EngineEvent>) {
    let mut events = Vec::new();
    let hint = done.retry.as_ref();
    if let Some(hint) = hint
        && hint.why.as_deref() == Some("rate_limit")
        && let Some(resets_at) = hint.resets_at
    {
        events.push(EngineEvent::Limited {
            resets_at: Some(resets_at),
        });
    }
    let kind = done
        .error
        .as_ref()
        .map_or(ErrorKind::Unknown, HostError::effective_kind);
    let failed = |reason: Option<Reason>| {
        EngineOutcome::FailedWith(Failure {
            message: sentence(reason, done.error.as_ref()),
            retry: hint.and_then(retry),
            disposition: disposition(done.disposition.as_deref()),
            code: None,
        })
    };
    let outcome = if kind == ErrorKind::SessionLost {
        EngineOutcome::SessionLost
    } else if done.ok {
        match done.reason {
            Some(Reason::Cancelled) => EngineOutcome::Cancelled,
            // A host that says ok with no reason completed the turn.
            Some(Reason::Completed) | None => EngineOutcome::Completed,
            other => failed(other),
        }
    } else if kind == ErrorKind::Cancelled || done.reason == Some(Reason::Cancelled) {
        EngineOutcome::Cancelled
    } else {
        failed(done.reason)
    };
    (outcome, events)
}

#[cfg(test)]
#[path = "outcome_tests.rs"]
mod tests;
