use super::parse_target;
use super::send::target_url;

#[test]
fn recorded_paths_stay_on_the_target() {
    let target = parse_target("http://127.0.0.1:3000").unwrap();
    assert_eq!(
        target_url(&target, "/orders?x=1").unwrap().as_str(),
        "http://127.0.0.1:3000/orders?x=1"
    );
    assert_eq!(
        target_url(&target, "//evil.com/x").unwrap().host_str(),
        Some("127.0.0.1")
    );
    assert!(target_url(&target, "@evil.com/x").is_err());
    assert!(target_url(&target, "evil.com").is_err());
}

#[test]
fn targets_must_be_http_urls() {
    assert!(parse_target("https://staging.example.com/api").is_ok());
    assert!(parse_target("file:///etc/passwd").is_err());
    assert!(parse_target("127.0.0.1:3000").is_err());
}
