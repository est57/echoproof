use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use crate::endpoint::endpoint_key;
use crate::outbound::OutboundCall;

/// Lowercased names; repeated headers are joined with ", ".
pub type Headers = BTreeMap<String, String>;

/// Accepts `[]` as no headers: PHP encodes an empty map as an empty list.
pub fn headers_or_empty_list<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Headers, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Map(Headers),
        List(Vec<Value>),
    }
    match Either::deserialize(deserializer)? {
        Either::Map(map) => Ok(map),
        Either::List(list) if list.is_empty() => Ok(Headers::new()),
        Either::List(_) => Err(serde::de::Error::custom("headers must be an object")),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    #[default]
    Empty,
    Json(Value),
    Text(String),
    Base64(String),
}

impl Body {
    pub fn from_bytes(bytes: &[u8], content_type: Option<&str>) -> Body {
        if bytes.is_empty() {
            return Body::Empty;
        }
        let declared_json = content_type.is_some_and(|ct| ct.contains("json"));
        if (declared_json || matches!(bytes.first(), Some(b'{' | b'[')))
            && let Ok(value) = serde_json::from_slice(bytes)
        {
            return Body::Json(value);
        }
        match std::str::from_utf8(bytes) {
            Ok(text) => Body::Text(text.to_string()),
            Err(_) => Body::Base64(STANDARD.encode(bytes)),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Body::Empty => Vec::new(),
            Body::Json(value) => serde_json::to_vec(value).unwrap_or_default(),
            Body::Text(text) => text.clone().into_bytes(),
            Body::Base64(encoded) => STANDARD.decode(encoded).unwrap_or_default(),
        }
    }

    pub fn preview(&self) -> Option<Value> {
        match self {
            Body::Empty => None,
            Body::Json(value) => Some(value.clone()),
            Body::Text(text) => Some(Value::String(text.clone())),
            Body::Base64(encoded) => Some(Value::String(format!(
                "<{} bytes binary>",
                encoded.len() * 3 / 4
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub method: String,
    /// Includes the query string.
    pub path: String,
    #[serde(default, deserialize_with = "headers_or_empty_list")]
    pub headers: Headers,
    #[serde(default)]
    pub body: Body,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub status: u16,
    #[serde(default, deserialize_with = "headers_or_empty_list")]
    pub headers: Headers,
    #[serde(default)]
    pub body: Body,
    #[serde(default)]
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exchange {
    pub id: String,
    #[serde(default)]
    pub recorded_at_ms: u64,
    pub request: Request,
    pub response: Response,
    /// Set when the request was deliberately not sent (safe mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// Calls to other services; `None` when they were not captured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound: Option<Vec<OutboundCall>>,
}

impl Exchange {
    pub fn endpoint(&self) -> String {
        endpoint_key(&self.request.method, &self.request.path)
    }
}

pub fn read_snapshot(path: &Path) -> Result<Vec<Exchange>> {
    let file =
        File::open(path).with_context(|| format!("cannot open snapshot {}", path.display()))?;
    let mut exchanges = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let exchange = serde_json::from_str(&line)
            .with_context(|| format!("{}:{}: invalid exchange", path.display(), index + 1))?;
        exchanges.push(exchange);
    }
    Ok(exchanges)
}

pub fn write_snapshot(path: &Path, exchanges: &[Exchange]) -> Result<()> {
    let mut writer = SnapshotWriter::create(path)?;
    for exchange in exchanges {
        writer.append(exchange)?;
    }
    Ok(())
}

/// Flushes after every line so a killed recorder leaves a valid file.
pub struct SnapshotWriter {
    inner: BufWriter<File>,
}

impl SnapshotWriter {
    pub fn create(path: &Path) -> Result<Self> {
        let file =
            File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
        Ok(Self {
            inner: BufWriter::new(file),
        })
    }

    pub fn append(&mut self, exchange: &Exchange) -> Result<()> {
        serde_json::to_writer(&mut self.inner, exchange)?;
        self.inner.write_all(b"\n")?;
        self.inner.flush()?;
        Ok(())
    }
}
