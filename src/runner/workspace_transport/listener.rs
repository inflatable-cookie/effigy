//! A session-custodied TLS listener. Handles outside custody are weak and cannot
//! keep the private signing key or bound socket alive after session settlement.
use super::*;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
    mpsc, Mutex, Weak,
};
use std::thread::{self, JoinHandle};

use effigy_kernel::run::Run;
use effigy_kernel::workspace_session::{
    ChannelListener, ListenerPhase, ServingKey, ServingRefusal, SessionLaunch,
};
use rustls::pki_types::PrivatePkcs8KeyDer;
use rustls::sign::{CertifiedKey, Signer, SigningKey, SingleCertAndKey};
use rustls::{ServerConfig, ServerConnection, SignatureAlgorithm};

type KeySlot = Mutex<Option<Arc<dyn SigningKey>>>;

/// Configurations and in-flight TLS sessions hold only this revocable reference,
/// never another private key owner. Even a retained transport cannot sign once
/// the owning session stops its listener.
#[derive(Debug)]
struct RevocableKey(Weak<KeySlot>);
impl SigningKey for RevocableKey {
    fn choose_scheme(&self, offered: &[SignatureScheme]) -> Option<Box<dyn Signer>> {
        if !offered.contains(&SignatureScheme::ECDSA_NISTP256_SHA256) {
            return None;
        }
        let slot = self.0.upgrade()?;
        if slot.lock().ok()?.is_none() {
            return None;
        }
        Some(Box::new(Self(self.0.clone())))
    }
    fn algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::ECDSA
    }
}
impl Signer for RevocableKey {
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, Error> {
        let slot = self
            .0
            .upgrade()
            .ok_or_else(|| Error::General("session key revoked".into()))?;
        let guard = slot
            .lock()
            .map_err(|_| Error::General("session key unavailable".into()))?;
        let signer = guard
            .as_ref()
            .and_then(|key| key.choose_scheme(&[self.scheme()]))
            .ok_or_else(|| Error::General("session key revoked".into()))?;
        signer.sign(message)
    }
    fn scheme(&self) -> SignatureScheme {
        SignatureScheme::ECDSA_NISTP256_SHA256
    }
}

struct ListenerState {
    endpoint: SocketAddr,
    pin: Digest,
    stopped: AtomicBool,
    serving: AtomicBool,
    phase: AtomicU8,
    sockets: Mutex<BTreeMap<u64, TcpStream>>,
    next: AtomicU64,
    key: Arc<KeySlot>,
}
impl ListenerState {
    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.key.lock().unwrap_or_else(|e| e.into_inner()).take();
        let mut sockets = self.sockets.lock().unwrap_or_else(|e| e.into_inner());
        for socket in sockets.values() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        sockets.clear();
    }
}

/// Only SessionLaunch owns this value. Its stop joins the bounded TLS accept
/// worker, revokes the signer and shuts down every accepted socket.
pub(crate) struct SessionTlsListener {
    endpoint: SocketAddr,
    state: Arc<ListenerState>,
    worker: Option<JoinHandle<()>>,
}
impl std::fmt::Debug for SessionTlsListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionTlsListener")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

