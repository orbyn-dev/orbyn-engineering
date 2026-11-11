//! The account routes that let a new device in, join settings, and bind a confirmation to the window
//! that typed the password (the nonce). Split from `account_tests.rs`, whose server it shares.

use orbyn_core::account::AccountGate;
use serde_json::{Value, json};

use super::account_tests::{SECRET, not_the_nonce, serve};

#[tokio::test]
async fn a_caller_holding_only_the_service_token_cannot_use_a_confirmation_it_did_not_obtain() {
    let served = serve("engine", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    // The window types the password and keeps the nonce; an engine with the same token never sees it.
    let nonce = served.nonce_for("sign_out_everywhere", None).await;
    let path = "/api/account/sign-out-everywhere";
    let guesses = std::iter::once(None).chain(not_the_nonce(&nonce).into_iter().map(Some));
    for guess in guesses {
        let (status, body) = served
            .call_with("POST", path, json!({}), guess.as_deref())
            .await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required")),
            "{guess:?}"
        );
    }
    // The failed guesses did not use the confirmation up.
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert!(!view["session"].is_null());
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&nonce))
            .await
            .0,
        204
    );
}

#[tokio::test]
async fn cancelling_drops_the_confirmation_and_only_its_own_nonce_can() {
    let served = serve("cancel", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let path = "/api/account/sign-out-everywhere";
    let nonce = served.nonce_for("sign_out_everywhere", None).await;
    // A wrong nonce cancels nothing.
    let cancel = |nonce: String| {
        served.call(
            "POST",
            "/api/account/confirm/cancel",
            json!({ "nonce": nonce }),
        )
    };
    for wrong in not_the_nonce(&nonce) {
        assert_eq!(cancel(wrong).await.0, 204);
    }
    let again = served.nonce_for("sign_out_everywhere", None).await;
    assert_eq!(cancel(again.clone()).await.0, 204);
    // The abandoned prompt's confirmation is gone, and so is the one it replaced.
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&again))
            .await
            .0,
        403
    );
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&nonce))
            .await
            .0,
        403
    );
    // The wrong cancel left a live one alone.
    let live = served.nonce_for("sign_out_everywhere", None).await;
    for wrong in not_the_nonce(&live) {
        assert_eq!(cancel(wrong).await.0, 204);
    }
    assert_eq!(
        served
            .call_with("POST", path, json!({}), Some(&live))
            .await
            .0,
        204
    );
}

#[tokio::test]
async fn letting_a_new_device_in_needs_the_password_for_that_request_and_is_used_up() {
    let served = serve("approve", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let (status, list) = served
        .call("GET", "/api/account/approvals", json!({}))
        .await;
    assert_eq!(status, 200);
    assert_eq!(list[0]["digits"], "418 207");
    assert_eq!(list[0]["device_name"], "iPad");
    let approve = "/api/account/approvals/req-ipad/approve";
    let refused = |(status, body): (u16, Value)| {
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required"))
        );
    };
    // Signing in did not confirm anything.
    refused(served.call("POST", approve, json!({})).await);
    // A confirmation for another action or another request does not count, with its own nonce.
    let other_action = served.nonce_for("sign_out_everywhere", None).await;
    refused(
        served
            .call_with("POST", approve, json!({}), Some(&other_action))
            .await,
    );
    let other_request = served.nonce_for("approve_device", Some("req-other")).await;
    refused(
        served
            .call_with("POST", approve, json!({}), Some(&other_request))
            .await,
    );
    assert_eq!(served.confirm("approve_device", None).await.0, 400);
    let nonce = served.nonce_for("approve_device", Some("req-ipad")).await;
    refused(served.call("POST", approve, json!({})).await);
    assert_eq!(
        served
            .call_with("POST", approve, json!({}), Some(&nonce))
            .await
            .0,
        204
    );
    let (_, list) = served
        .call("GET", "/api/account/approvals", json!({}))
        .await;
    assert_eq!(list.as_array().map(Vec::len), Some(0));
    // Used up: the same confirmation does not let a second request in.
    refused(
        served
            .call_with("POST", approve, json!({}), Some(&nonce))
            .await,
    );
}

