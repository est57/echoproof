use anyhow::{Result, bail};
use axum::body::Bytes;
use axum::http::HeaderMap;
use axum::http::header::{CONTENT_TYPE, HeaderName};
use parity_core::snapshot::Headers;

pub const MAX_BODY: usize = 32 * 1024 * 1024;

/// Per-hop headers, plus ones the client sets itself. Without `accept-encoding`
/// bodies stay readable.
pub fn is_hop_header(name: &str) -> bool {
    matches!(
        name,
        "connection"
            | "keep-alive"
            | "proxy-connection"
            | "transfer-encoding"
            | "te"
            | "trailer"
            | "upgrade"
            | "host"
            | "content-length"
            | "accept-encoding"
    )
}

pub fn forwardable(name: &HeaderName) -> bool {
    !is_hop_header(name.as_str())
}

pub fn to_headers(map: &HeaderMap) -> Headers {
    let mut headers = Headers::new();
    for (name, value) in map {
        let Ok(value) = value.to_str() else { continue };
        headers
            .entry(name.as_str().to_string())
            .and_modify(|existing: &mut String| {
                existing.push_str(", ");
                existing.push_str(value);
            })
            .or_insert_with(|| value.to_string());
    }
    headers
}

pub fn content_type(map: &HeaderMap) -> Option<&str> {
    map.get(CONTENT_TYPE).and_then(|v| v.to_str().ok())
}

pub async fn read_body(mut response: reqwest::Response) -> Result<Bytes> {
    if response
        .content_length()
        .is_some_and(|n| n > MAX_BODY as u64)
    {
        bail!("response body exceeds {MAX_BODY} bytes");
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > MAX_BODY {
            bail!("response body exceeds {MAX_BODY} bytes");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Bytes::from(body))
}
