//! Sending a sign-in cancel again when no answer came back.
//!
//! **Why under the client.** `DeviceClient::deny_joined` takes the `JoinedApproval`, so once a
//! cancel fails the account module has nothing left to ask with and a second cancel only lets go
//! locally (issue #758). The client offers its transport for a host to wrap (`DeviceClient::api`,
//! "for a host that wraps it"), so the attempt that is given up on is sent again one layer lower:
//! one cancel on the screen makes the bounded attempts itself, without a handle.
//!
//! **Why a cancel may be sent twice.** The Engine's cancel is terminal and repeatable, so a
//! repeated one cannot toggle an approval. At the pinned `engine-v0.2.4`: the route
//! `POST /v1/devices/approvals/{id}/cancel` (`orbyn-server/src/accounts/mod.rs`) calls
//! `cancel_approval`, which reads the row with `approval_row` â€” the account and the device, never
//! its state â€” and then `close`, an unconditional `UPDATE device_approvals SET state = 'closed'
//! WHERE id = $1` (`orbyn-core/src/accounts/devices.rs`). Nothing in the Engine ever writes a
//! closed approval back to `open`, so a second cancel writes the same terminal state again and
//! answers 200: one transition at most, however many attempts arrive.
//!
//! **What is never sent again.** Only `POST` to an approval cancel path, and only after
//! [`orbyn_device_client::DeviceError::Transport`], the one error that means no answer came back.
//! Everything else, and every other request, is answered once: a sign-in, a code or a confirm is
//! not idempotent, and a failure the Engine did answer must reach the person unchanged.
//!
//! The wait is `tokio::time::sleep`, so Windows and macOS run the same code: no signal, no
//! platform timer.

use std::time::Duration;

use orbyn_device_client as dc;

/// The attempts a cancel is worth, the first one included. Bounded: two retries and no more.
const CANCEL_TRIES: usize = 3;

/// How long to wait before the second attempt. It doubles before the third and stops there.
const CANCEL_PAUSE: Duration = Duration::from_millis(250);

/// The longest wait between two attempts, whatever the doubling reaches.
const CANCEL_PAUSE_MAX: Duration = Duration::from_secs(1);

/// The Engine's transport, with the cancel sent again after no answer.
pub(crate) struct Retrying<T> {
    inner: T,
    tries: usize,
    pause: Duration,
}

impl<T> Retrying<T> {
    /// The policy every real client uses.
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            tries: CANCEL_TRIES,
            pause: CANCEL_PAUSE,
        }
    }

    /// [`Retrying::new`] with a shorter wait, so a test does not sleep for the real one. The
    /// behaviour is the same; only the wait between attempts changes.
    #[cfg(test)]
    pub(crate) fn with_pause(inner: T, pause: Duration) -> Self {
        Self {
            inner,
            tries: CANCEL_TRIES,
            pause,
        }
    }
}

/// Whether this request is the one the Engine may be asked twice: an approval cancel, either
/// device's. Its id is percent-encoded by the client and no id ends in `/cancel`, so the shape is
/// the whole route.
fn is_cancel(request: &dc::Request) -> bool {
    request.method == dc::Method::Post
        && request.path.starts_with("/v1/devices/approvals/")
        && request.path.ends_with("/cancel")
}

impl<T: dc::DeviceApi> dc::DeviceApi for Retrying<T> {
    fn base_url(&self) -> Option<&str> {
        self.inner.base_url()
    }

