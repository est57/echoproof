/// `**` matches zero or more segments, `*` and `{name}` exactly one.
pub fn match_segments<P: AsRef<str>, S: AsRef<str>>(pattern: &[P], path: &[S]) -> bool {
    let Some((head, rest)) = pattern.split_first() else {
        return path.is_empty();
    };
    let head = head.as_ref();
    if head == "**" {
        return (0..=path.len()).any(|skip| match_segments(rest, &path[skip..]));
    }
    match path.split_first() {
        Some((segment, path_rest)) => {
            segment_matches(head, segment.as_ref()) && match_segments(rest, path_rest)
        }
        None => false,
    }
}

fn segment_matches(pattern: &str, segment: &str) -> bool {
    pattern == "*" || (pattern.starts_with('{') && pattern.ends_with('}')) || pattern == segment
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globbing() {
        assert!(match_segments(&["**", "id"], &["a", "b", "id"]));
        assert!(match_segments(&["**", "id"], &["id"]));
        assert!(match_segments(&["a", "*", "c"], &["a", "x", "c"]));
        assert!(match_segments(&["a", "{id}"], &["a", "42"]));
        assert!(!match_segments(&["a", "*"], &["a"]));
        assert!(!match_segments(&["a"], &["a", "b"]));
    }
}
