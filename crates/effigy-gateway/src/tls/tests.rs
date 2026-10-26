use super::*;

#[test]
fn sni_resolver_empty_returns_none() {
    let resolver = SniCertResolver::new();
    assert_eq!(resolver.cert_count(), 0);
    assert!(!resolver.has_cert("example.test"));
}

#[test]
fn sni_resolver_from_empty_dir() {
    let dir = tempfile::tempdir().unwrap();
    let resolver = build_sni_resolver_from_dir(dir.path()).unwrap();
    assert_eq!(resolver.cert_count(), 0);
}

#[test]
fn sni_resolver_from_nonexistent_dir() {
    let resolver = build_sni_resolver_from_dir(Path::new("/nonexistent/certs")).unwrap();
    assert_eq!(resolver.cert_count(), 0);
}

#[test]
fn tls_config_mkcert_check_doesnt_panic() {
    let _env = crate::env_test_support::EnvGuard::set_many(&[]);
    // Just verify these don't panic — mkcert may not be installed.
    let _ = TlsConfig::mkcert_available();
    let _ = TlsConfig::ca_installed();
}

#[test]
fn cert_paths_for_domain() {
    let config = TlsConfig::new(PathBuf::from("/tmp/certs"));
    let result = config.load_cert("nonexistent.test");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        GatewayError::TlsCertNotFound { .. }
    ));
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
