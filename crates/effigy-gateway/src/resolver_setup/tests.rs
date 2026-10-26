use super::*;

/// The resolver file for a suffix lives at its `/etc/resolver` path and names
/// the gateway's loopback DNS listener on the managed port.
#[test]
fn resolver_file_specs_name_the_suffix_path_and_listener_port() {
    let cases = [("test", 15353), ("dev", 15353), ("dev", 5353)];
    for (suffix, port) in cases {
        let spec = resolver_file_spec(suffix, port);
        assert_eq!(spec.suffix, suffix);
        assert_eq!(spec.path, resolver_path(suffix));
        assert_eq!(
            spec.path,
            PathBuf::from(format!("/etc/resolver/{suffix}")),
            "suffix {suffix}"
        );
        assert_eq!(spec.content, resolver_file_contents(port));
        assert!(
            spec.content.contains("nameserver 127.0.0.1"),
            "suffix {suffix} names the loopback listener: {}",
            spec.content
        );
        assert!(
            spec.content.contains(&format!("port {port}")),
            "suffix {suffix} names port {port}: {}",
            spec.content
        );
        assert!(spec.content.contains("Effigy"), "suffix {suffix}");
    }
}

/// Suffixes the resolver must serve beyond the managed TLD: every route's
/// parent domain, deduplicated, trailing dots stripped, the managed TLD and
/// its subdomains skipped, and unportable names refused rather than written
/// into a file path.
#[test]
fn route_driven_suffixes_keep_portable_parent_domains() {
    let cases: [(&[&str], &str, &[&str]); 4] = [
        (
            &[
                "myapp.test",
                "db.myapp.test",
                "test",
                "dev.cumberland.co.uk",
                "admin.cumberland.co.uk",
                "DEV.CUMBERLAND.CO.UK",
            ],
            "test",
            &["admin.cumberland.co.uk", "dev.cumberland.co.uk"],
        ),
        (
            &["api.localhost", "localhost", "external.example.com"],
            "localhost",
            &["external.example.com"],
        ),
        (
            &["dev.cumberland.co.uk.", "dev.cumberland.co.uk"],
            "test",
            &["dev.cumberland.co.uk"],
        ),
        (
            &[
                "valid.example.com",
                "../escape",
                "bad/label.example.com",
                "-bad.example.com",
            ],
            "test",
            &["valid.example.com"],
        ),
    ];
    for (domains, managed_tld, expected) in cases {
        let domains: Vec<String> = domains.iter().map(|domain| domain.to_string()).collect();
        assert_eq!(
            route_driven_resolver_suffixes(domains.clone(), managed_tld),
            expected
                .iter()
                .map(|suffix| suffix.to_string())
                .collect::<Vec<_>>(),
            "domains {domains:?} under {managed_tld}"
        );
    }
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
