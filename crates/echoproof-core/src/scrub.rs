//! Removes secret values that leak into other places: an API key that a third party echoes back
//! in its response body, or a bearer token repeated in a URL. Values are collected where the
//! redaction rules apply, then erased everywhere in the exchange.

use serde_json::Value;
use std::collections::BTreeSet;

use crate::sanitize::{REDACTED, Sanitizer};
use crate::snapshot::{Body, Exchange, Headers};

/// Shorter values would erase ordinary text.
const MIN_LEN: usize = 8;

pub(crate) fn secrets(sanitizer: &Sanitizer, exchange: &Exchange) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut headers = |h: &Headers| {
        for (name, value) in h {
            if sanitizer.header_is_secret(name) {
                add(&mut found, value);
            }
        }
    };
    headers(&exchange.request.headers);
    headers(&exchange.response.headers);
    for call in exchange.outbound.iter().flatten() {
        headers(&call.request.headers);
        headers(&call.response.headers);
    }
    query(sanitizer, &exchange.request.path, &mut found);
    body(sanitizer, &exchange.request.body, &mut found);
    body(sanitizer, &exchange.response.body, &mut found);
    for call in exchange.outbound.iter().flatten() {
        query(sanitizer, &call.request.url, &mut found);
        body(sanitizer, &call.request.body, &mut found);
        body(sanitizer, &call.response.body, &mut found);
    }
    found
}

pub(crate) fn scrub(exchange: &mut Exchange, secrets: &BTreeSet<String>) {
    if secrets.is_empty() {
        return;
    }
    let text = |s: &mut String| {
        for secret in secrets {
            if s.contains(secret.as_str()) {
                *s = s.replace(secret.as_str(), REDACTED);
            }
        }
    };
    let mut parts = Parts { text };
    parts.string(&mut exchange.request.path);
    parts.headers(&mut exchange.request.headers);
    parts.body(&mut exchange.request.body);
    parts.headers(&mut exchange.response.headers);
    parts.body(&mut exchange.response.body);
    for call in exchange.outbound.iter_mut().flatten() {
        parts.string(&mut call.request.url);
        parts.headers(&mut call.request.headers);
        parts.body(&mut call.request.body);
        parts.headers(&mut call.response.headers);
        parts.body(&mut call.response.body);
    }
}

struct Parts<F: FnMut(&mut String)> {
    text: F,
}

impl<F: FnMut(&mut String)> Parts<F> {
    fn string(&mut self, s: &mut String) {
        (self.text)(s);
    }

    fn headers(&mut self, headers: &mut Headers) {
        headers.values_mut().for_each(|v| (self.text)(v));
    }

    fn body(&mut self, body: &mut Body) {
        match body {
            Body::Json(value) => self.json(value),
            Body::Text(text) => (self.text)(text),
            Body::Empty | Body::Base64(_) => {}
        }
    }

    fn json(&mut self, value: &mut Value) {
        match value {
            Value::String(s) => (self.text)(s),
            Value::Array(items) => items.iter_mut().for_each(|v| self.json(v)),
            Value::Object(map) => map.values_mut().for_each(|v| self.json(v)),
            _ => {}
        }
    }
}

fn add(found: &mut BTreeSet<String>, value: &str) {
    let value = value.trim();
    if value == REDACTED {
        return;
    }
    if value.len() >= MIN_LEN {
        found.insert(value.to_string());
    }
    // "Bearer sk_live_..." leaks as the bare token.
    for part in value.split_whitespace() {
        if part.len() >= MIN_LEN {
            found.insert(part.to_string());
        }
    }
}

fn query(sanitizer: &Sanitizer, url: &str, found: &mut BTreeSet<String>) {
    let Some((_, query)) = url.split_once('?') else {
        return;
    };
    for pair in query.split('&') {
        if let Some((name, value)) = pair.split_once('=') {
            let value = Value::String(decode(value));
            if sanitizer.field_is_secret(name, &value) {
                add(found, value.as_str().unwrap_or_default());
            }
        }
    }
}

fn body(sanitizer: &Sanitizer, body: &Body, found: &mut BTreeSet<String>) {
    if let Body::Json(value) = body {
        json(sanitizer, value, found);
    }
}

fn json(sanitizer: &Sanitizer, value: &Value, found: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                match child {
                    Value::String(s) if sanitizer.field_is_secret(key, child) => add(found, s),
                    _ => json(sanitizer, child, found),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| json(sanitizer, v, found)),
        _ => {}
    }
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(high), Some(low)) => {
                    out.push((high * 16 + low) as u8);
                    i += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            byte => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::decode;
    use crate::config::SanitizeConfig;
    use crate::outbound::{OutboundCall, OutboundRequest};
    use crate::sanitize::Sanitizer;
    use crate::snapshot::{Body, Exchange, Request, Response};
    use serde_json::json;

    fn exchange_with_echo() -> Exchange {
        let echoed = json!({"url": "https://api.test/v2?api_key=sk_live_abc123", "headers": {"Authorization": "Bearer tok_9f8e7d6c5b"}, "amount": 18000});
        Exchange {
            id: "e".into(),
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
                body: Body::Text("charged with tok_9f8e7d6c5b".into()),
                latency_ms: 0,
            },
            skipped: None,
            outbound: Some(vec![OutboundCall {
                request: OutboundRequest {
                    method: "POST".into(),
                    url: "https://api.test/v2?api_key=sk_live_abc123&currency=IDR".into(),
                    headers: [(
                        "authorization".to_string(),
                        "Bearer tok_9f8e7d6c5b".to_string(),
                    )]
                    .into(),
                    body: Body::Json(json!({"amount": 18000, "note": "pin 1234"})),
                },
                response: Response {
                    status: 200,
                    headers: Default::default(),
                    body: Body::Json(echoed),
                    latency_ms: 0,
                },
                unrecorded: false,
            }]),
        }
    }

    #[test]
    fn secrets_echoed_by_a_third_party_are_erased_everywhere() {
        let mut exchange = exchange_with_echo();
        Sanitizer::new(&SanitizeConfig::default()).apply(&mut exchange);
        let text = serde_json::to_string(&exchange).unwrap();
        assert!(!text.contains("sk_live_abc123"), "{text}");
        assert!(!text.contains("tok_9f8e7d6c5b"), "{text}");
        assert!(
            text.contains("18000") && text.contains("currency=IDR") && text.contains("pin 1234")
        );
    }

    #[test]
    fn decodes_queries_without_panicking_on_odd_input() {
        assert_eq!(decode("sk%5Flive+x"), "sk_live x");
        assert_eq!(decode("%E2%82%AC%"), "\u{20ac}%");
        assert_eq!(decode("%€9"), "%€9");
    }
}
