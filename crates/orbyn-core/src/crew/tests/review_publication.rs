//! Source-only regression drafts for lead Network admission and uncertain comments.
use super::review_publication_support::{args, fake, keys, setup};
use crate::source_control::forge::with_crew_input_fixture;
use serde_json::json;

fn full_access_token(s: &super::review_publication_support::Setup) -> String {
    use crate::{governance::approvals::RunScope, tools::mcp::McpBinding};
    s.w.core
        .mcp
        .register(McpBinding {
            scope: RunScope {
                run_id: s.run.clone(),
                thread_id: s.w.lead.clone(),
                engine: "alpha".into(),
                family: "alpha".into(),
                write_scope: None,
                mode: orbyn_contracts::PermissionMode::FullAccess,
                plan: false,
            },
            permission_prompt: false,
            skills: vec![],
        })
        .unwrap()
}

#[tokio::test]
async fn off_asks_even_with_full_access_and_existing_person_approval_controls_the_effect() {
    let s = setup().await;
    keys(&s);
    s.w.core
        .store
        .set_crew_autopilot_mode("default", "off")
        .unwrap();
    let token = full_access_token(&s);
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let publish = s.w.call(&token, "crew_publish_review", args(&s));
        let answer = async {
            s.w.until("publication asks the person", |w| {
                !w.core.store.pending_for_thread(&w.lead).unwrap().is_empty()
            })
            .await;
            assert_eq!(remote.lock().unwrap().posts, 0);
            let pending =
                s.w.core
                    .store
                    .pending_for_thread(&s.w.lead)
                    .unwrap()
                    .remove(0);
            s.w.core
                .gate
                .decide(&pending.id, true, false, "test-person")
                .unwrap();
        };
        let ((result, error), ()) = tokio::join!(publish, answer);
        assert!(!error, "{result}");
        assert_eq!(result["state"], "published");
        assert_eq!(remote.lock().unwrap().posts, 1);
    })
    .await;
}

