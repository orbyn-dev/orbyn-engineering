use super::*;

#[test]
fn hello_reads_protocol_family_and_capabilities() {
    let line = r#"{"type":"hello","protocol":1,"host":"0.1.0","family":"claude","sdk":{"name":"x"},"capabilities":["steer","fork"]}"#;
    let Incoming::Hello(hello) = decode(line).unwrap() else {
        panic!("not a hello");
    };
    assert_eq!(hello.protocol, 1);
    assert_eq!(hello.family, "claude");
    assert_eq!(hello.capabilities, ["steer", "fork"]);
}

#[test]
fn a_line_that_does_not_fit_is_an_error_not_a_guess() {
    assert_eq!(decode("{not json"), Err(FrameError::NotJson));
    assert_eq!(decode("[1,2]"), Err(FrameError::NotAnObject));
    assert_eq!(
        decode(r#"{"type":"mystery"}"#),
        Err(FrameError::UnknownType("mystery".into()))
    );
    assert_eq!(
        decode(r#"{"x":1}"#),
        Err(FrameError::UnknownType(String::new()))
    );
    assert_eq!(
        decode(r#"{"type":"delta","update":{}}"#),
        Err(FrameError::Invalid("sendId"))
    );
    assert_eq!(
        decode(r#"{"type":"hello","protocol":"1","family":"x"}"#),
        Err(FrameError::Invalid("protocol"))
    );
}

#[test]
fn a_failed_response_keeps_the_whole_cause_chain() {
    let mut cause = json!({"name":"Error","message":"root","kind":"auth"});
    for level in 0..8 {
        cause = json!({"name":"Error","message":format!("level {level}"),"cause":cause});
    }
    let line = json!({"type":"response","id":7,"ok":false,"error":{"method":"send","cause":cause}});
    let Incoming::Response(response) = decode(&line.to_string()).unwrap() else {
        panic!("not a response");
    };
    let error = response.result.unwrap_err();
    // the wrapper, then nine nested errors
    assert_eq!(error.depth(), 10);
    assert_eq!(error.effective_kind(), ErrorKind::Auth);
    assert_eq!(error.message_text(), Some("level 7"));
}

#[test]
fn run_done_reads_reason_disposition_and_retry_from_result_or_top_level() {
    let inner = r#"{"type":"runDone","sendId":3,"ok":false,"error":{"name":"E","message":"slow down","kind":"rate_limit"},"result":{"reason":"failed","threadDisposition":"reusable","retry":{"afterMs":500,"why":"rate_limit","resetsAt":1700000000}}}"#;
    let Incoming::RunDone(done) = decode(inner).unwrap() else {
        panic!("not a runDone");
    };
    assert_eq!(done.reason, Some(Reason::Failed));
    assert_eq!(done.disposition.as_deref(), Some("reusable"));
    let retry = done.retry.unwrap();
    assert_eq!(
        (retry.after_ms, retry.resets_at),
        (Some(500), Some(1_700_000_000))
    );
    assert_eq!(retry.why.as_deref(), Some("rate_limit"));

    let top = r#"{"type":"runDone","sendId":3,"ok":true,"threadDisposition":"broken","result":{"reason":"completed"}}"#;
    let Incoming::RunDone(done) = decode(top).unwrap() else {
        panic!("not a runDone");
    };
    assert_eq!(done.disposition.as_deref(), Some("broken"));
}

#[test]
fn permission_and_question_keep_the_engines_input_untouched() {
    let line = r#"{"type":"permission","sendId":7,"requestId":"p-31","tool":"Edit","input":{"path":"a.txt"},"options":[]}"#;
    let Incoming::Permission(ask) = decode(line).unwrap() else {
        panic!("not a permission");
    };
    assert_eq!(
        (ask.send_id, ask.request_id.as_str(), ask.tool.as_str()),
        (7, "p-31", "Edit")
    );
    assert_eq!(ask.input, json!({"path":"a.txt"}));
    assert_eq!(
        ask.options,
        Some(json!([])),
        "the engine's options are kept as sent"
    );
    assert!(matches!(
        decode(
            r#"{"type":"question","sendId":1,"requestId":"q","tool":"AskUserQuestion","input":{}}"#
        ),
        Ok(Incoming::Question(_))
    ));
}

#[test]
fn requests_carry_their_id_and_type() {
    let line = request(5, "cancel", json!({"sendId": 4}));
    let value: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(value, json!({"id":5,"type":"cancel","sendId":4}));
    assert!(!line.contains('\n'));
    assert!(is_gated("steer") && !is_gated("open"));
}
