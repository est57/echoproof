use serde_json::{Value, json};

use super::*;
use crate::snapshot::{Request, Response};

fn exchange(status: u16, body: Value) -> Exchange {
    Exchange {
        id: "x".into(),
        recorded_at_ms: 0,
        request: Request {
            method: "GET".into(),
            path: "/".into(),
            headers: Default::default(),
            body: Body::Empty,
        },
        response: Response {
            status,
            headers: Default::default(),
            body: Body::Json(body),
            latency_ms: 0,
        },
        skipped: None,
    }
}

fn options(ignore: &[&str]) -> DiffOptions {
    DiffOptions {
        ignore: ignore.iter().map(|p| FieldPattern::parse(p)).collect(),
        auto_noise: true,
        ..Default::default()
    }
}

fn kinds(changes: &[Change]) -> Vec<(String, ChangeKind)> {
    changes.iter().map(|c| (c.path.clone(), c.kind)).collect()
}

#[test]
fn ignores_volatile_values() {
    let a = exchange(
        200,
        json!({"id": "2f1c6a0e-8f7b-4a51-9d2b-0b8e7c7d9f10", "at": "2026-10-14T07:00:00Z", "n": 1}),
    );
    let b = exchange(
        200,
        json!({"id": "9a1c6a0e-8f7b-4a51-9d2b-0b8e7c7d9f11", "at": "2026-10-14T07:00:09Z", "n": 1.0}),
    );
    assert!(diff_exchange(&a, &b, &options(&[])).is_empty());
}

#[test]
fn classifies_changes() {
    let a = exchange(
        200,
        json!({"total": 100, "gone": true, "items": [{"p": 1}, {"p": 2}]}),
    );
    let b = exchange(
        200,
        json!({"total": null, "new": 1, "items": [{"p": 1}, {"p": 3}, {"p": 4}]}),
    );
    let changes = diff_exchange(&a, &b, &options(&[]));
    assert_eq!(
        kinds(&changes),
        vec![
            ("$.total".into(), ChangeKind::TypeChanged),
            ("$.gone".into(), ChangeKind::FieldRemoved),
            ("$.items".into(), ChangeKind::LengthChanged),
            ("$.items[1].p".into(), ChangeKind::ValueChanged),
            ("$.new".into(), ChangeKind::FieldAdded),
        ]
    );
    assert_eq!(changes[3].pattern, "$.items[*].p");
}

#[test]
fn status_to_error_is_breaking() {
    let changes = diff_exchange(
        &exchange(200, json!({})),
        &exchange(500, json!({})),
        &options(&[]),
    );
    assert_eq!(changes[0].kind, ChangeKind::StatusChanged);
    assert_eq!(changes[0].severity, Severity::Breaking);
}

#[test]
fn learned_noise_is_ignored_with_children() {
    let a = exchange(200, json!({"etag": "a", "meta": {"x": 1}, "total": 1}));
    let b = exchange(200, json!({"etag": "b", "meta": {"x": 2}, "total": 1}));
    let mut opts = options(&[]);
    opts.noise = learn_noise(std::slice::from_ref(&a), std::slice::from_ref(&b), &opts);
    assert_eq!(opts.noise.len(), 2);
    let c = exchange(200, json!({"etag": "c", "meta": {"x": 3}, "total": 2}));
    assert_eq!(
        kinds(&diff_exchange(&a, &c, &opts)),
        vec![("$.total".into(), ChangeKind::ValueChanged)]
    );
}

#[test]
fn noise_prefix_does_not_match_sibling_fields() {
    let mut opts = options(&[]);
    opts.noise.insert(("GET /".into(), "$.id".into()));
    assert!(opts.is_noise("GET /", "$.id"));
    assert!(opts.is_noise("GET /", "$.id.x"));
    assert!(!opts.is_noise("GET /", "$.identity"));
}

#[test]
fn ignore_patterns() {
    let a = exchange(
        200,
        json!({"meta": {"trace": 1}, "items": [{"etag": 1, "v": 1}], "trace": 1}),
    );
    let b = exchange(
        200,
        json!({"meta": {"trace": 2}, "items": [{"etag": 2, "v": 1}], "trace": 2}),
    );
    assert!(diff_exchange(&a, &b, &options(&["trace", "items[*].etag"])).is_empty());
    let anchored = diff_exchange(&a, &b, &options(&["$.trace", "items[*].etag"]));
    assert_eq!(
        kinds(&anchored),
        vec![("$.meta.trace".into(), ChangeKind::ValueChanged)]
    );
}

#[test]
fn arrays_of_objects_match_by_identity() {
    let a = exchange(
        200,
        json!({"tickets": [{"ticket_id": 1, "type": "DINE"}, {"ticket_id": 2, "type": "TAKE"}, {"ticket_id": 3, "type": "DINE"}]}),
    );
    let reordered = exchange(
        200,
        json!({"tickets": [{"ticket_id": 2, "type": "TAKE"}, {"ticket_id": 1, "type": "DINE"}, {"ticket_id": 3, "type": "DINE"}]}),
    );
    let changes = diff_exchange(&a, &reordered, &options(&[]));
    assert_eq!(
        kinds(&changes),
        vec![("$.tickets#order".into(), ChangeKind::OrderChanged)]
    );

    let edited = exchange(
        200,
        json!({"tickets": [{"ticket_id": 2, "type": "TAKE"}, {"ticket_id": 1, "type": "TAKE"}, {"ticket_id": 4, "type": "DINE"}]}),
    );
    let changes = diff_exchange(&a, &edited, &options(&[]));
    assert_eq!(
        kinds(&changes),
        vec![
            ("$.tickets[0].type".into(), ChangeKind::ValueChanged),
            ("$.tickets[2]".into(), ChangeKind::ItemRemoved),
            ("$.tickets[2]".into(), ChangeKind::ItemAdded),
            ("$.tickets#order".into(), ChangeKind::OrderChanged),
        ]
    );
}

#[test]
fn order_noise_does_not_hide_item_changes() {
    let mut opts = options(&[]);
    opts.noise
        .insert(("GET /".into(), "$.tickets#order".into()));
    let a = exchange(
        200,
        json!({"tickets": [{"id": 1, "v": 1}, {"id": 2, "v": 1}]}),
    );
    let b = exchange(
        200,
        json!({"tickets": [{"id": 2, "v": 1}, {"id": 1, "v": 9}]}),
    );
    assert_eq!(
        kinds(&diff_exchange(&a, &b, &opts)),
        vec![("$.tickets[0].v".into(), ChangeKind::ValueChanged)]
    );
}

#[test]
fn volatile_ids_fall_back_to_position() {
    let a = exchange(
        200,
        json!({"rows": [{"id": "a", "v": 1}, {"id": "b", "v": 2}]}),
    );
    let b = exchange(
        200,
        json!({"rows": [{"id": "x", "v": 1}, {"id": "y", "v": 3}]}),
    );
    let changes = diff_exchange(&a, &b, &options(&[]));
    assert!(
        changes
            .iter()
            .all(|c| c.kind != ChangeKind::ItemRemoved && c.kind != ChangeKind::ItemAdded)
    );
}
