use serde_json::{Value, json};
use std::collections::HashMap;

use super::json::diff as diff_json;
use super::{Change, ChangeKind, DiffOptions, Severity, change};
use crate::endpoint::endpoint_key;
use crate::outbound::OutboundCall;
use crate::snapshot::Body;

/// Compares what two builds sent to other services. Calls pair up by method, host and path
/// (ids folded) in the order they were made; nothing is compared when the candidate's calls
/// were not captured.
pub(super) fn diff(
    baseline: Option<&[OutboundCall]>,
    candidate: Option<&[OutboundCall]>,
    options: &DiffOptions,
    out: &mut Vec<Change>,
) {
    let Some(candidate) = candidate else {
        return;
    };
    let before = labelled(baseline.unwrap_or_default());
    let after = labelled(candidate);
    let lookup: HashMap<&str, &OutboundCall> = after
        .iter()
        .map(|(label, call)| (label.as_str(), *call))
        .collect();
    let paired: HashMap<&str, &OutboundCall> = before
        .iter()
        .map(|(label, call)| (label.as_str(), *call))
        .collect();

    for (label, call) in &before {
        match lookup.get(label.as_str()) {
            Some(other) => compare(label, call, other, options, out),
            None => out.push(change(
                label,
                ChangeKind::CallRemoved,
                Severity::Behavior,
                Some(summary(call)),
                None,
            )),
        }
    }
    for (label, call) in after
        .iter()
        .filter(|(l, _)| !paired.contains_key(l.as_str()))
    {
        let severity = if call.unrecorded {
            Severity::Breaking
        } else {
            Severity::Behavior
        };
        out.push(change(
            label,
            ChangeKind::CallAdded,
            severity,
            None,
            Some(summary(call)),
        ));
    }
}

fn labelled(calls: &[OutboundCall]) -> Vec<(String, &OutboundCall)> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    calls
        .iter()
        .map(|call| {
            let key = folded_key(call);
            let n = seen.entry(key.clone()).or_default();
            let label = match *n {
                0 => format!("outbound[{key}]"),
                n => format!("outbound[{key}#{n}]"),
            };
            *n += 1;
            (label, call)
        })
        .collect()
}

fn folded_key(call: &OutboundCall) -> String {
    let key = call.key();
    let (method, rest) = key.split_once(' ').unwrap_or(("", key.as_str()));
    let folded = endpoint_key(method, &format!("/{rest}"));
    folded.replacen(" /", " ", 1)
}

fn compare(
    label: &str,
    a: &OutboundCall,
    b: &OutboundCall,
    options: &DiffOptions,
    out: &mut Vec<Change>,
) {
    if a.request.url != b.request.url {
        out.push(change(
            &format!("{label} url"),
            ChangeKind::ValueChanged,
            Severity::Behavior,
            Some(json!(a.request.url)),
            Some(json!(b.request.url)),
        ));
    }
    match (&a.request.body, &b.request.body) {
        (Body::Json(x), Body::Json(y)) => {
            let mut inner = Vec::new();
            diff_json(x, y, options, &mut inner);
            out.extend(inner.into_iter().map(|c| Change {
                path: format!("{label} {}", c.path),
                pattern: format!("{label} {}", c.pattern),
                ..c
            }));
        }
        (x, y) if x == y => {}
        (x, y) => out.push(change(
            &format!("{label} body"),
            ChangeKind::BodyChanged,
            Severity::Behavior,
            x.preview(),
            y.preview(),
        )),
    }
}

fn summary(call: &OutboundCall) -> Value {
    let mut value = json!({
        "method": call.request.method,
        "url": call.request.url,
        "body": call.request.body.preview(),
    });
    if call.unrecorded {
        value["unrecorded"] = json!(true);
    }
    value
}
