use serde::{Deserialize, Serialize};

use crate::snapshot::{Body, Headers, Response, headers_or_empty_list};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutboundRequest {
    pub method: String,
    pub url: String,
    #[serde(default, deserialize_with = "headers_or_empty_list")]
    pub headers: Headers,
    #[serde(default)]
    pub body: Body,
}

/// A call the service made to another service while handling one request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundCall {
    pub request: OutboundRequest,
    pub response: Response,
    /// Set by a replay-mode build when no recorded call matched.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unrecorded: bool,
}

impl OutboundRequest {
    /// Matching key shared with echoproof/laravel: method, scheme, host, non-default port and
    /// path, without the query string.
    pub fn route(&self) -> String {
        let (scheme, rest) = self
            .url
            .split_once("://")
            .unwrap_or(("", self.url.as_str()));
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        let (host, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let scheme = scheme.to_ascii_lowercase();
        let host = host.to_ascii_lowercase();
        let default_port = match scheme.as_str() {
            "https" => ":443",
            "http" => ":80",
            _ => "",
        };
        let host = host
            .strip_suffix(default_port)
            .filter(|_| !default_port.is_empty())
            .unwrap_or(&host);
        format!(
            "{} {scheme}://{host}{path}",
            self.method.to_ascii_uppercase()
        )
    }
}

impl OutboundCall {
    /// `POST api.example.com/v2/charges`: method, host and path, without scheme or query.
    pub fn key(&self) -> String {
        let url = &self.request.url;
        let rest = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        format!(
            "{} {}",
            self.request.method.to_ascii_uppercase(),
            rest.to_ascii_lowercase()
        )
    }

    pub fn query(&self) -> Option<&str> {
        self.request.url.split_once('?').map(|(_, q)| q)
    }
}
