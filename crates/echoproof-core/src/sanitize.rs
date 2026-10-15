use serde_json::Value;
use std::collections::HashSet;

use crate::config::SanitizeConfig;
use crate::snapshot::{Body, Exchange, Headers};

pub const REDACTED: &str = "[REDACTED]";

/// Field rules: an exact name (`pin`) redacts any value; a pattern with `*`
/// (`*token*`, `*_hash`) redacts string values only, so numbers such as
/// `max_tokens` stay comparable.
#[derive(Debug, Clone)]
pub struct Sanitizer {
    headers: HashSet<String>,
    fields: HashSet<String>,
    patterns: Vec<String>,
}

impl Sanitizer {
    pub fn new(config: &SanitizeConfig) -> Self {
        let lower = |items: &[String]| {
            items
                .iter()
                .map(|s| s.to_ascii_lowercase())
                .collect::<Vec<_>>()
        };
        let (patterns, fields): (Vec<_>, Vec<_>) = lower(&config.redact_fields)
            .into_iter()
            .partition(|f| f.contains('*'));
        Self {
            headers: lower(&config.redact_headers).into_iter().collect(),
            fields: fields.into_iter().collect(),
            patterns,
        }
    }

    pub fn apply(&self, exchange: &mut Exchange) {
        self.redact_headers(&mut exchange.request.headers);
        self.redact_headers(&mut exchange.response.headers);
        self.redact_body(&mut exchange.request.body);
        self.redact_body(&mut exchange.response.body);
        for call in exchange.outbound.iter_mut().flatten() {
            call.request.url = self.redact_url(&call.request.url);
            self.redact_headers(&mut call.request.headers);
            self.redact_headers(&mut call.response.headers);
            self.redact_body(&mut call.request.body);
            self.redact_body(&mut call.response.body);
        }
    }

    /// Query parameters follow the field rules: `?api_key=...` is as secret as a body field.
    fn redact_url(&self, url: &str) -> String {
        let Some((base, query)) = url.split_once('?') else {
            return url.to_string();
        };
        let pairs: Vec<String> = query
            .split('&')
            .map(|pair| match pair.split_once('=') {
                Some((name, value)) => {
                    let key = name.to_ascii_lowercase();
                    let by_pattern = self.patterns.iter().any(|p| wildcard(p, &key));
                    if self.fields.contains(&key) || (by_pattern && !value.is_empty()) {
                        format!("{name}=%5BREDACTED%5D")
                    } else {
                        pair.to_string()
                    }
                }
                None => pair.to_string(),
            })
            .collect();
        format!("{base}?{}", pairs.join("&"))
    }

    fn redact_headers(&self, headers: &mut Headers) {
        for (name, value) in headers.iter_mut() {
            if self.headers.contains(name) {
                *value = REDACTED.to_string();
            }
        }
    }

    fn redact_body(&self, body: &mut Body) {
        if let Body::Json(value) = body {
            self.redact_json(value);
        }
    }

    fn redact_json(&self, value: &mut Value) {
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    let key = key.to_ascii_lowercase();
                    let by_pattern =
                        child.is_string() && self.patterns.iter().any(|p| wildcard(p, &key));
                    if self.fields.contains(&key) || by_pattern {
                        *child = Value::String(REDACTED.to_string());
                    } else {
                        self.redact_json(child);
                    }
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| self.redact_json(item)),
            _ => {}
        }
    }
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !text.starts_with(first) || !text.ends_with(last) || text.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{Request, Response};
    use serde_json::json;

    fn exchange(request: Value, response: Value) -> Exchange {
        Exchange {
            id: "1".into(),
            recorded_at_ms: 0,
            request: Request {
                method: "POST".into(),
                path: "/login".into(),
                headers: [("authorization".to_string(), "Bearer abc".to_string())].into(),
                body: Body::Json(request),
            },
            response: Response {
                status: 200,
                headers: Headers::new(),
                body: Body::Json(response),
                latency_ms: 0,
            },
            skipped: None,
            outbound: None,
        }
    }

    #[test]
    fn redacts_nested_fields_and_headers() {
        let mut ex = exchange(
            json!({"user": {"Password": "hunter2", "name": "a"}}),
            json!([{"token": "xyz"}]),
        );
        Sanitizer::new(&SanitizeConfig::default()).apply(&mut ex);
        assert_eq!(ex.request.headers["authorization"], REDACTED);
        assert_eq!(
            ex.request.body,
            Body::Json(json!({"user": {"Password": REDACTED, "name": "a"}}))
        );
        assert_eq!(ex.response.body, Body::Json(json!([{"token": REDACTED}])));
    }

    #[test]
    fn patterns_redact_strings_only() {
        let mut ex = exchange(
            json!({"pin_hash": "pbkdf2$x", "api_token": "t", "max_tokens": 16000, "user_password": "p"}),
            json!({}),
        );
        Sanitizer::new(&SanitizeConfig::default()).apply(&mut ex);
        let expected = json!({"pin_hash": REDACTED, "api_token": REDACTED, "max_tokens": 16000, "user_password": REDACTED});
        assert_eq!(ex.request.body, Body::Json(expected));
    }

    #[test]
    fn wildcard_matching() {
        assert!(wildcard("*token*", "api_token_value"));
        assert!(wildcard("*_hash", "pin_hash"));
        assert!(!wildcard("*_hash", "hash_value"));
        assert!(wildcard("pin*", "pin"));
    }

    #[test]
    fn redacts_secret_query_parameters_of_outbound_calls() {
        let sanitizer = Sanitizer::new(&SanitizeConfig::default());
        let url =
            sanitizer.redact_url("https://pay.test/v2?api_key=k1&currency=IDR&access_token=t");
        assert_eq!(
            url,
            "https://pay.test/v2?api_key=%5BREDACTED%5D&currency=IDR&access_token=%5BREDACTED%5D"
        );
    }
}