    async fn send(&self, request: dc::Request) -> Result<dc::Response, dc::DeviceError> {
        let again = is_cancel(&request);
        // The request is rebuilt for each attempt: the body is wiped on drop, so it is kept in its
        // wiping holder and copied per attempt instead of being borrowed twice.
        let dc::Request {
            method,
            path,
            bearer,
            body,
        } = request;
        let mut attempt = 1;
        let mut pause = self.pause;
        loop {
            let result = self
                .inner
                .send(dc::Request {
                    method,
                    path: path.clone(),
                    bearer: bearer.clone(),
                    body: body.clone(),
                })
                .await;
            // The one case worth another attempt: a cancel the network lost, and attempts are
            // left. A cancel that was answered is not asked again.
            let lost = matches!(result, Err(dc::DeviceError::Transport));
            if !again || !lost || attempt >= self.tries {
                return result;
            }
            tokio::time::sleep(pause).await;
            pause = (pause * 2).min(CANCEL_PAUSE_MAX);
            attempt += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use dc::DeviceApi as _;

    use super::*;

    fn lock<T>(held: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        held.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The answers requests get, in order, and what the transport was asked.
    #[derive(Default)]
    struct Scripted {
        answers: Mutex<VecDeque<Result<u16, dc::DeviceError>>>,
        asked: Mutex<Vec<String>>,
    }

    impl Scripted {
        fn of(answers: Vec<Result<u16, dc::DeviceError>>) -> Arc<Self> {
            let scripted = Self::default();
            lock(&scripted.answers).extend(answers);
            Arc::new(scripted)
        }

        fn asked(&self) -> usize {
            lock(&self.asked).len()
        }
    }

    /// The transport: one scripted answer per request, in order.
    struct Double(Arc<Scripted>);

    impl dc::DeviceApi for Double {
        async fn send(&self, request: dc::Request) -> Result<dc::Response, dc::DeviceError> {
            let next = lock(&self.0.answers).pop_front();
            lock(&self.0.asked).push(format!("{:?} {}", request.method, request.path));
            match next {
                Some(Ok(status)) => Ok(dc::Response {
                    status,
                    retry_after: None,
                    body: dc::Zeroizing::new(Vec::new()),
                }),
                Some(Err(error)) => Err(error),
                // A request sent with no answer left for it: only an unbounded retry asks that
                // often, and a test says so by its attempt count.
                None => Err(dc::DeviceError::Transport),
            }
        }
    }

    fn cancel() -> dc::Request {
        dc::Request {
            method: dc::Method::Post,
            path: "/v1/devices/approvals/0a0a0a0a-0000-4000-8000-0000000000a1/cancel".to_owned(),
            bearer: None,
            body: None,
        }
    }

    fn retrying(scripted: &Arc<Scripted>) -> Retrying<Double> {
        Retrying::with_pause(Double(scripted.clone()), Duration::from_millis(1))
    }

    #[tokio::test]
    async fn a_lost_cancel_is_sent_again_and_its_answer_is_the_one_returned() {
        let scripted = Scripted::of(vec![Err(dc::DeviceError::Transport), Ok(200)]);
        let answer = retrying(&scripted).send(cancel()).await;
        assert_eq!(answer.map(|response| response.status), Ok(200));
        assert_eq!(scripted.asked(), 2, "one lost attempt, then the answer");
    }

    #[tokio::test]
    async fn the_attempts_are_bounded_and_the_network_failure_is_what_comes_back() {
        let scripted = Scripted::of(vec![
            Err(dc::DeviceError::Transport),
            Err(dc::DeviceError::Transport),
            Err(dc::DeviceError::Transport),
            // A fourth answer would be taken only if the retry were unbounded.
            Ok(200),
        ]);
        let answer = retrying(&scripted).send(cancel()).await;
        assert_eq!(answer.err(), Some(dc::DeviceError::Transport));
        assert_eq!(scripted.asked(), CANCEL_TRIES, "three attempts, never more");
    }

    #[tokio::test]
    async fn a_request_that_is_not_a_cancel_is_never_sent_twice() {
        for (method, path) in [
            // A sign-in POST is not repeatable.
            (dc::Method::Post, "/v1/devices/sign-in"),
            // Nor is a read, even of the cancel's own path.
            (
                dc::Method::Get,
                "/v1/devices/approvals/0a0a0a0a-0000-4000-8000-0000000000a1/cancel",
            ),
        ] {
            let scripted = Scripted::of(vec![Err(dc::DeviceError::Transport), Ok(200)]);
            let answer = retrying(&scripted)
                .send(dc::Request {
                    method,
                    path: path.to_owned(),
                    bearer: None,
                    body: None,
                })
                .await;
            assert_eq!(
                answer.err(),
                Some(dc::DeviceError::Transport),
                "{path} is not sent again"
            );
            assert_eq!(scripted.asked(), 1, "{path} was sent once");
        }
    }

    #[tokio::test]
    async fn an_answer_the_engine_gave_is_never_asked_again() {
        // A 500 is an answer: the Engine was reached, so the person hears what it said, and the
        // cancel is not sent on top of it.
        let scripted = Scripted::of(vec![Ok(500), Ok(200)]);
        let answer = retrying(&scripted).send(cancel()).await;
        assert_eq!(answer.map(|response| response.status), Ok(500));
        assert_eq!(scripted.asked(), 1);
    }

    #[test]
    fn the_policy_shipped_is_three_attempts_and_a_short_growing_wait() {
        // Guards the numbers the product ships: a cancel costs at most two retries, and the whole
        // wait stays under a second.
        let policy = Retrying::new(());
        assert_eq!(policy.tries, CANCEL_TRIES);
        assert_eq!(policy.tries, 3);
        assert_eq!(policy.pause, CANCEL_PAUSE);
        assert_eq!(CANCEL_PAUSE, Duration::from_millis(250));
        assert_eq!(CANCEL_PAUSE_MAX, Duration::from_secs(1));
    }
}
