use std::path::Path;
use std::process::Command;

const MAX_CHARS: usize = 100_000;

/// The project's uncommitted changes against HEAD, so the judge sees the code even when an
/// agent cannot write a diff file. External diff drivers, textconv filters and fsmonitor hooks
/// are disabled because they would run commands from the repository's configuration.
pub fn working_tree(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "core.fsmonitor=false"])
        .args([
            "diff",
            "HEAD",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if text.trim().is_empty() {
        return None;
    }
    Some(clip(&text))
}

fn clip(text: &str) -> String {
    let count = text.chars().count();
    if count <= MAX_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_CHARS).collect();
    format!(
        "{kept}\n[diff truncated: {} more characters]",
        count - MAX_CHARS
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn returns_uncommitted_changes_and_nothing_outside_git() {
        let dir = std::env::temp_dir().join(format!("echoproof-gitdiff-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(working_tree(&dir), None);

        git(&dir, &["init", "-q"]);
        fs::write(dir.join("a.php"), "$discount = $v['discount_amount'];\n").unwrap();
        git(&dir, &["add", "a.php"]);
        git(&dir, &["commit", "-q", "-m", "init"]);
        assert_eq!(working_tree(&dir), None);

        fs::write(dir.join("a.php"), "$discount = $v['discount'];\n").unwrap();
        let diff = working_tree(&dir).unwrap();
        assert!(diff.contains("-$discount = $v['discount_amount'];"));
        assert!(diff.contains("+$discount = $v['discount'];"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn long_diffs_are_clipped() {
        let clipped = clip(&"x".repeat(MAX_CHARS + 5));
        assert!(clipped.ends_with("[diff truncated: 5 more characters]"));
    }
}
