use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use super::json::{Segment, Walker};
use super::{ChangeKind, Severity};

const PREFERRED_KEYS: [&str; 3] = ["id", "uuid", "code"];

fn identity(item: &Value, key: &str) -> Option<String> {
    match item.as_object()?.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn unique_on(items: &[Value], key: &str) -> bool {
    let mut seen = HashSet::new();
    items
        .iter()
        .all(|item| identity(item, key).is_some_and(|id| seen.insert(id)))
}

/// A key that identifies every object in both arrays, when most identities
/// survive between them; otherwise items are compared by position.
pub(super) fn identity_key(a: &[Value], b: &[Value]) -> Option<String> {
    let first = a.first().or(b.first())?.as_object()?;
    let mut candidates: Vec<&String> = first
        .keys()
        .filter(|k| {
            PREFERRED_KEYS.contains(&k.as_str())
                || ["_id", "_no", "_num", "_code"]
                    .iter()
                    .any(|s| k.ends_with(s))
        })
        .collect();
    candidates.sort_by_key(|k| {
        (
            PREFERRED_KEYS
                .iter()
                .position(|p| p == k)
                .unwrap_or(PREFERRED_KEYS.len()),
            (*k).clone(),
        )
    });
    candidates
        .into_iter()
        .find(|key| {
            if !unique_on(a, key) || !unique_on(b, key) {
                return false;
            }
            let b_ids: HashSet<String> = b.iter().filter_map(|item| identity(item, key)).collect();
            let shared = a
                .iter()
                .filter_map(|item| identity(item, key))
                .filter(|id| b_ids.contains(id))
                .count();
            shared * 2 >= a.len().max(b.len())
        })
        .cloned()
}

impl Walker<'_> {
    pub(super) fn walk_array(&mut self, a: &[Value], b: &[Value]) {
        if a.len() != b.len() {
            self.push(
                ChangeKind::LengthChanged,
                Severity::Behavior,
                Some(json!(a.len())),
                Some(json!(b.len())),
            );
        }
        match identity_key(a, b) {
            Some(key) => self.walk_keyed(a, b, &key),
            None => {
                for (i, (a_item, b_item)) in a.iter().zip(b).enumerate() {
                    self.path.push(Segment::Index(i));
                    self.walk(a_item, b_item);
                    self.path.pop();
                }
            }
        }
    }

    fn walk_keyed(&mut self, a: &[Value], b: &[Value], key: &str) {
        let b_index: HashMap<String, usize> = b
            .iter()
            .enumerate()
            .filter_map(|(j, item)| identity(item, key).map(|id| (id, j)))
            .collect();
        let a_ids: HashSet<String> = a.iter().filter_map(|item| identity(item, key)).collect();
        for (i, item) in a.iter().enumerate() {
            self.path.push(Segment::Index(i));
            match identity(item, key).and_then(|id| b_index.get(&id)) {
                Some(&j) => self.walk(item, &b[j]),
                None => self.push(
                    ChangeKind::ItemRemoved,
                    Severity::Behavior,
                    Some(item.clone()),
                    None,
                ),
            }
            self.path.pop();
        }
        for (j, item) in b.iter().enumerate() {
            if identity(item, key).is_some_and(|id| !a_ids.contains(&id)) {
                self.path.push(Segment::Index(j));
                self.push(
                    ChangeKind::ItemAdded,
                    Severity::Behavior,
                    None,
                    Some(item.clone()),
                );
                self.path.pop();
            }
        }
        let shared = |items: &[Value], other: &dyn Fn(&String) -> bool| -> Vec<String> {
            items
                .iter()
                .filter_map(|item| identity(item, key))
                .filter(|id| other(id))
                .collect()
        };
        let a_order = shared(a, &|id| b_index.contains_key(id));
        let b_order = shared(b, &|id| a_ids.contains(id));
        if a_order != b_order {
            self.push_with_suffix(
                "#order",
                ChangeKind::OrderChanged,
                Severity::Behavior,
                Some(json!(a_order)),
                Some(json!(b_order)),
            );
        }
    }
}
