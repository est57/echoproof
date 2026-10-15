use serde_json::{Value, json};

use super::*;
use crate::outbound::{OutboundCall, OutboundRequest};
use crate::snapshot::{Request, Response};

fn call(method: &str, url: &str, body: Value) -> OutboundCall {
    OutboundCall {
        request: OutboundRequest {
            method: method.into(),
            url: url.into(),
            headers: Default::default(),
            body: Body::Json(body),
        },
        response: Response {
            status: 200,
            headers: Default::default(),
            body: Body::Empty,
            latency_ms: 0,
        },
        unrecorded: false,
    }
}

fn exchange(outbound: Option<Vec<OutboundCall>>) -> Exchange {
    Exchange {
        id: "x".into(),
        recorded_at_ms: 0,
        request: Request {
            method: "POST".into(),
            path: "/checkout".into(),
            headers: Default::default(),
            body: Body::Empty,
        },
        response: Response {
            status: 200,
            headers: Default::default(),
            body: Body::Json(json!({"paid": true})),
            latency_ms: 0,
        },
        skipped: None,
        outbound,
    }
}

fn charge(amount: i64) -> OutboundCall {
    call(
        "POST",
        "https://pay.test/v2/charges?key=[REDACTED]",
        json!({"amount": amount, "currency": "IDR"}),
    )
}

fn changes(a: Option<Vec<OutboundCall>>, b: Option<Vec<OutboundCall>>) -> Vec<Change> {
    let options = DiffOptions {
        auto_noise: true,
        ..Default::default()
    };
    diff_exchange(&exchange(a), &exchange(b), &options)
}

#[test]
fn uncaptured_candidate_is_not_compared() {
    assert!(changes(Some(vec![charge(18000)]), None).is_empty());
    assert!(changes(Some(vec![charge(18000)]), Some(vec![charge(18000)])).is_empty());
}

#[test]
fn reports_a_changed_field_sent_to_another_service() {
    let found = changes(Some(vec![charge(18000)]), Some(vec![charge(1800000)]));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, "outbound[POST pay.test/v2/charges] $.amount");
    assert_eq!(found[0].kind, ChangeKind::ValueChanged);
    assert_eq!(found[0].after, Some(json!(1800000)));
}

#[test]
fn unrecorded_new_call_is_breaking_and_removed_call_is_behavior() {
    let mut notify = call("POST", "https://wa.test/notify", json!({"order": "A"}));
    notify.unrecorded = true;
    let added = changes(Some(vec![charge(1)]), Some(vec![charge(1), notify]));
    assert_eq!(added.len(), 1);
    assert_eq!(
        (added[0].kind, added[0].severity),
        (ChangeKind::CallAdded, Severity::Breaking)
    );
    assert_eq!(added[0].path, "outbound[POST wa.test/notify]");

    let removed = changes(Some(vec![charge(1)]), Some(vec![]));
    assert_eq!(
        (removed[0].kind, removed[0].severity),
        (ChangeKind::CallRemoved, Severity::Behavior)
    );
}

#[test]
fn ids_in_urls_fold_into_one_pattern() {
    let a = call("GET", "https://crm.test/v1/customers/123", json!(null));
    let b = call("GET", "https://crm.test/v1/customers/456", json!(null));
    let found = changes(Some(vec![a]), Some(vec![b]));
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].pattern,
        "outbound[GET crm.test/v1/customers/{id}] url"
    );
}

#[test]
fn repeated_calls_pair_in_order() {
    let found = changes(
        Some(vec![charge(1), charge(2)]),
        Some(vec![charge(1), charge(3)]),
    );
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].path,
        "outbound[POST pay.test/v2/charges#1] $.amount"
    );
}

#[test]
fn noise_learned_from_base_runs_covers_outbound_fields() {
    let stamped = |at: &str| {
        call(
            "POST",
            "https://pay.test/v2/charges",
            json!({"amount": 1, "sent_at": at}),
        )
    };
    let options = DiffOptions::default();
    let first = vec![exchange(Some(vec![stamped("10:00")]))];
    let second = vec![exchange(Some(vec![stamped("10:01")]))];
    let noise = learn_noise(&first, &second, &options);
    let options = DiffOptions { noise, ..options };
    let base = exchange(Some(vec![stamped("10:00")]));
    let pr = exchange(Some(vec![stamped("10:05")]));
    assert!(diff_exchange(&base, &pr, &options).is_empty());
}

#[test]
fn empty_header_lists_from_php_parse_as_no_headers() {
    let text = r#"{"request":{"method":"POST","url":"http://wa.test/notify","headers":[],"body":"empty"},
        "response":{"status":599,"headers":[],"body":{"text":"unrecorded"}},"unrecorded":true}"#;
    let call: OutboundCall = serde_json::from_str(text).unwrap();
    assert!(call.request.headers.is_empty() && call.response.headers.is_empty());
    assert!(call.unrecorded);
    let bad = text.replacen(r#""headers":[]"#, r#""headers":["x"]"#, 1);
    assert!(serde_json::from_str::<OutboundCall>(&bad).is_err());
}

#[test]
fn routes_ignore_default_ports_case_and_query() {
    let route = |url: &str| call("post", url, json!(null)).request.route();
    assert_eq!(
        route("HTTPS://Pay.Test:443/v2/charges?key=1"),
        "POST https://pay.test/v2/charges"
    );
    assert_eq!(
        route("http://pay.test:8080/v2"),
        "POST http://pay.test:8080/v2"
    );
    assert_eq!(route("https://pay.test"), "POST https://pay.test/");
}
