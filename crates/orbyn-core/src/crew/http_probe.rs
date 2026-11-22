//! The `http_probe` check kind: one GET to a loopback URL, compared with the status the answer
//! key expects (docs/crew.md, "Verification without limits"). The URL's own type allows only
//! `http://` on `127.0.0.1`, `localhost` or `[::1]`, so a probe never leaves the machine.

use std::time::{Duration, Instant};

use orbyn_contracts::{CheckOutcome, HttpProbeParams};

use super::checks::{Ran, ran};

/// Runs one `http_probe`: one request, no redirect followed, no retry, inside `allowed`.
///
/// The status the URL answers is the whole check: the expected one passes, another is a finding.
/// A request that never got an answer (refused, timed out, a broken connection) is an error: it
/// observed nothing, so it never passes and never fails on a guess.
///
/// `allowed` is the budget every check already gets (`checks::TIMEOUT`, cut to what is left of
/// the task's own). Nothing here picks a shorter probe timeout of its own: that number is the
/// answer key author's decision, not this runner's.
pub(super) async fn probe(
    params: &HttpProbeParams,
    allowed: Duration,
    description: String,
    started: Instant,
) -> Ran {
    if allowed.is_zero() {
        let why = "The task's time budget ran out before this check could run.".to_owned();
        return unobserved(description, why.clone(), why, started);
    }
    let client = match reqwest::Client::builder()
        .timeout(allowed)
        // A redirect is never followed: the status compared is the one this URL answered, so a
        // page cannot hand the probe to another path or host.
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(why) => {
            let reason = "The probe's HTTP client could not be built.".to_owned();
            return unobserved(description, reason, causes(&why), started);
        }
    };
    let url = params.url.as_str();
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(why) => {
            let reason = if why.is_timeout() {
                "The probe ran out of time and was stopped."
            } else {
                "The probe could not reach the URL."
            };
            return unobserved(description, reason.to_owned(), causes(&why), started);
        }
    };
    let status = response.status().as_u16();
    let expected = params.expect_status;
    let log = format!("GET {url} answered {status}.");
    if status == expected {
        return ran(CheckOutcome::Pass, description, log, started);
    }
    let mut result = ran(CheckOutcome::Fail, description, log, started);
    // What failed, by name: the status the URL answered, so a changed answer is a new
    // fingerprint and a repeat of the same one is the same failure.
    result.names.push(format!("status: {status}"));
    result.reason = Some(format!(
        "The probe expected status {expected}, and the URL answered {status}."
    ));
    result
}

/// A probe that observed nothing: an error, never a pass, with a reason a worker may read.
fn unobserved(description: String, reason: String, log: String, started: Instant) -> Ran {
    let mut result = ran(CheckOutcome::Error, description, log, started);
    result.reason = Some(reason);
    result
}

/// The error and every cause under it. `reqwest`'s own words are only "error sending request
/// for url (...)"; without the cause a log cannot tell a refused connection from a broken one.
fn causes(error: &reqwest::Error) -> String {
    let mut text = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}
