use super::*;

#[test]
fn resolver_path_is_correct() {
    assert_eq!(resolver_path("test"), PathBuf::from("/etc/resolver/test"));
    assert_eq!(resolver_path("dev"), PathBuf::from("/etc/resolver/dev"));
}

#[test]
fn resolver_file_contents_format() {
    let content = resolver_file_contents(15353);
    assert!(content.contains("nameserver 127.0.0.1"));
    assert!(content.contains("port 15353"));
    assert!(content.contains("Effigy"));
}

#[test]
fn resolver_spec_has_correct_path_and_content() {
    let spec = resolver_file_spec("test", 15353);
    assert_eq!(spec.path, PathBuf::from("/etc/resolver/test"));
    assert!(spec.content.contains("port 15353"));
}

#[test]
fn resolver_spec_with_custom_port() {
    let spec = resolver_file_spec("dev", 5353);
    assert!(spec.content.contains("port 5353"));
}

#[test]
fn route_driven_suffixes_skip_managed_tld() {
    let domains = vec![
        "myapp.test".to_string(),
        "db.myapp.test".to_string(),
        "test".to_string(),
        "dev.cumberland.co.uk".to_string(),
        "admin.cumberland.co.uk".to_string(),
        "DEV.CUMBERLAND.CO.UK".to_string(), // case-insensitive dedup
    ];

    let suffixes = route_driven_resolver_suffixes(domains, "test");
    assert_eq!(
        suffixes,
        vec![
            "admin.cumberland.co.uk".to_string(),
            "dev.cumberland.co.uk".to_string(),
        ]
    );
}

#[test]
fn route_driven_suffixes_handle_alternate_managed_tld() {
    // A repo running with a custom managed TLD (e.g. `localhost` or
    // `dev`) should still get the same skip-the-bootstrap behaviour.
    let domains = vec![
        "api.localhost".to_string(),
        "localhost".to_string(),
        "external.example.com".to_string(),
    ];

    let suffixes = route_driven_resolver_suffixes(domains, "localhost");
    assert_eq!(suffixes, vec!["external.example.com".to_string()]);
}

#[test]
fn route_driven_suffixes_strip_trailing_dot_and_dedupe() {
    let domains = vec![
        "dev.cumberland.co.uk.".to_string(),
        "dev.cumberland.co.uk".to_string(),
    ];

    let suffixes = route_driven_resolver_suffixes(domains, "test");
    assert_eq!(suffixes, vec!["dev.cumberland.co.uk".to_string()]);
}

#[test]
fn file_is_effigy_managed_detects_header() {
    let dir = tempfile::tempdir().unwrap();
    let managed = dir.path().join("managed");
    let unmanaged = dir.path().join("unmanaged");
    let absent = dir.path().join("absent");
    std::fs::write(&managed, resolver_file_contents(15353)).unwrap();
    std::fs::write(&unmanaged, "nameserver 8.8.8.8\nport 53\n").unwrap();

    assert!(file_is_effigy_managed(&managed));
    assert!(!file_is_effigy_managed(&unmanaged));
    assert!(!file_is_effigy_managed(&absent));
}
