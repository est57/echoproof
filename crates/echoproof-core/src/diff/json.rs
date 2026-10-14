use serde_json::Value;

use super::pattern::INDEX;
use super::{Change, ChangeKind, DiffOptions, Severity};
use crate::noise;

pub(super) enum Segment {
    Key(String),
    Index(usize),
}

pub(super) struct Walker<'a> {
    pub(super) options: &'a DiffOptions,
    pub(super) path: Vec<Segment>,
    out: &'a mut Vec<Change>,
}

pub(super) fn diff(a: &Value, b: &Value, options: &DiffOptions, out: &mut Vec<Change>) {
    Walker {
        options,
        path: Vec::new(),
        out,
    }
    .walk(a, b);
}

impl Walker<'_> {
    fn render(&self, fold: bool) -> String {
        let mut text = String::from("$");
        for segment in &self.path {
            match segment {
                Segment::Key(key) => {
                    text.push('.');
                    text.push_str(key);
                }
                Segment::Index(_) if fold => text.push_str("[*]"),
                Segment::Index(i) => text.push_str(&format!("[{i}]")),
            }
        }
        text
    }

    fn ignored(&self) -> bool {
        if self.options.ignore.is_empty() {
            return false;
        }
        let segments: Vec<&str> = self
            .path
            .iter()
            .map(|s| match s {
                Segment::Key(key) => key.as_str(),
                Segment::Index(_) => INDEX,
            })
            .collect();
        self.options.ignore.iter().any(|p| p.matches(&segments))
    }

    pub(super) fn push(
        &mut self,
        kind: ChangeKind,
        severity: Severity,
        before: Option<Value>,
        after: Option<Value>,
    ) {
        self.push_with_suffix("", kind, severity, before, after);
    }

    /// A suffix keeps a pattern such as `$.items#order` from covering `$.items[*].x` as noise.
    pub(super) fn push_with_suffix(
        &mut self,
        suffix: &str,
        kind: ChangeKind,
        severity: Severity,
        before: Option<Value>,
        after: Option<Value>,
    ) {
        if self.ignored() {
            return;
        }
        let change = Change {
            path: self.render(false) + suffix,
            pattern: self.render(true) + suffix,
            kind,
            severity,
            before,
            after,
        };
        self.out.push(change);
    }

    pub(super) fn walk(&mut self, a: &Value, b: &Value) {
        if self.ignored() {
            return;
        }
        match (a, b) {
            (Value::Object(a_map), Value::Object(b_map)) => {
                for (key, a_value) in a_map {
                    self.path.push(Segment::Key(key.clone()));
                    match b_map.get(key) {
                        Some(b_value) => self.walk(a_value, b_value),
                        None => self.push(
                            ChangeKind::FieldRemoved,
                            Severity::Breaking,
                            Some(a_value.clone()),
                            None,
                        ),
                    }
                    self.path.pop();
                }
                for (key, b_value) in b_map.iter().filter(|(k, _)| !a_map.contains_key(*k)) {
                    self.path.push(Segment::Key(key.clone()));
                    self.push(
                        ChangeKind::FieldAdded,
                        Severity::Additive,
                        None,
                        Some(b_value.clone()),
                    );
                    self.path.pop();
                }
            }
            (Value::Array(a_items), Value::Array(b_items)) => self.walk_array(a_items, b_items),
            (Value::Number(x), Value::Number(y)) => {
                if x != y && x.as_f64() != y.as_f64() {
                    self.push(
                        ChangeKind::ValueChanged,
                        Severity::Behavior,
                        Some(a.clone()),
                        Some(b.clone()),
                    );
                }
            }
            _ if a == b => {}
            _ if std::mem::discriminant(a) == std::mem::discriminant(b) => {
                if !(self.options.auto_noise && noise::same_volatile_shape(a, b)) {
                    self.push(
                        ChangeKind::ValueChanged,
                        Severity::Behavior,
                        Some(a.clone()),
                        Some(b.clone()),
                    );
                }
            }
            _ => self.push(
                ChangeKind::TypeChanged,
                Severity::Breaking,
                Some(a.clone()),
                Some(b.clone()),
            ),
        }
    }
}