/// Does not own the listener or key. A receive requires the exact live session
/// run and its SessionLaunch's serving permission, including after handoff.
pub(crate) struct HostChannelAcceptor {
    state: Weak<ListenerState>,
    incoming: mpsc::Receiver<ServerTlsTransport>,
}
impl std::fmt::Debug for HostChannelAcceptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HostChannelAcceptor")
    }
}
impl HostChannelAcceptor {
    pub(crate) fn accept(
        &mut self,
        launch: &SessionLaunch,
        run: &Run<'_>,
    ) -> io::Result<Option<ServerTlsTransport>> {
        let state = self
            .state
            .upgrade()
            .ok_or_else(|| invalid("session listener stopped"))?;
        if state.stopped.load(Ordering::SeqCst) || !launch.serving(run) {
            return Err(invalid("session listener is not serving"));
        }
        // Require the run's actual listener, not another live SessionLaunch.
        if launch.endpoint() != Some(state.endpoint)
            || launch.lease().as_ref().map(|lease| lease.pin()) != Some(&state.pin)
        {
            return Err(invalid("wrong session listener"));
        }
        state.serving.store(true, Ordering::SeqCst);
        match self.incoming.try_recv() {
            Ok(transport) => Ok(Some(transport)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(invalid("session listener stopped")),
        }
    }
}

/// Server-side byte adapter for the existing kernel dispatcher. The dispatcher
/// still owns Hello, credentials, grants, frame limits and all authority.
pub(crate) struct ServerTlsTransport {
    stream: StreamOwned<ServerConnection, DeadlineSocket>,
    state: Weak<ListenerState>,
    id: u64,
    pin: Digest,
    timeout: Duration,
}
impl std::fmt::Debug for ServerTlsTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServerTlsTransport(<redacted>)")
    }
}
impl ServerTlsTransport {
    fn begin(&mut self) -> io::Result<()> {
        if self
            .state
            .upgrade()
            .is_none_or(|state| state.stopped.load(Ordering::SeqCst))
        {
            return Err(invalid("session listener stopped"));
        }
        self.stream.sock.until = Instant::now()
            .checked_add(self.timeout)
            .ok_or_else(|| invalid("invalid TLS deadline"))?;
        Ok(())
    }
}
impl Transport for ServerTlsTransport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.begin()?;
        self.stream.read(bytes)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.begin()?;
        self.stream.write_all(bytes)?;
        self.stream.flush()
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        // The dispatcher's pin is the identity of this TLS channel's server;
        // client authority is established separately by the kernel protocol.
        self.state
            .upgrade()
            .filter(|s| !s.stopped.load(Ordering::SeqCst))
            .map(|_| ServerIdentity {
                digest: self.pin.clone(),
            })
    }
}
impl Drop for ServerTlsTransport {
    fn drop(&mut self) {
        let _ = self.stream.sock.socket.shutdown(Shutdown::Both);
        if let Some(state) = self.state.upgrade() {
            state
                .sockets
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&self.id);
        }
    }
}

