use crate::glob::match_segments;

pub(super) const INDEX: &str = "[]";

/// `updated_at` matches at any depth; `$.meta.id` and `items[*].id` are anchored at the root.
#[derive(Debug, Clone)]
pub struct FieldPattern(Vec<String>);

impl FieldPattern {
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        let mut segments = Vec::new();
        for part in raw
            .trim_start_matches('$')
            .split('.')
            .filter(|p| !p.is_empty())
        {
            let (name, mut brackets) = match part.find('[') {
                Some(i) => (&part[..i], &part[i..]),
                None => (part, ""),
            };
            if !name.is_empty() {
                segments.push(name.to_string());
            }
            while let Some(end) = brackets.find(']') {
                segments.push(INDEX.to_string());
                brackets = &brackets[end + 1..];
            }
        }
        if !raw.starts_with('$') && segments.len() == 1 && segments[0] != "**" {
            segments.insert(0, "**".to_string());
        }
        Self(segments)
    }

    pub(super) fn matches(&self, path: &[&str]) -> bool {
        match_segments(&self.0, path)
    }
}