#[tokio::test]
async fn refusing_a_new_device_needs_no_password_and_lets_nobody_in() {
    let served = serve("deny", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let deny = "/api/account/approvals/req-ipad/deny";
    assert_eq!(
        served
            .call("POST", deny, json!({ "codes_differ": true }))
            .await
            .0,
        204
    );
    let (_, list) = served
        .call("GET", "/api/account/approvals", json!({}))
        .await;
    assert_eq!(list.as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn a_new_device_waits_for_a_trusted_one_and_can_give_up() {
    let served = serve("waiting", AccountGate::LocalOnly, false).await;
    let (status, body) = served
        .call(
            "POST",
            "/api/account/sign-in",
            json!({ "email": "late@example.com", "password": SECRET }),
        )
        .await;
    assert_eq!(
        (status, body["outcome"].as_str()),
        (200, Some("awaiting_approval"))
    );
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert_eq!(view["approval"]["state"], "waiting");
    assert_eq!(view["approval"]["digits"], "418 207");
    assert!(view["session"].is_null());
    let (status, polled) = served
        .call("POST", "/api/account/sign-in/approval", json!({}))
        .await;
    assert_eq!((status, polled["state"].as_str()), (200, Some("waiting")));
    assert_eq!(
        served
            .call("POST", "/api/account/sign-in/cancel", json!({}))
            .await
            .0,
        204
    );
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert!(view.get("approval").is_none());
}

#[tokio::test]
async fn the_first_sync_asks_how_to_join_the_settings_once() {
    let served = serve("connect", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    let (_, view) = served.call("GET", "/api/account", json!({})).await;
    assert_eq!(
        view["connect"], "connected",
        "a computer with no settings of its own has nothing to join"
    );
    assert_eq!(
        served
            .call("POST", "/api/account/connect", json!({}))
            .await
            .0,
        204,
        "connecting when already connected changes nothing"
    );
}

#[tokio::test]
async fn the_choices_that_overwrite_need_the_password_for_that_choice() {
    let served = serve("overwrite", AccountGate::LocalOnly, false).await;
    served.sign_in().await;
    for path in [
        "/api/account/connect/use-device",
        "/api/account/connect/use-account",
    ] {
        let (status, body) = served.call("POST", path, json!({})).await;
        assert_eq!(
            (status, body["code"].as_str()),
            (403, Some("reauth_required")),
            "{path}"
        );
    }
    // Combine needs no password, so a confirmation for it, or for nothing, is refused.
    for body in [
        json!({ "password": SECRET, "action": "connect_settings", "policy": "combine" }),
        json!({ "password": SECRET, "action": "connect_settings" }),
    ] {
        assert_eq!(
            served.call("POST", "/api/account/confirm", body).await.0,
            400
        );
    }
    let nonce = served.nonce_for_policy("use_account").await;
    // Not for the other choice, and not without the nonce.
    assert_eq!(
        served
            .call_with(
                "POST",
                "/api/account/connect/use-device",
                json!({}),
                Some(&nonce)
            )
            .await
            .0,
        403
    );
    assert_eq!(
        served
            .call("POST", "/api/account/connect/use-account", json!({}))
            .await
            .0,
        403
    );
    // For its own, and used up by it.
    assert_eq!(
        served
            .call_with(
                "POST",
                "/api/account/connect/use-account",
                json!({}),
                Some(&nonce)
            )
            .await
            .0,
        204
    );
    assert_eq!(
        served
            .call_with(
                "POST",
                "/api/account/connect/use-account",
                json!({}),
                Some(&nonce)
            )
            .await
            .0,
        403
    );
}
