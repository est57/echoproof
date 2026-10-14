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
