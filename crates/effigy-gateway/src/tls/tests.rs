use super::*;

#[cfg(unix)]
#[test]
fn trust_bundle_matches_only_the_current_fake_mkcert_ca() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = tempfile::tempdir().unwrap();
    let current_ca = rcgen::generate_simple_self_signed(vec!["current.test".into()])
        .unwrap()
        .cert
        .pem();
    let stale_ca = rcgen::generate_simple_self_signed(vec!["stale.test".into()])
        .unwrap()
        .cert
        .pem();
    let root_ca_path = fixture.path().join("rootCA.pem");
    std::fs::write(&root_ca_path, &current_ca).unwrap();

    let mkcert = fixture.path().join("mkcert-fixture");
    std::fs::write(
        &mkcert,
        "#!/bin/sh\ncase \"$1\" in\n  -CAROOT) dirname \"$0\" ;;\n  *) exit 64 ;;\nesac\n",
    )
    .unwrap();
    std::fs::set_permissions(&mkcert, std::fs::Permissions::from_mode(0o700)).unwrap();
    let _env =
        crate::env_test_support::EnvGuard::set_many(&[(MKCERT_BIN_ENV, Some(mkcert.as_os_str()))]);

    let resolved_root = mkcert_root_ca_pem().expect("fake mkcert returns its CA root");
    assert_eq!(resolved_root, root_ca_path);
    let fingerprint = certificate_fingerprint_from_file(&resolved_root).unwrap();

    let current_bundle = fixture.path().join("current-trust-bundle.pem");
    std::fs::write(&current_bundle, format!("{stale_ca}\n{current_ca}")).unwrap();
    assert!(ca_bundle_paths_contain_fingerprint(
        [current_bundle.as_path()],
        &fingerprint
    ));

    let stale_bundle = fixture.path().join("stale-trust-bundle.pem");
    std::fs::write(&stale_bundle, &stale_ca).unwrap();
    assert!(!ca_bundle_paths_contain_fingerprint(
        [stale_bundle.as_path()],
        &fingerprint
    ));
}

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

/// Generate a self-signed certificate pair for a fixture route.
fn write_self_signed_pair(certs: &Path, domain: &str) {
    let cert = certs.join(format!("{domain}.pem"));
    let key = certs.join(format!("{domain}-key.pem"));
    let output = std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-sha256",
            "-days",
            "30",
            "-subj",
            &format!("/CN={domain}"),
            "-addext",
            &format!("subjectAltName=DNS:{domain}"),
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-keyout",
            key.to_str().expect("UTF-8 key path"),
            "-out",
            cert.to_str().expect("UTF-8 cert path"),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .expect("run openssl for a fixture certificate");
    assert!(
        output.status.success(),
        "openssl failed for {domain}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The post-registration self-test (papercut 9604503b): a running gateway that
/// serves the registered certificate completes the probe, an SNI it never
/// loaded refuses the handshake, and a handshake that presents another route's
/// certificate refuses too.
#[test]
fn route_handshake_probe_proves_the_registered_certificate_is_served() {
    let dir = tempfile::tempdir().unwrap();
    let certs = dir.path().join("certs");
    std::fs::create_dir_all(&certs).unwrap();
    for domain in ["probe.test", "other.test"] {
        write_self_signed_pair(&certs, domain);
    }
    let resolver = build_sni_resolver_from_dir(&certs).unwrap();
    assert!(resolver.has_cert("probe.test"), "fixture pair loads");
    let resolver = Arc::new(resolver);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let config = Arc::new(server_config_from_resolver(resolver));
        // Three connections: the served route, an unregistered SNI and a
        // handshake that presents the wrong registered certificate.
        for _ in 0..3 {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let session = rustls::ServerConnection::new(Arc::clone(&config)).unwrap();
            let mut tls = rustls::StreamOwned::new(session, stream);
            // An unregistered SNI ends the handshake with an alert here.
            let mut byte = [0u8; 1];
            let _ = std::io::Read::read(&mut tls, &mut byte);
        }
    });

    let registered = certs.join("probe.test.pem");
    probe_route_handshake(address, "probe.test", &registered, Duration::from_secs(10))
        .expect("the running gateway serves the registered route");

    let unknown = probe_route_handshake(
        address,
        "missing.test",
        &registered,
        Duration::from_secs(10),
    )
    .expect_err("an SNI the gateway never loaded refuses the handshake");
    assert!(
        unknown.to_string().contains("missing.test") || unknown.to_string().contains("handshake"),
        "{unknown}"
    );

    let wrong = probe_route_handshake(address, "other.test", &registered, Duration::from_secs(10))
        .expect_err("another route's certificate is not this route's certificate");
    assert!(
        wrong
            .to_string()
            .contains("has not loaded this route's certificate"),
        "{wrong}"
    );
    server.join().unwrap();
}
