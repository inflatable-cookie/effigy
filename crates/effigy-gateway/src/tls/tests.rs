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
