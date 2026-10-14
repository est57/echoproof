use serde_json::Value;

use crate::snapshot::{Body, Exchange};

pub const PLACEHOLDER: &str = "{origin}";

/// Replaces a build's own origin (e.g. `http://127.0.0.1:8003`) in response
/// bodies and headers, so builds running on different addresses compare equal.
pub fn normalize(exchanges: &mut [Exchange], origin: &str) {
    if origin.is_empty() {
        return;
    }
    for exchange in exchanges {
        let response = &mut exchange.response;
        for value in response.headers.values_mut() {
            replace(value, origin);
        }
        match &mut response.body {
            Body::Json(value) => walk(value, origin),
            Body::Text(text) => replace(text, origin),
            Body::Empty | Body::Base64(_) => {}
        }
    }
}

fn walk(value: &mut Value, origin: &str) {
    match value {
        Value::String(text) => replace(text, origin),
        Value::Array(items) => items.iter_mut().for_each(|item| walk(item, origin)),
        Value::Object(map) => map.values_mut().for_each(|item| walk(item, origin)),
        _ => {}
    }
}

fn replace(text: &mut String, origin: &str) {
    if text.contains(origin) {
        *text = text.replace(origin, PLACEHOLDER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{Request, Response};
    use serde_json::json;

    #[test]
    fn replaces_origin_in_body_and_headers() {
        let mut exchanges = vec![Exchange {
            id: "a".into(),
            recorded_at_ms: 0,
            request: Request {
                method: "GET".into(),
                path: "/".into(),
                headers: Default::default(),
                body: Body::Empty,
            },
            response: Response {
                status: 200,
                headers: [(
                    "location".to_string(),
                    "http://127.0.0.1:8003/x".to_string(),
                )]
                .into(),
                body: Body::Json(
                    json!({"path": "http://127.0.0.1:8003/a", "items": [{"img": "http://127.0.0.1:8003/i.jpg"}], "n": 1}),
                ),
                latency_ms: 0,
            },
            skipped: None,
            outbound: None,
        }];
        normalize(&mut exchanges, "http://127.0.0.1:8003");
        let response = &exchanges[0].response;
        assert_eq!(response.headers["location"], "{origin}/x");
        assert_eq!(
            response.body,
            Body::Json(json!({"path": "{origin}/a", "items": [{"img": "{origin}/i.jpg"}], "n": 1}))
        );
    }
}
