use super::agent;
use crate::report::{Report, Verdict};

fn report(verdict: Verdict, skipped: usize) -> Report {
    Report {
        verdict,
        total: 48,
        changed: 0,
        unchanged: 48 - skipped,
        missing: 0,
        skipped,
        noise: Vec::new(),
        expect: Vec::new(),
        endpoints: Vec::new(),
        findings: Vec::new(),
        judge: None,
    }
}

#[test]
fn full_pass_reads_as_verified() {
    let text = agent(&report(Verdict::Pass, 0), Some("r1"), true);
    assert!(text.starts_with("Echoproof verdict: PASS\n"));
    assert!(text.trim_end().ends_with("No unexpected behavior changes."));
}

#[test]
fn pass_with_skipped_writes_is_stated_as_partial() {
    let text = agent(&report(Verdict::Pass, 8), Some("r1"), false);
    let first = text.lines().next().unwrap();
    assert_eq!(
        first,
        "Echoproof verdict: PASS for the 40 replayed requests only; 8 write requests were NOT verified"
    );
    assert!(text.contains("Do not report the change as fully verified"));
    assert!(text.contains("Allow them per endpoint"));
    assert!(!text.contains("No unexpected behavior changes."));
}

#[test]
fn resettable_projects_are_told_to_allow_every_write() {
    let text = agent(&report(Verdict::Pass, 8), Some("r1"), true);
    assert!(text.contains(r#"pass `allow_writes: ["*"]` to replay them all"#));
    assert!(!text.contains("Allow them per endpoint"));
}

#[test]
fn regressions_keep_the_plain_verdict() {
    let text = agent(&report(Verdict::Regression, 8), Some("r1"), true);
    assert!(text.starts_with("Echoproof verdict: REGRESSION\n"));
    assert!(text.contains("echoproof_exchange"));
}