#[tokio::test]
async fn off_timeout_cannot_fall_through_to_full_access() {
    let s = super::review_publication_support::setup_timeout(Some(
        std::time::Duration::from_millis(20),
    ))
    .await;
    s.w.core
        .store
        .set_crew_autopilot_mode("default", "off")
        .unwrap();
    let token = full_access_token(&s);
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let (answer, error) = s.w.call(&token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{answer}");
        assert_eq!(answer["state"], "draft");
        assert_eq!(remote.lock().unwrap().posts, 0);
        assert_eq!(
            s.w.core
                .store
                .list_approvals(Some(orbyn_contracts::ApprovalState::Expired))
                .unwrap()
                .len(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn mode_revoked_while_chain_waits_invalidates_the_saved_admission() {
    use super::autopilot_support::{Scripted, approvers};
    let s = setup().await;
    let core = s.w.core.clone();
    let lead = Scripted::new("alpha", "alpha", move |_| {
        core.store
            .set_crew_autopilot_mode("default", "off")
            .unwrap();
        crate::crew::Answer {
            approve: true,
            reason: "Scripted allow before mode revocation is rechecked.".into(),
        }
    });
    s.w.core
        .set_crew_approvers(approvers(&lead, &[&Scripted::yes("beta", "beta")]));
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let (answer, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{answer}");
        assert_eq!(answer["state"], "stale");
        assert_eq!(remote.lock().unwrap().posts, 0);
    })
    .await;
}

#[tokio::test]
async fn concurrent_lead_requests_share_one_durable_send_scope() {
    let s = setup().await;
    keys(&s);
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let (left, right) = tokio::join!(
            s.w.call(&s.token, "crew_publish_review", args(&s)),
            s.w.call(&s.token, "crew_publish_review", args(&s))
        );
        assert!(!left.1 || !right.1, "both refused: {left:?} {right:?}");
        assert_eq!(remote.lock().unwrap().posts, 1);
    })
    .await;
}

#[tokio::test]
async fn head_movement_after_post_retains_the_real_receipt_as_needing_a_person() {
    let s = setup().await;
    keys(&s);
    let (remote, fixture) = fake(&s);
    remote.lock().unwrap().move_after_post = true;
    with_crew_input_fixture(fixture, async {
        let (answer, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{answer}");
        assert_eq!(answer["state"], "needs_person");
        assert_eq!(answer["receipt"]["id"], 91);
        assert_eq!(remote.lock().unwrap().posts, 1);
    })
    .await;
}

#[tokio::test]
async fn dropping_an_ask_releases_only_the_unstarted_admission() {
    let s = setup().await;
    s.w.core
        .store
        .set_crew_autopilot_mode("default", "off")
        .unwrap();
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture,async {
        let mut publish=Box::pin(s.w.call(&s.token,"crew_publish_review",args(&s)));
        tokio::select! {
            result=&mut publish=>panic!("should await existing approval: {result:?}"),
            ()=s.w.until("pending publication approval",|w|!w.core.store.pending_for_thread(&w.lead).unwrap().is_empty())=>{}
        }
        let pending=s.w.core.store.pending_for_thread(&s.w.lead).unwrap().remove(0);
        let id=pending.detail["crew_review_comment"].as_str().unwrap().to_owned();
        drop(publish);
        assert_eq!(s.w.core.store.review_publication(&id).unwrap().unwrap().state,crate::store::PublicationState::Draft);
        assert_eq!(remote.lock().unwrap().posts,0);
    }).await;
}

#[tokio::test]
async fn publication_payload_cas_and_sending_scope_survive_database_reopen() {
    use crate::store::PublicationState;
    let s = setup().await;
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        // Full with no configured approvers prepares a safe immutable draft and denies.
        let (answer, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{answer}");
        let id = answer["publication"].as_str().unwrap();
        let mut row = s.w.core.store.review_publication(id).unwrap().unwrap();
        let mut changed = row.clone();
        changed.body.push_str("changed after preparation");
        assert!(
            s.w.core
                .store
                .update_review_publication(&mut changed, PublicationState::Stale, None, None)
                .is_err()
        );
        s.w.core
            .store
            .claim_review_publication(&mut row, &s.run, &s.w.lead, "full")
            .unwrap();
        let mut old = row.clone();
        s.w.core
            .store
            .update_review_publication(
                &mut row,
                PublicationState::Admitting,
                Some("two_keys".into()),
                None,
            )
            .unwrap();
        assert!(
            s.w.core
                .store
                .update_review_publication(
                    &mut old,
                    PublicationState::Sending,
                    Some("stale claim".into()),
                    None
                )
                .is_err()
        );
        s.w.core
            .store
            .update_review_publication(&mut row, PublicationState::Sending, None, None)
            .unwrap();
        assert!(
            s.w.core
                .store
                .update_review_publication(&mut row, PublicationState::Draft, None, None)
                .is_err()
        );
        let mut duplicate = row.clone();
        duplicate.id = crate::util::new_id();
        duplicate.revision = 0;
        duplicate.state = PublicationState::Draft;
        duplicate.claim = None;
        duplicate
            .body
            .push_str("new body cannot bypass uncertain scope");
        assert_eq!(
            s.w.core
                .store
                .add_review_publication(&duplicate)
                .unwrap()
                .id,
            id
        );
        let path =
            s.w.core
                .store
                .reader()
                .query_row("PRAGMA database_list", [], |r| r.get::<_, String>(2))
                .unwrap();
        let reopened =
            crate::store::Store::open(std::path::Path::new(&path), "publication-reopen").unwrap();
        let saved = reopened.review_publication(id).unwrap().unwrap();
        assert_eq!(saved.state, PublicationState::Sending);
        assert_eq!(saved.body, row.body);
        assert_eq!(saved.body_sha256, row.body_sha256);
        assert_eq!(remote.lock().unwrap().posts, 0);
    })
    .await;
}

#[tokio::test]
async fn live_lead_two_keys_publish_the_exact_saved_body_once() {
    let s = setup().await;
    keys(&s);
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let (answer, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{answer}");
        assert_eq!(answer["state"], "published");
        let id = answer["publication"].as_str().unwrap();
        let stored = s.w.core.store.review_publication(id).unwrap().unwrap();
        {
            let state = remote.lock().unwrap();
            assert_eq!(state.posts, 1);
            assert_eq!(state.body.as_deref(), Some(stored.body.as_str()));
        }
        assert!(stored.body.contains(&format!(
            "orbyn://crew/task/{}",
            s.subject.replace(':', "%3A")
        )));
        assert!(stored.body.contains("not a forge approval"));
        assert!(stored.decision.unwrap().contains("two_keys"));
        let (again, error) =
            s.w.call(
                &s.token,
                "crew_publish_review",
                json!({"task":s.subject,"publication":id}),
            )
            .await;
        assert!(!error, "{again}");
        assert_eq!(remote.lock().unwrap().posts, 1);
    })
    .await;
}

