//! The nonce a confirmation answers with: usable only with it, for one action and request, used up,
//! cancellable by its own nonce only, and gone with the session.

use super::dev;
use crate::CoreError;
use crate::account::ConfirmFor;
use crate::account::Password;
use crate::account::dev::PASSWORD;
use crate::testing::TempDir;

fn pw(text: &str) -> Password {
    Password::new(text.to_owned())
}

fn code(error: CoreError) -> &'static str {
    error.code()
}

async fn signed_in(core: &crate::Core, email: &str) {
    core.account.sign_in(email, pw(PASSWORD)).await.unwrap();
}

/// Values that are not the nonce, made from it or from fresh randomness, never written out as
/// literals: one character flipped, shorter, longer, empty, and another confirmation's.
fn not_the_nonce(nonce: &str) -> Vec<String> {
    let mut flipped = nonce.to_owned();
    let first = flipped.remove(0);
    flipped.insert(0, if first == 'a' { 'b' } else { 'a' });
    let fresh: String = crate::util::random_bytes::<16>()
        .unwrap()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    vec![
        flipped,
        nonce[..nonce.len() - 1].to_owned(),
        format!("{nonce}{}", &nonce[..1]),
        nonce[..0].to_owned(),
        fresh,
    ]
}

#[tokio::test]
async fn a_confirmation_is_only_usable_with_the_nonce_it_answered_with() {
    let dir = TempDir::new();
    let core = dev(&dir, "nonce");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    let ask = || ConfirmFor::SignOutEverywhere;
    let nonce = a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    assert_eq!(nonce.len(), 32);
    // Without one, or with another, the confirmation is refused and not used up.
    assert_eq!(
        code(a.require_confirmed(ask(), None).await.unwrap_err()),
        "reauth_required"
    );
    assert!(!a.confirmed_for(&ask(), None));
    for wrong in not_the_nonce(&nonce) {
        assert_eq!(
            code(a.require_confirmed(ask(), Some(&wrong)).await.unwrap_err()),
            "reauth_required"
        );
        assert!(!a.confirmed_for(&ask(), Some(&wrong)));
    }
    assert!(a.confirmed_for(&ask(), Some(&nonce)));
    // A nonce is for its own action and request.
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::RemoveDevice("dev-mini".into()), Some(&nonce))
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
    // Used up on first use, and every confirmation has its own.
    a.require_confirmed(ask(), Some(&nonce)).await.unwrap();
    assert_eq!(
        code(a.require_confirmed(ask(), Some(&nonce)).await.unwrap_err()),
        "reauth_required"
    );
    let first = a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    let second = a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    assert_ne!(first, second);
    assert!(
        !a.confirmed_for(&ask(), Some(&first)),
        "a later confirmation replaces the earlier one"
    );
    assert!(a.confirmed_for(&ask(), Some(&second)));
}

#[tokio::test]
async fn cancelling_clears_the_confirmation_and_only_its_nonce_can() {
    let dir = TempDir::new();
    let core = dev(&dir, "nonce-cancel");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    let ask = || ConfirmFor::RemoveDevice("dev-mini".into());
    let nonce = a.confirm(ask(), pw(PASSWORD)).await.unwrap();
    for wrong in not_the_nonce(&nonce) {
        a.cancel_confirmation(&wrong);
    }
    assert!(
        a.confirmed_for(&ask(), Some(&nonce)),
        "someone else's cancel did nothing"
    );
    a.cancel_confirmation(&nonce);
    assert!(!a.confirmed_for(&ask(), Some(&nonce)));
    assert_eq!(
        code(a.require_confirmed(ask(), Some(&nonce)).await.unwrap_err()),
        "reauth_required"
    );
}

#[tokio::test]
async fn a_nonce_does_not_survive_signing_out_or_signing_in_again() {
    let dir = TempDir::new();
    let core = dev(&dir, "nonce-session");
    let a = &core.account;
    signed_in(&core, "dev@example.com").await;
    let nonce = a
        .confirm(ConfirmFor::SignOutEverywhere, pw(PASSWORD))
        .await
        .unwrap();
    a.sign_out(false).await.unwrap();
    signed_in(&core, "dev@example.com").await;
    assert_eq!(
        code(
            a.require_confirmed(ConfirmFor::SignOutEverywhere, Some(&nonce))
                .await
                .unwrap_err()
        ),
        "reauth_required"
    );
}
