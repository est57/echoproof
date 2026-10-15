use serde_json::json;

use crate::idmap::IdMap;

fn created_order() -> IdMap {
    let mut ids = IdMap::default();
    ids.learn(
        "/pos-sync/orders",
        &json!({"result": {"id": 9812, "status": "saved"}}),
        &json!({"result": {"id": 57, "status": "saved"}}),
    );
    ids
}

#[test]
fn rewrites_the_path_segment_after_the_same_resource_only() {
    let ids = created_order();
    assert_eq!(ids.len(), 1);
    assert_eq!(
        ids.rewrite_path("/pos-sync/orders/9812"),
        "/pos-sync/orders/57"
    );
    assert_eq!(
        ids.rewrite_path("/pos-sync/orders/9812/items"),
        "/pos-sync/orders/57/items"
    );
    assert_eq!(
        ids.rewrite_path("/pos-sync/tables/9812"),
        "/pos-sync/tables/9812"
    );
}

#[test]
fn rewrites_query_and_body_fields_of_the_same_entity() {
    let ids = created_order();
    assert_eq!(
        ids.rewrite_path("/pos-sync/orders/void?order_id=9812&table_id=9812"),
        "/pos-sync/orders/void?order_id=57&table_id=9812"
    );
    let mut body = json!({"order_id": 9812, "table_id": 9812, "lines": [{"order_id": "9812"}]});
    ids.rewrite_body("/pos-sync/orders/void", &mut body);
    assert_eq!(
        body,
        json!({"order_id": 57, "table_id": 9812, "lines": [{"order_id": "57"}]})
    );
}

#[test]
fn learns_from_enclosing_keys_and_lists() {
    let mut ids = IdMap::default();
    ids.learn(
        "/pos-sync/start-shift",
        &json!({"shift": {"id": "SH-A1"}, "tickets": [{"id": 3001}, {"id": 3002}]}),
        &json!({"shift": {"id": "SH-B7"}, "tickets": [{"id": 3001}, {"id": 4410}]}),
    );
    assert_eq!(ids.len(), 2);
    assert_eq!(
        ids.rewrite_path("/pos-sync/shifts/SH-A1"),
        "/pos-sync/shifts/SH-B7"
    );
    assert_eq!(ids.rewrite_path("/kds/tickets/3002"), "/kds/tickets/4410");
    assert_eq!(ids.rewrite_path("/kds/tickets/3001"), "/kds/tickets/3001");
}

#[test]
fn ignores_small_values_and_unchanged_ids() {
    let mut ids = IdMap::default();
    ids.learn(
        "/orders",
        &json!({"id": 5, "code": "A"}),
        &json!({"id": 6, "code": "B"}),
    );
    ids.learn("/orders", &json!({"id": 9812}), &json!({"id": 9812}));
    assert!(ids.is_empty());
    assert_eq!(ids.rewrite_path("/orders/5"), "/orders/5");
}

#[test]
fn line_items_inside_an_order_are_their_own_entity() {
    let mut ids = IdMap::default();
    ids.learn(
        "/orders",
        &json!({"data": {"id": 9812, "items": [{"id": 70011}]}}),
        &json!({"data": {"id": 57, "items": [{"id": 120}]}}),
    );
    assert_eq!(
        ids.rewrite_path("/orders/9812/items/70011"),
        "/orders/57/items/120"
    );
    assert_eq!(ids.rewrite_path("/orders/70011"), "/orders/70011");
}
