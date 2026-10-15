//! Carries IDs created during a replay forward. When a replayed response holds a different
//! identity than the recording (order 9812 was created as order 57), later requests that refer
//! to the recorded value for the same entity are rewritten. Entities come from field names
//! (`order_id`), enclosing keys (`"order": {"id": ..}`) or the request path (`POST /orders`),
//! so the ID of an order never rewrites a table number that happens to be equal.

use serde_json::Value;

use crate::endpoint::is_id_like;

const OWN_KEYS: [&str; 2] = ["id", "uuid"];
const SUFFIXES: [&str; 5] = ["_id", "_uuid", "_no", "_num", "_code"];
const WRAPPERS: [&str; 3] = ["result", "data", "payload"];

#[derive(Debug, Default, Clone)]
pub struct IdMap {
    entries: Vec<(String, String, Value)>,
}

impl IdMap {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Learns from one recorded response and its replay.
    pub fn learn(&mut self, path: &str, recorded: &Value, replayed: &Value) {
        let context = resource(path);
        self.walk(recorded, replayed, context.as_deref());
    }

    pub fn rewrite_path(&self, path: &str) -> String {
        if self.is_empty() {
            return path.to_string();
        }
        let (route, query) = match path.split_once('?') {
            Some((route, query)) => (route, Some(query)),
            None => (path, None),
        };
        let segments: Vec<&str> = route.split('/').collect();
        let mut out: Vec<String> = Vec::with_capacity(segments.len());
        for (i, segment) in segments.iter().enumerate() {
            let replaced = (i > 0)
                .then(|| self.lookup(&entity(segments[i - 1]), segment))
                .flatten();
            out.push(replaced.map_or_else(|| segment.to_string(), text));
        }
        let mut rewritten = out.join("/");
        if let Some(query) = query {
            let pairs: Vec<String> = query
                .split('&')
                .map(|pair| match pair.split_once('=') {
                    Some((key, value)) => {
                        match field_entity(key).and_then(|e| self.lookup(&e, value)) {
                            Some(to) => format!("{key}={}", text(to)),
                            None => pair.to_string(),
                        }
                    }
                    None => pair.to_string(),
                })
                .collect();
            rewritten = format!("{rewritten}?{}", pairs.join("&"));
        }
        rewritten
    }

    pub fn rewrite_body(&self, path: &str, body: &mut Value) {
        if !self.is_empty() {
            let context = resource(path);
            self.rewrite_value(body, context.as_deref());
        }
    }

    fn rewrite_value(&self, value: &mut Value, context: Option<&str>) {
        match value {
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    let owner = key_entity(key, context);
                    let found = owner
                        .zip(scalar(child))
                        .and_then(|(e, s)| self.lookup(&e, &s))
                        .cloned();
                    match found {
                        Some(to) => *child = typed_like(child, &to),
                        None => self.rewrite_value(child, child_context(key, context)),
                    }
                }
            }
            Value::Array(items) => items
                .iter_mut()
                .for_each(|v| self.rewrite_value(v, context)),
            _ => {}
        }
    }

    fn walk(&mut self, a: &Value, b: &Value, context: Option<&str>) {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                for (key, av) in x {
                    let Some(bv) = y.get(key) else { continue };
                    if let Some(owner) = key_entity(key, context) {
                        self.record(owner, av, bv);
                    }
                    self.walk(av, bv, child_context(key, context));
                }
            }
            (Value::Array(x), Value::Array(y)) => {
                for (av, bv) in x.iter().zip(y) {
                    self.walk(av, bv, context);
                }
            }
            _ => {}
        }
    }

    fn record(&mut self, owner: String, from: &Value, to: &Value) {
        let (Some(key), Some(_)) = (scalar(from), scalar(to)) else {
            return;
        };
        if from == to || !eligible(&key, from) {
            return;
        }
        self.entries.retain(|(e, f, _)| !(*e == owner && *f == key));
        self.entries.push((owner, key, to.clone()));
    }

    fn lookup(&self, owner: &str, value: &str) -> Option<&Value> {
        self.entries
            .iter()
            .find(|(e, f, _)| e == owner && f == value)
            .map(|(_, _, to)| to)
    }
}

/// `orders`, `Temp-Orders` and `order` all name the entity `order` / `temp_order`.
fn entity(name: &str) -> String {
    let name = name.to_ascii_lowercase().replace('-', "_");
    name.strip_suffix('s').unwrap_or(&name).to_string()
}

fn field_entity(key: &str) -> Option<String> {
    let key = key.to_ascii_lowercase();
    SUFFIXES
        .iter()
        .find_map(|s| key.strip_suffix(s))
        .filter(|prefix| !prefix.is_empty())
        .map(entity)
}

fn key_entity(key: &str, context: Option<&str>) -> Option<String> {
    if OWN_KEYS.contains(&key.to_ascii_lowercase().as_str()) {
        context.map(entity)
    } else {
        field_entity(key)
    }
}

fn child_context<'a>(key: &'a str, context: Option<&'a str>) -> Option<&'a str> {
    if WRAPPERS.contains(&key.to_ascii_lowercase().as_str()) {
        context
    } else {
        Some(key)
    }
}

/// The last path segment that names a resource: `/pos-sync/orders/57/items` -> `items`.
fn resource(path: &str) -> Option<String> {
    let route = path.split('?').next().unwrap_or(path);
    route
        .split('/')
        .rev()
        .find(|s| !s.is_empty() && !is_id_like(s))
        .map(str::to_string)
}

fn scalar(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Small numbers and one-character strings collide too easily to rewrite.
fn eligible(key: &str, value: &Value) -> bool {
    match value {
        Value::Number(_) => key.parse::<i64>().map_or(true, |n| n.abs() >= 10),
        Value::String(s) => s.chars().count() >= 2,
        _ => false,
    }
}

fn text(value: &Value) -> String {
    scalar(value).unwrap_or_default()
}

fn typed_like(original: &Value, to: &Value) -> Value {
    match original {
        Value::String(_) => Value::String(text(to)),
        _ => to.clone(),
    }
}
