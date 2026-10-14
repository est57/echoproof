use serde_json::Value;

use crate::diff::ChangeKind;

pub fn percent(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}

pub fn kind_label(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::StatusChanged => "status changed",
        ChangeKind::HeaderChanged => "header changed",
        ChangeKind::BodyChanged => "body changed",
        ChangeKind::FieldRemoved => "removed",
        ChangeKind::TypeChanged => "type changed",
        ChangeKind::ValueChanged => "value changed",
        ChangeKind::LengthChanged => "length changed",
        ChangeKind::ItemRemoved => "item removed",
        ChangeKind::ItemAdded => "item added",
        ChangeKind::OrderChanged => "order changed",
        ChangeKind::FieldAdded => "added",
    }
}

/// Compact single-line JSON, truncated for display.
pub fn short(value: &Option<Value>) -> String {
    const MAX: usize = 120;
    let text = match value {
        None => "(absent)".to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
    };
    if text.chars().count() > MAX {
        format!("{}...", text.chars().take(MAX).collect::<String>())
    } else {
        text
    }
}
