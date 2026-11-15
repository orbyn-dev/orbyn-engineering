//! Regression source saved before repair1; replay with reviewed production bodies
//! under a future authorized runtime lease to establish genuine failing-before evidence.
use super::autopilot_support::{Scripted, approvers};
use super::review_publication_support::{args, fake, keys, setup};
use crate::source_control::forge::{Forge, with_crew_input_fixture};
use crate::store::PublicationState;
use serde_json::json;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

async fn assert_persisted_stop_fence(before_claim: bool) {
    let s = setup().await;
    let (_, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        // Missing approvers prepares an immutable draft without a remote effect.
        let (answer, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{answer}");
        let id = answer["publication"].as_str().unwrap();
        let mut row = s.w.core.store.review_publication(id).unwrap().unwrap();
        if !before_claim {
            s.w.core
                .store
                .claim_review_publication(&mut row, &s.run, &s.w.lead, "full")
                .unwrap();
            s.w.core
                .store
                .update_review_publication(
                    &mut row,
                    PublicationState::Admitting,
                    Some("two_keys".into()),
                    None,
                )
                .unwrap();
        }
        s.w.core.store.request_stop(&s.run).unwrap();
        assert!(!s.w.core.store.run(&s.run).unwrap().state.is_terminal());
        let denied = if before_claim {
            s.w.core
                .store
                .claim_review_publication(&mut row, &s.run, &s.w.lead, "full")
        } else {
            s.w.core.store.update_review_publication(
                &mut row,
                PublicationState::Sending,
                None,
                None,
            )
        };
        assert!(
            denied.is_err(),
            "persisted Stop must fence this publication transaction"
        );
        assert_ne!(
            s.w.core
                .store
                .review_publication(id)
                .unwrap()
                .unwrap()
                .state,
            PublicationState::Sending
        );
    })
    .await;
}

#[tokio::test]
async fn persisted_stop_while_nonterminal_prevents_claim() {
    assert_persisted_stop_fence(true).await;
}

#[tokio::test]
async fn persisted_stop_while_nonterminal_prevents_sending_cas() {
    assert_persisted_stop_fence(false).await;
}

#[tokio::test]
async fn stop_accepted_during_two_key_admission_never_posts() {
    let s = setup().await;
    let core = s.w.core.clone();
    let run = s.run.clone();
    let lead = Scripted::new("alpha", "alpha", move |_| {
        core.store.request_stop(&run).unwrap();
        crate::crew::Answer {
            approve: true,
            reason: "Allow cannot override the persisted Stop.".into(),
        }
    });
    s.w.core
        .set_crew_approvers(approvers(&lead, &[&Scripted::yes("beta", "beta")]));
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let _ = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!s.w.core.store.run(&s.run).unwrap().state.is_terminal());
        assert_eq!(remote.lock().unwrap().posts, 0);
    })
    .await;
}

#[tokio::test]
async fn deliberate_stale_renewal_retains_old_bytes_and_requires_fresh_admission() {
    let s = setup().await;
    let core = s.w.core.clone();
    let lead = Scripted::new("alpha", "alpha", move |_| {
        core.store
            .set_crew_autopilot_mode("default", "off")
            .unwrap();
        crate::crew::Answer {
            approve: true,
            reason: "The mode drift makes this pre-send attempt stale.".into(),
        }
    });
    s.w.core
        .set_crew_approvers(approvers(&lead, &[&Scripted::yes("beta", "beta")]));
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let (first, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{first}");
        assert_eq!(first["state"], "stale");
        let old_id = first["publication"].as_str().unwrap();
        let old = s.w.core.store.review_publication(old_id).unwrap().unwrap();
        assert_eq!(remote.lock().unwrap().posts, 0);
        s.w.core
            .store
            .set_crew_autopilot_mode("default", "full")
            .unwrap();
        keys(&s);
        // The pre-existing preparation API must not silently create a new send
        // scope when authority is restored. This also records the old stale-scope
        // barrier separately from the new explicit renewal API's availability.
        let (repeated, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{repeated}");
        assert_eq!(repeated["state"], "stale");
        assert_eq!(repeated["publication"], first["publication"]);
        assert_eq!(remote.lock().unwrap().posts, 0);
        assert_eq!(s.w.core.store.review_publication(old_id).unwrap().unwrap(), old);
        eprintln!("COMMON_API_STALE_SCOPE: restored valid mode/keys; repeat preparation retained original Stale ID and issued zero POSTs");
        let mut request = args(&s);
        request["renew"] = json!(old_id);
        let (fresh, error) = s.w.call(&s.token, "crew_publish_review", request).await;
        assert!(!error, "{fresh}");
        assert_eq!(fresh["state"], "published");
        assert_ne!(fresh["publication"], first["publication"]);
        assert_eq!(
            s.w.core.store.review_publication(old_id).unwrap().unwrap(),
            old
        );
        assert_eq!(remote.lock().unwrap().posts, 1);
    })
    .await;
}