impl SessionTlsListener {
    /// Generate one P-256 key and leaf for one session. No disk key, trust-store
    /// mutation, address fallback or listener rebind exists here.
    pub(crate) fn bind(
        address: SocketAddr,
        timeout: Duration,
    ) -> io::Result<(Self, ServingKey, HostChannelAcceptor)> {
        if timeout.is_zero() || timeout > Duration::from_secs(5) {
            return Err(invalid("TLS accept budget must be within five seconds"));
        }
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|_| invalid("TLS key generation failed"))?;
        let params = rcgen::CertificateParams::new(vec!["effigy-host".into()])
            .map_err(|_| invalid("TLS leaf generation failed"))?;
        let certificate = params
            .self_signed(&key)
            .map_err(|_| invalid("TLS leaf generation failed"))?;
        let der = certificate.der().clone();
        let pin = certificate_pin(der.as_ref(), UnixTime::now())
            .map_err(|_| invalid("TLS leaf invalid"))?;
        let secret = key.serialize_der();
        let private_key = PrivatePkcs8KeyDer::from(secret.clone()).into();
        let signer = rustls::crypto::ring::sign::any_supported_type(&private_key)
            .map_err(|_| invalid("TLS key invalid"))?;
        let key_slot = Arc::new(Mutex::new(Some(signer)));
        let certified =
            CertifiedKey::new(vec![der], Arc::new(RevocableKey(Arc::downgrade(&key_slot))));
        let mut config =
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_protocol_versions(&[&rustls::version::TLS13])
                .map_err(|_| invalid("TLS configuration failed"))?
                .with_no_client_auth()
                .with_cert_resolver(Arc::new(SingleCertAndKey::from(certified)));
        // No resumable ticket or prior TLS state survives reconnect.
        config.send_tls13_tickets = 0;
        let listener = TcpListener::bind(address)?;
        let endpoint = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let state = Arc::new(ListenerState {
            endpoint,
            pin: pin.clone(),
            stopped: AtomicBool::new(false),
            serving: AtomicBool::new(false),
            phase: AtomicU8::new(0),
            sockets: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            key: key_slot,
        });
        let (sender, incoming) = mpsc::sync_channel(1);
        let worker_state = state.clone();
        let worker_pin = pin.clone();
        let worker = thread::Builder::new()
            .name("workspace-tls".into())
            .spawn(move || {
                struct StopOnExit(Arc<ListenerState>);
                impl Drop for StopOnExit {
                    fn drop(&mut self) {
                        self.0.stop();
                    }
                }
                let _stop = StopOnExit(worker_state.clone());
                let config = Arc::new(config);
                while !worker_state.stopped.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((socket, _)) => {
                            if socket.set_nonblocking(false).is_err() {
                                continue;
                            }
                            let serving_at_accept = worker_state.serving.load(Ordering::SeqCst);
                            let id = worker_state.next.fetch_add(1, Ordering::Relaxed);
                            {
                                let mut sockets = worker_state
                                    .sockets
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner());
                                if sockets.len() >= 32
                                    || worker_state.stopped.load(Ordering::SeqCst)
                                {
                                    continue;
                                }
                                let Ok(copy) = socket.try_clone() else {
                                    continue;
                                };
                                sockets.insert(id, copy);
                            }
                            let Ok(connection) = ServerConnection::new(config.clone()) else {
                                worker_state
                                    .sockets
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .remove(&id);
                                continue;
                            };
                            let mut transport = ServerTlsTransport {
                                stream: StreamOwned::new(
                                    connection,
                                    DeadlineSocket {
                                        socket,
                                        until: Instant::now() + timeout,
                                    },
                                ),
                                state: Arc::downgrade(&worker_state),
                                id,
                                pin: worker_pin.clone(),
                                timeout,
                            };
                            let mut handshake_ok = true;
                            while transport.stream.conn.is_handshaking() {
                                if transport
                                    .stream
                                    .conn
                                    .complete_io(&mut transport.stream.sock)
                                    .is_err()
                                {
                                    handshake_ok = false;
                                    break;
                                }
                            }
                            if handshake_ok
                                && serving_at_accept
                                && !worker_state.stopped.load(Ordering::SeqCst)
                            {
                                let _ = sender.try_send(transport);
                            } else if handshake_ok {
                                transport.stream.conn.send_close_notify();
                                let _ = transport.stream.flush();
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(_) => break,
                    }
                }
                worker_state.stop();
            })?;
        let key = ServingKey::new(secret, pin).map_err(|_| invalid("TLS key custody refused"))?;
        let acceptor = HostChannelAcceptor {
            state: Arc::downgrade(&state),
            incoming,
        };
        Ok((
            Self {
                endpoint,
                state,
                worker: Some(worker),
            },
            key,
            acceptor,
        ))
    }
}
impl ChannelListener for SessionTlsListener {
    fn endpoint(&self) -> Option<SocketAddr> {
        if self.state.stopped.load(Ordering::SeqCst) {
            None
        } else {
            Some(self.endpoint)
        }
    }
    fn phase(&self) -> ListenerPhase {
        if self.state.stopped.load(Ordering::SeqCst) {
            ListenerPhase::Stopped
        } else if self.state.phase.load(Ordering::SeqCst) == 0 {
            ListenerPhase::Qualifying
        } else {
            ListenerPhase::Holding
        }
    }
    fn qualify(&mut self) -> Result<(), ServingRefusal> {
        if self.state.stopped.load(Ordering::SeqCst) {
            return Err(ServingRefusal::Stopped);
        }
        self.state.phase.store(1, Ordering::SeqCst);
        Ok(())
    }
    fn stop(&mut self) {
        self.state.stop();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for SessionTlsListener {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn close_deadline_does_not_wait_for_an_unresponsive_tls_peer() {
        let (mut listener, key, acceptor) =
            SessionTlsListener::bind("127.0.0.1:0".parse().unwrap(), Duration::from_secs(1))
                .unwrap();
        // Wire-only fixture; the session-authority checks have separate tests.
        listener.state.serving.store(true, Ordering::SeqCst);
        let mut client = TlsChannelTransport::connect(
            listener.endpoint().unwrap(),
            key.pin(),
            Duration::from_secs(2),
        )
        .unwrap();
        let _silent_server = acceptor
            .incoming
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        client.operation_timeout = Duration::from_millis(100);
        let started = Instant::now();
        client.close().unwrap();
        assert!(started.elapsed() < Duration::from_secs(1));
        listener.stop();
    }
    #[test]
    fn retained_signer_has_no_private_key_after_revocation() {
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256).unwrap();
        let der = PrivatePkcs8KeyDer::from(key.serialize_der()).into();
        let slot = Arc::new(Mutex::new(Some(
            rustls::crypto::ring::sign::any_supported_type(&der).unwrap(),
        )));
        let revocable = RevocableKey(Arc::downgrade(&slot));
        let signer = revocable
            .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
            .unwrap();
        assert!(signer.sign(b"live session").is_ok());
        slot.lock().unwrap().take();
        assert!(signer.sign(b"dead session").is_err());
        assert!(revocable
            .choose_scheme(&[SignatureScheme::ECDSA_NISTP256_SHA256])
            .is_none());
    }
}
