use super::*;
use std::net::TcpListener;
use std::thread::{self, JoinHandle};

use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection};

const BUDGET: Duration = Duration::from_secs(3);

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let until = Instant::now() + BUDGET;
    loop {
        match listener.accept() {
            Ok((socket, _)) => {
                socket.set_nonblocking(false).unwrap();
                return socket;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < until => {
                thread::sleep(Duration::from_millis(2))
            }
            Err(error) => panic!("bounded fixture accept: {error}"),
        }
    }
}

fn identity(expired: bool) -> (CertificateDer<'static>, PrivateKeyDer<'static>, Digest) {
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
    let mut params = rcgen::CertificateParams::new(vec!["effigy-host".into()]).unwrap();
    if expired {
        params.not_before = rcgen::date_time_ymd(2019, 1, 1);
        params.not_after = rcgen::date_time_ymd(2020, 1, 1);
    }
    let certificate = params.self_signed(&key).unwrap();
    let der = certificate.der().clone();
    // Expired certificates still have an SPKI: the pin cannot waive validity.
    let (_, parsed) = parse_x509_certificate(der.as_ref()).unwrap();
    let pin = Digest::of(parsed.public_key().raw);
    (
        der,
        PrivatePkcs8KeyDer::from(key.serialize_der()).into(),
        pin,
    )
}

fn server(expired: bool) -> (SocketAddr, Digest, JoinHandle<Vec<u8>>) {
    server_with_key(expired, false)
}

fn server_with_key(expired: bool, wrong_key: bool) -> (SocketAddr, Digest, JoinHandle<Vec<u8>>) {
    let (certificate, key, pin) = identity(expired);
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let signing_key = if wrong_key { identity(false).1 } else { key };
    #[derive(Debug)]
    struct Resolver(Arc<rustls::sign::CertifiedKey>);
    impl rustls::server::ResolvesServerCert for Resolver {
        fn resolve(
            &self,
            _hello: rustls::server::ClientHello<'_>,
        ) -> Option<Arc<rustls::sign::CertifiedKey>> {
            Some(self.0.clone())
        }
    }
    let certified_key = rustls::sign::CertifiedKey::new(
        vec![certificate],
        rustls::crypto::ring::sign::any_supported_type(&signing_key).unwrap(),
    );
    let config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(Resolver(Arc::new(certified_key))));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        // The test harness enforces an outer timeout. The peer is always one
        // connection started by this test, never a container or machine route.
        let socket = accept(&listener);
        let connection = ServerConnection::new(Arc::new(config)).unwrap();
        let mut stream = StreamOwned::new(
            connection,
            DeadlineSocket {
                socket,
                until: Instant::now() + BUDGET,
            },
        );
        let mut application_bytes = Vec::new();
        let _ = stream.read_to_end(&mut application_bytes);
        application_bytes
    });
    (address, pin, handle)
}

#[test]
fn pinned_peer_returns_verified_transport_before_first_frame() {
    let (address, pin, server) = server(false);
    let mut transport = TlsChannelTransport::connect(address, pin.clone(), BUDGET).unwrap();
    assert_eq!(
        transport.verified_peer(),
        Some(ServerIdentity { digest: pin })
    );
    transport.write_all(b"hello then credential").unwrap();
    transport.close().unwrap();
    assert_eq!(server.join().unwrap(), b"hello then credential");
}

#[test]
fn mispinned_peer_receives_zero_application_or_credential_bytes() {
    let (address, _pin, server) = server(false);
    assert!(TlsChannelTransport::connect(address, Digest::of(b"another key"), BUDGET).is_err());
    assert!(server.join().unwrap().is_empty());
}

#[test]
fn expired_pinned_peer_receives_zero_application_bytes() {
    let (address, pin, server) = server(true);
    assert!(TlsChannelTransport::connect(address, pin, BUDGET).is_err());
    assert!(server.join().unwrap().is_empty());
}

#[test]
fn absent_pin_and_zero_budget_refuse_without_connecting() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    assert!(TlsChannelTransport::connect(address, Digest([0; 32]), BUDGET).is_err());
    assert!(TlsChannelTransport::connect(address, Digest::of(b"key"), Duration::ZERO).is_err());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn changed_key_on_reconnect_cannot_reuse_previous_verification() {
    let (address, old_pin, first) = server(false);
    TlsChannelTransport::connect(address, old_pin.clone(), BUDGET)
        .unwrap()
        .close()
        .unwrap();
    assert!(first.join().unwrap().is_empty());
    let (address, new_pin, second) = server(false);
    assert_ne!(old_pin, new_pin);
    assert!(TlsChannelTransport::connect(address, old_pin, BUDGET).is_err());
    assert!(second.join().unwrap().is_empty());
}

#[test]
fn malformed_certificate_and_future_validity_refuse() {
    assert!(certificate_pin(b"not DER", UnixTime::now()).is_err());
    let (certificate, _, _) = identity(false);
    assert!(certificate_pin(
        certificate.as_ref(),
        UnixTime::since_unix_epoch(Duration::ZERO)
    )
    .is_err());
    let mut trailing = certificate.as_ref().to_vec();
    trailing.push(0);
    assert!(certificate_pin(&trailing, UnixTime::now()).is_err());
}

#[test]
fn silent_peer_cannot_hold_the_handshake_open() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut socket = accept(&listener);
        socket.set_read_timeout(Some(BUDGET)).unwrap();
        let mut bytes = Vec::new();
        let _ = socket.read_to_end(&mut bytes);
    });
    let started = Instant::now();
    assert!(
        TlsChannelTransport::connect(address, Digest::of(b"pin"), Duration::from_millis(100))
            .is_err()
    );
    assert!(started.elapsed() < BUDGET);
    server.join().unwrap();
}

#[test]
fn copied_leaf_without_its_private_key_receives_zero_application_bytes() {
    let (address, pin, server) = server_with_key(false, true);
    assert!(TlsChannelTransport::connect(address, pin, BUDGET).is_err());
    assert!(server.join().unwrap().is_empty());
}

#[test]
fn silent_peer_cannot_hold_a_verified_read_open() {
    let (address, pin, server) = server(false);
    let mut transport = TlsChannelTransport::connect(address, pin, BUDGET).unwrap();
    transport.operation_timeout = Duration::from_millis(100);
    let started = Instant::now();
    assert!(transport.read(&mut [0; 1]).is_err());
    assert!(started.elapsed() < BUDGET);
    drop(transport);
    assert!(server.join().unwrap().is_empty());
}
