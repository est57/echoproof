use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

static UUID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .unwrap()
});
static ULID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9A-HJKMNP-TV-Z]{26}$").unwrap());
// Date-times only: a changed bare date (e.g. delivery date) is real behavior.
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?$").unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Volatile {
    Uuid,
    Ulid,
    Timestamp,
}

pub fn classify(value: &str) -> Option<Volatile> {
    if UUID.is_match(value) {
        Some(Volatile::Uuid)
    } else if ULID.is_match(value) {
        Some(Volatile::Ulid)
    } else if TIMESTAMP.is_match(value) {
        Some(Volatile::Timestamp)
    } else {
        None
    }
}

pub fn is_uuid(value: &str) -> bool {
    UUID.is_match(value)
}

pub fn is_ulid(value: &str) -> bool {
    ULID.is_match(value)
}

/// Two different UUIDs (or ULIDs, or date-times) are noise, not behavior.
pub fn same_volatile_shape(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::String(x), Value::String(y)) => {
            classify(x).is_some_and(|kind| classify(y) == Some(kind))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_shapes() {
        assert_eq!(
            classify("2f1c6a0e-8f7b-4a51-9d2b-0b8e7c7d9f10"),
            Some(Volatile::Uuid)
        );
        assert_eq!(
            classify("2026-10-14T07:01:02.123Z"),
            Some(Volatile::Timestamp)
        );
        assert_eq!(
            classify("2026-10-14T14:01:02+07:00"),
            Some(Volatile::Timestamp)
        );
        assert_eq!(classify("01JA2B3C4D5E6F7G8H9J0K1M2N"), Some(Volatile::Ulid));
        assert_eq!(classify("2026-10-14"), None);
        assert_eq!(classify("HEMAT10"), None);
    }

    #[test]
    fn shapes_must_match() {
        assert!(same_volatile_shape(
            &json!("2026-10-14T07:01:02Z"),
            &json!("2026-10-15T08:00:00Z")
        ));
        assert!(!same_volatile_shape(
            &json!("2026-10-14T07:01:02Z"),
            &json!("abc")
        ));
        assert!(!same_volatile_shape(&json!(1), &json!(2)));
    }
}
