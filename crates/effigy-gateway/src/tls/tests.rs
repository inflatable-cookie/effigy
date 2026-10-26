use super::*;

/// An empty certificate cache yields an empty resolver, whether the directory
/// is empty or not created yet.
#[test]
fn sni_resolver_from_an_empty_or_absent_dir_caches_no_certificate() {
    let dir = tempfile::tempdir().unwrap();
    let absent = dir.path().join("certs");
    let cases = [dir.path(), absent.as_path()];
    for certs_dir in cases {
        let resolver = build_sni_resolver_from_dir(certs_dir).unwrap();
        assert_eq!(resolver.cert_count(), 0, "dir {}", certs_dir.display());
        assert!(!resolver.has_cert("example.test"));
    }
}

/// A domain's pair is read from its two cache files: a missing key is a
/// not-found naming that key, and an existing pair resolves both paths.
#[test]
fn cert_paths_for_domain_resolve_an_existing_pair() {
    let dir = tempfile::tempdir().unwrap();
    let config = TlsConfig::new(dir.path().to_path_buf());
    let paths = config.cert_paths("app.test");
    assert_eq!(paths.cert, dir.path().join("app.test.pem"));
    assert_eq!(paths.key, dir.path().join("app.test-key.pem"));

    std::fs::write(&paths.cert, "cert").unwrap();
    let missing_key = config.load_cert("app.test").expect_err("key absent");
    assert!(
        matches!(missing_key, GatewayError::TlsCertNotFound { ref path } if *path == paths.key),
        "a missing key names the key path: {missing_key:?}"
    );

    std::fs::write(&paths.key, "key").unwrap();
    let found = config.load_cert("app.test").expect("pair exists");
    assert_eq!(found.cert, paths.cert);
    assert_eq!(found.key, paths.key);

    let no_pair = config.load_cert("nonexistent.test").expect_err("no pair");
    assert!(matches!(no_pair, GatewayError::TlsCertNotFound { .. }));
}

#[test]
fn orphan_certificates_prune_by_age_and_a_routed_domain_keeps_its_pair() {
    let dir = tempfile::tempdir().unwrap();
    let now = std::time::SystemTime::now();
    let old = now - Duration::from_secs(8 * 24 * 60 * 60);
    for domain in ["orphan.test", "routed.test", "fresh.test"] {
        for name in [format!("{domain}.pem"), format!("{domain}-key.pem")] {
            let path = dir.path().join(name);
            std::fs::write(&path, "pem").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(if domain == "fresh.test" { now } else { old })
                .unwrap();
        }
    }

    let pruned = prune_orphan_certs(
        dir.path(),
        &BTreeSet::from(["routed.test".to_owned()]),
        CERT_CACHE_MAX_AGE,
        now,
    )
    .unwrap();

    assert_eq!(pruned, ["orphan.test"]);
    assert!(!dir.path().join("orphan.test.pem").exists());
    assert!(!dir.path().join("orphan.test-key.pem").exists());
    assert!(dir.path().join("routed.test.pem").exists());
    assert!(dir.path().join("routed.test-key.pem").exists());
    assert!(dir.path().join("fresh.test.pem").exists());
    assert!(dir.path().join("fresh.test-key.pem").exists());
}

#[test]
fn orphan_certificate_prune_ignores_a_missing_directory() {
    let dir = tempfile::tempdir().unwrap();
    let pruned = prune_orphan_certs(
        &dir.path().join("missing"),
        &BTreeSet::new(),
        CERT_CACHE_MAX_AGE,
        std::time::SystemTime::now(),
    )
    .unwrap();
    assert!(pruned.is_empty());
}

#[test]
fn resolved_mkcert_program_uses_absolute_override() {
    let dir = tempfile::tempdir().unwrap();
    let mkcert = dir.path().join("mkcert");
    std::fs::write(&mkcert, "#!/bin/sh\nexit 0\n").unwrap();

    let _env =
        crate::env_test_support::EnvGuard::set_many(&[(MKCERT_BIN_ENV, Some(mkcert.as_os_str()))]);

    let resolved = resolved_mkcert_program();

    assert_eq!(resolved.as_deref(), Some(mkcert.as_path()));
}

#[test]
fn resolved_mkcert_program_ignores_relative_override() {
    let _env = crate::env_test_support::EnvGuard::set_many(&[(
        MKCERT_BIN_ENV,
        Some(std::ffi::OsStr::new("mkcert")),
    )]);

    let resolved = resolved_mkcert_program();

    assert_ne!(resolved.as_deref(), Some(Path::new("mkcert")));
}

#[test]
fn private_mkcert_program_uses_absolute_override_and_fails_closed_when_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let mkcert = dir.path().join("mkcert-fixture");
    std::fs::write(&mkcert, "fixture executable").unwrap();
    let _env =
        crate::env_test_support::EnvGuard::set_many(&[(MKCERT_BIN_ENV, Some(mkcert.as_os_str()))]);
    let resolved = private_mkcert_program();

    unsafe {
        std::env::set_var(MKCERT_BIN_ENV, dir.path().join("missing-mkcert"));
    }
    let invalid = private_mkcert_program();

    assert_eq!(resolved.as_deref(), Some(mkcert.as_path()));
    assert!(
        invalid.is_none(),
        "an invalid explicit path must not fall back"
    );
}