#[tokio::test]
async fn possible_remote_acceptance_cannot_be_bypassed_by_renewal() {
    let s = setup().await;
    keys(&s);
    let (remote, fixture) = fake(&s);
    remote.lock().unwrap().lose_reply = true;
    with_crew_input_fixture(fixture, async {
        let (first, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{first}");
        assert_eq!(first["state"], "unknown");
        let mut request = args(&s);
        request["renew"] = first["publication"].clone();
        let _ = s.w.call(&s.token, "crew_publish_review", request).await;
        assert_eq!(remote.lock().unwrap().posts, 1);
        assert_eq!(
            s.w.core
                .store
                .review_publication(first["publication"].as_str().unwrap())
                .unwrap()
                .unwrap()
                .state,
            PublicationState::Unknown
        );
    })
    .await;
}

#[tokio::test]
async fn a_new_live_lead_can_reconcile_an_unknown_send_after_the_old_run_was_stopped() {
    let s = setup().await;
    keys(&s);
    let (remote, fixture) = fake(&s);
    remote.lock().unwrap().lose_reply = true;
    with_crew_input_fixture(fixture, async {
        let (first, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{first}");
        assert_eq!(first["state"], "unknown");
        s.w.core.store.request_stop(&s.run).unwrap();
        s.w.core
            .store
            .finish_run(&s.run, orbyn_contracts::RunState::Cancelled, None)
            .unwrap();
        let next =
            s.w.core
                .store
                .admit_run(crate::store::Admission {
                    thread_id: &s.w.lead,
                    prompt: "Reconcile the prior advisory comment only.",
                    actor: "test",
                })
                .unwrap();
        s.w.core.store.mark_starting(&next.id).unwrap();
        s.w.core.store.mark_running(&next.id).unwrap();
        let token = super::harness::bind(&s.w.core, &s.w.lead, &next.id);
        let (recovered, error) =
            s.w.call(
                &token,
                "crew_publish_review",
                json!({"task":s.subject,"publication":first["publication"]}),
            )
            .await;
        assert!(!error, "{recovered}");
        assert_eq!(recovered["state"], "published");
        assert_eq!(remote.lock().unwrap().posts, 1);
    })
    .await;
}

#[tokio::test]
async fn exact_receipt_can_reconcile_after_the_pr_closes_or_merges_without_reposting() {
    for merged in [false, true] {
        let s = setup().await;
        keys(&s);
        let (remote, base_fixture) = fake(&s);
        remote.lock().unwrap().lose_reply = true;
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let fixture = Arc::new(
            move |forge: Forge, cwd: &Path, args: &[&str], input: Option<&str>| {
                let response = base_fixture(forge, cwd, args, input)?;
                if args.get(1) == Some(&"repos/fixture/repo/pulls/7") && flag.load(Ordering::SeqCst)
                {
                    let mut row: serde_json::Value = serde_json::from_str(&response)?;
                    row["state"] = json!("closed");
                    row["merged"] = json!(merged);
                    return Ok(row.to_string());
                }
                Ok(response)
            },
        );
        with_crew_input_fixture(fixture, async {
            let (first, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
            assert!(!error, "{first}");
            assert_eq!(first["state"], "unknown");
            closed.store(true, Ordering::SeqCst);
            let (recovered, error) =
                s.w.call(
                    &s.token,
                    "crew_publish_review",
                    json!({"task":s.subject,"publication":first["publication"]}),
                )
                .await;
            assert!(!error, "{recovered}");
            assert_eq!(recovered["state"], "needs_person");
            assert_eq!(recovered["receipt"]["id"], 91);
            assert_eq!(remote.lock().unwrap().posts, 1);
        })
        .await;
    }
}
