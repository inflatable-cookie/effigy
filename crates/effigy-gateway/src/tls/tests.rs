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
fn remove_cert_ignores_missing_files() {
    let dir = tempfile::tempdir().unwrap();
    let config = TlsConfig::new(dir.path().to_path_buf());
    config.remove_cert("missing.test").unwrap();
}

#[test]
fn resolved_mkcert_program_uses_absolute_override() {
    let dir = tempfile::tempdir().unwrap();
    let mkcert = dir.path().join("mkcert");
    std::fs::write(&mkcert, "#!/bin/sh\nexit 0\n").unwrap();

    let previous = std::env::var_os(MKCERT_BIN_ENV);
    unsafe {
        std::env::set_var(MKCERT_BIN_ENV, &mkcert);
    }

    let resolved = resolved_mkcert_program();

    match previous {
        Some(value) => unsafe { std::env::set_var(MKCERT_BIN_ENV, value) },
        None => unsafe { std::env::remove_var(MKCERT_BIN_ENV) },
    }

    assert_eq!(resolved.as_deref(), Some(mkcert.as_path()));
}

#[test]
fn resolved_mkcert_program_ignores_relative_override() {
    let previous = std::env::var_os(MKCERT_BIN_ENV);
    unsafe {
        std::env::set_var(MKCERT_BIN_ENV, "mkcert");
    }

    let resolved = resolved_mkcert_program();

    match previous {
        Some(value) => unsafe { std::env::set_var(MKCERT_BIN_ENV, value) },
        None => unsafe { std::env::remove_var(MKCERT_BIN_ENV) },
    }

    assert_ne!(resolved.as_deref(), Some(Path::new("mkcert")));
}
