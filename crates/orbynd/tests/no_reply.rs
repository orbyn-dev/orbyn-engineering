//! A turn the engine ends without saying or doing anything is not a success: the run fails with a
//! plain "No reply", so the person sends the message again. A command may have nothing to say.

mod support;

use support::{Options, Service, TempDir, setup};

async fn last_event(service: &Service, thread: &str) -> serde_json::Value {
    service
        .all_events(thread)
        .await
        .last()
        .expect("events")
        .clone()
}

#[tokio::test]
async fn an_ordinary_turn_with_no_output_fails_as_no_reply() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.admit(&thread, "go [silent]").await;
    assert_eq!(service.finished(&thread).await, "failed");
    let last = last_event(&service, &thread).await;
    assert!(
        last["event"]["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("No reply")),
        "{last}"
    );
}

#[tokio::test]
async fn a_turn_that_answered_still_succeeds() {
    let dir = TempDir::new();
    let (service, thread) = setup(&dir, Options::default(), "claude").await;
    service.admit(&thread, "go").await;
    assert_eq!(service.finished(&thread).await, "completed");
}