#[tokio::test]
async fn accepted_but_lost_reply_and_delayed_visibility_never_repeat_post() {
    let s = setup().await;
    keys(&s);
    let (remote, fixture) = fake(&s);
    remote.lock().unwrap().lose_reply = true;
    with_crew_input_fixture(fixture, async {
        let (first, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
        assert!(!error, "{first}");
        assert_eq!(first["state"], "unknown");
        let id = first["publication"].as_str().unwrap();
        remote.lock().unwrap().invisible = true;
        let request = json!({"task":s.subject,"publication":id});
        let (empty, error) =
            s.w.call(&s.token, "crew_publish_review", request.clone())
                .await;
        assert!(!error, "{empty}");
        assert_eq!(empty["state"], "unknown");
        remote.lock().unwrap().invisible = false;
        let (seen, error) = s.w.call(&s.token, "crew_publish_review", request).await;
        assert!(!error, "{seen}");
        assert_eq!(seen["state"], "published");
        assert_eq!(remote.lock().unwrap().posts, 1);
    })
    .await;
}

#[tokio::test]
async fn policy_deny_and_missing_second_vendor_prevent_publication() {
    use orbyn_contracts::{ActionKind, CreatePolicyRule, PolicyEffect};
    for case in 0..3 {
        let s = setup().await;
        if case == 1 {
            use super::autopilot_support::{Scripted, approvers};
            s.w.core.set_crew_approvers(approvers(
                &Scripted::yes("alpha", "alpha"),
                &[&Scripted::yes("beta", "alpha")],
            ));
        }
        if case == 2 {
            keys(&s);
            s.w.core
                .store
                .add_rule(
                    &CreatePolicyRule {
                        effect: PolicyEffect::Deny,
                        kind: ActionKind::Network,
                        pattern: "*".into(),
                        engine: None,
                        note: None,
                    },
                    "test",
                )
                .unwrap();
        }
        let (remote, fixture) = fake(&s);
        with_crew_input_fixture(fixture, async {
            let (answer, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
            assert!(!error, "{answer}");
            assert_eq!(answer["state"], "draft");
            assert_eq!(remote.lock().unwrap().posts, 0);
            assert_eq!(s.w.task(&s.subject).state, "passed");
            assert_eq!(s.w.task(&s.reviewer).state, "passed");
        })
        .await;
    }
}

#[tokio::test]
async fn forged_claim_reviewer_and_nonexistent_run_cannot_export() {
    use crate::governance::approvals::{ActionAsk, RunScope};
    use orbyn_contracts::{ActionKind, PermissionMode};
    let s = setup().await;
    keys(&s);
    let (remote, fixture) = fake(&s);
    with_crew_input_fixture(fixture, async {
        let scope = RunScope {
            run_id: s.run.clone(),
            thread_id: s.w.lead.clone(),
            engine: "alpha".into(),
            family: "alpha".into(),
            write_scope: None,
            mode: PermissionMode::FullAccess,
            plan: false,
        };
        assert!(
            !s.w.core
                .gate
                .request(
                    &scope,
                    ActionAsk {
                        kind: ActionKind::Network,
                        target: "https://github.com/fixture/repo/pull/7".into(),
                        summary: "Advisory review".into(),
                        paths: vec![],
                        detail: json!({"crew_review_comment":"not-a-saved-claim","claim":"made-up"})
                    }
                )
                .await
                .is_allowed()
        );
        let reviewer_token =
            super::harness::bind(&s.w.core, &s.w.worker(&s.reviewer), "ended-reviewer");
        assert!(
            s.w.call(&reviewer_token, "crew_publish_review", args(&s))
                .await
                .1
        );
        assert!(
            s.w.call(&s.w.lead_token, "crew_publish_review", args(&s))
                .await
                .1,
            "harness legacy run-lead is not an admitted live run"
        );
        assert_eq!(remote.lock().unwrap().posts, 0);
    })
    .await;
}

#[tokio::test]
async fn duplicate_or_spoofed_marker_requires_a_person_without_another_post() {
    for duplicate in [false, true] {
        let s = setup().await;
        keys(&s);
        let (remote, fixture) = fake(&s);
        remote.lock().unwrap().lose_reply = true;
        with_crew_input_fixture(fixture, async {
            let (first, error) = s.w.call(&s.token, "crew_publish_review", args(&s)).await;
            assert!(!error, "{first}");
            {
                let mut state = remote.lock().unwrap();
                state.duplicate = duplicate;
                state.spoof = !duplicate;
            }
            let (answer, error) =
                s.w.call(
                    &s.token,
                    "crew_publish_review",
                    json!({"task":s.subject,"publication":first["publication"]}),
                )
                .await;
            assert!(!error, "{answer}");
            assert_eq!(answer["state"], "needs_person");
            assert_eq!(remote.lock().unwrap().posts, 1);
        })
        .await;
    }
}
