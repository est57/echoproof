use crate::glob::match_segments;
use crate::noise;

pub fn endpoint_key(method: &str, path: &str) -> String {
    let path = path.split('?').next().unwrap_or("/");
    let segments: Vec<&str> = path
        .split('/')
        .map(|s| if is_id_like(s) { "{id}" } else { s })
        .collect();
    format!("{} {}", method.to_ascii_uppercase(), segments.join("/"))
}

fn is_id_like(segment: &str) -> bool {
    !segment.is_empty()
        && (segment.bytes().all(|b| b.is_ascii_digit())
            || noise::is_uuid(segment)
            || noise::is_ulid(segment)
            || (segment.len() >= 16 && segment.bytes().all(|b| b.is_ascii_hexdigit())))
}

/// `POST /orders`, `GET /products/*`, `/admin/**` or `DELETE`.
#[derive(Debug, Clone)]
pub struct EndpointPattern {
    raw: String,
    method: Option<String>,
    segments: Vec<String>,
}

impl EndpointPattern {
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        let (method, path) = match raw.split_once(char::is_whitespace) {
            Some((method, path)) => (Some(method.to_ascii_uppercase()), path.trim()),
            None if raw.starts_with('/') => (None, raw),
            None => (Some(raw.to_ascii_uppercase()), "/**"),
        };
        let path = path.split('?').next().unwrap_or("/");
        Self {
            raw: raw.to_string(),
            method: method.filter(|m| m != "*"),
            segments: path.split('/').map(str::to_string).collect(),
        }
    }

    pub fn matches(&self, endpoint: &str) -> bool {
        let (method, path) = endpoint.split_once(' ').unwrap_or(("", endpoint));
        if self.method.as_deref().is_some_and(|m| m != method) {
            return false;
        }
        let segments: Vec<&str> = path.split('/').collect();
        match_segments(&self.segments, &segments)
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ids() {
        assert_eq!(
            endpoint_key("get", "/products/42?x=1"),
            "GET /products/{id}"
        );
        assert_eq!(
            endpoint_key("GET", "/orders/2f1c6a0e-8f7b-4a51-9d2b-0b8e7c7d9f10/items"),
            "GET /orders/{id}/items"
        );
        assert_eq!(endpoint_key("POST", "/orders"), "POST /orders");
    }

    #[test]
    fn patterns() {
        assert!(EndpointPattern::parse("GET /products/*").matches("GET /products/{id}"));
        assert!(EndpointPattern::parse("/products/{id}").matches("GET /products/{id}"));
        assert!(!EndpointPattern::parse("POST /products/*").matches("GET /products/{id}"));
        assert!(EndpointPattern::parse("/admin/**").matches("DELETE /admin/users/{id}"));
        assert!(EndpointPattern::parse("DELETE").matches("DELETE /admin/users/{id}"));
        assert!(!EndpointPattern::parse("POST /orders").matches("POST /orders/{id}"));
    }
}
