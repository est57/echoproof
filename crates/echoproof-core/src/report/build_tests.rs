use serde_json::{Value, json};

use super::{NotReproduced, ReportOptions, build_report};
use crate::diff::DiffOptions;
use crate::render;
use crate::snapshot::{Body, Exchange, Request, Response};

fn exchange(id: &str, path: &str, status: u16, body: Value) -> Exchange {
    Exchange {
        id: id.into(),
        recorded_at_ms: 0,
        request: Request {
            method: "GET".into(),
            path: path.into(),
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
        outbound: None,
    }
}

fn options(missing: &[(&str, u16, u16)]) -> ReportOptions {
    let not_reproduced: NotReproduced = missing
        .iter()
        .map(|(id, recorded, base)| (id.to_string(), (*recorded, *base)))
        .collect();
    ReportOptions {
        max_examples: 3,
        not_reproduced,
        ..Default::default()
    }
}

#[test]
fn requests_that_failed_on_base_are_not_counted_as_unchanged() {
    let base = vec![
        exchange("a", "/orders/9812", 404, json!({"message": "not found"})),
        exchange("b", "/orders/1", 200, json!({"total": 10})),
    ];
    let report = build_report(
        &base,
        &base,
        &DiffOptions::default(),
        &options(&[("a", 200, 404)]),
    );
    assert_eq!(
        (report.unchanged, report.not_reproduced, report.changed),
        (1, 1, 0)
    );
    let endpoint = &report.endpoints[0];
    assert_eq!(
        endpoint.not_reproduced_examples,
        ["GET /orders/9812: recorded 200, base build 404"]
    );
}

#[test]
fn a_change_on_a_request_that_did_not_reproduce_is_still_reported() {
    let base = vec![exchange("a", "/orders/9812", 404, json!({}))];
    let pr = vec![exchange("a", "/orders/9812", 500, json!({}))];
    let report = build_report(
        &base,
        &pr,
        &DiffOptions::default(),
        &options(&[("a", 200, 404)]),
    );
    assert_eq!((report.changed, report.not_reproduced), (1, 0));
}

#[test]
fn reports_show_the_gap() {
    let base = vec![
        exchange("a", "/orders/9812", 404, json!({})),
        exchange("b", "/orders/1", 200, json!({})),
    ];
    let report = build_report(
        &base,
        &base,
        &DiffOptions::default(),
        &options(&[("a", 200, 0)]),
    );
    let text = render::text(&report);
    assert!(text.contains("1 not reproduced"));
    assert!(text.contains("GET /orders/9812: recorded 200, base build no response"));
    let agent = render::agent(&report, Some("r1"), true);
    assert!(agent.starts_with(
        "Echoproof verdict: PASS for the 1 reproduced requests only; 1 requests did not reproduce on the base build and were NOT verified"
    ));
    assert!(agent.contains("Do not report the change as fully verified"));
}
