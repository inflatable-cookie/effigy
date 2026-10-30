//! TLS boundary below the workspace-channel framing and authority layers.
//!
//! No application bytes are writable until both the SPKI pin and the TLS
//! CertificateVerify signature have passed. Each connection owns fresh TLS
//! state. This module does not publish locators or admit workspace requests.
#![allow(dead_code)] // Consumed by the channel qualification and #452 glue.

use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use effigy_kernel::id::Digest;
use effigy_kernel::workspace_channel::{check_server_pin, ServerIdentity, Transport};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, ClientConnection, DigitallySignedStruct, Error,
    SignatureScheme, StreamOwned,
};
use x509_parser::parse_x509_certificate;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Only SPKI, never a certificate fingerprint or an unverified peer claim.
fn certificate_pin(der: &[u8], now: UnixTime) -> Result<Digest, Error> {
    let (remaining, certificate) = parse_x509_certificate(der)
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
    if !remaining.is_empty() {
        return Err(Error::InvalidCertificate(CertificateError::BadEncoding));
    }
    let seconds = i64::try_from(now.as_secs())
        .map_err(|_| Error::InvalidCertificate(CertificateError::BadEncoding))?;
    if seconds < certificate.validity().not_before.timestamp() {
        return Err(Error::InvalidCertificate(CertificateError::NotValidYet));
    }
    if seconds > certificate.validity().not_after.timestamp() {
        return Err(Error::InvalidCertificate(CertificateError::Expired));
    }
    Ok(Digest::of(certificate.public_key().raw))
}

#[derive(Debug)]
struct PinVerifier {
    pin: Digest,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for PinVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let peer = ServerIdentity {
            digest: certificate_pin(end_entity.as_ref(), now)?,
        };
        check_server_pin(&self.pin, Some(&peer)).map_err(|_| {
            Error::InvalidCertificate(CertificateError::ApplicationVerificationFailure)
        })?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            certificate,
            signature,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ECDSA_NISTP256_SHA256]
    }
}

/// Refresh the OS timeout before *each syscall*, against one absolute deadline.
/// A peer trickling handshake or record bytes cannot extend an operation.
struct DeadlineSocket {
    socket: TcpStream,
    until: Instant,
}

impl DeadlineSocket {
    fn remaining(&self) -> io::Result<Duration> {
        self.until
            .checked_duration_since(Instant::now())
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "workspace TLS deadline expired")
            })
    }
}

impl Read for DeadlineSocket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.socket.set_read_timeout(Some(self.remaining()?))?;
        self.socket.read(bytes)
    }
}

impl Write for DeadlineSocket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.socket.set_write_timeout(Some(self.remaining()?))?;
        self.socket.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.remaining()?;
        self.socket.flush()
    }
}

/// A peer-verified stream; construction never accepts application bytes.
/// Debug deliberately omits TLS internals and buffered protocol frames.
pub(crate) struct TlsChannelTransport {
    stream: StreamOwned<ClientConnection, DeadlineSocket>,
    peer: ServerIdentity,
    operation_timeout: Duration,
}

impl std::fmt::Debug for TlsChannelTransport {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TlsChannelTransport")
            .field("peer", &self.peer)
            .finish_non_exhaustive()
    }
}

impl TlsChannelTransport {
    /// Dial an already resolved, qualified address. Name resolution and backend
    /// selection belong to the reachability boundary, not this socket adapter.
    pub(crate) fn connect(address: SocketAddr, pin: Digest, timeout: Duration) -> io::Result<Self> {
        if timeout.is_zero() || pin == Digest([0; 32]) {
            return Err(invalid("workspace TLS requires a deadline and a known pin"));
        }
        let until = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| invalid("invalid workspace TLS deadline"))?;
        let socket = TcpStream::connect_timeout(&address, timeout)?;
        socket.set_nodelay(true)?;
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = ClientConfig::builder_with_provider(provider.clone())
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|_| invalid("workspace TLS configuration failed"))?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinVerifier {
                pin: pin.clone(),
                provider,
            }))
            .with_no_client_auth();
        // The public key is the identity. No OS trust store or hostname fallback.
        let name =
            ServerName::try_from("effigy-host").map_err(|_| invalid("invalid channel name"))?;
        let connection = ClientConnection::new(Arc::new(config), name)
            .map_err(|_| invalid("workspace TLS configuration failed"))?;
        let mut stream = StreamOwned::new(connection, DeadlineSocket { socket, until });
        while stream.conn.is_handshaking() {
            stream.conn.complete_io(&mut stream.sock)?;
        }
        // Defence in depth at the handoff. Only a completed TLS handshake can
        // produce this type; no raw stream or verifier assertion is exposed.
        let der = stream
            .conn
            .peer_certificates()
            .and_then(|chain| chain.first())
            .ok_or_else(|| invalid("workspace TLS peer has no certificate"))?;
        let peer = ServerIdentity {
            digest: certificate_pin(der.as_ref(), UnixTime::now())
                .map_err(|_| invalid("workspace TLS identity expired"))?,
        };
        check_server_pin(&pin, Some(&peer)).map_err(|_| invalid("workspace TLS pin mismatch"))?;
        Ok(Self {
            stream,
            peer,
            operation_timeout: timeout,
        })
    }

    fn begin_operation(&mut self) -> io::Result<()> {
        self.stream.sock.until = Instant::now()
            .checked_add(self.operation_timeout)
            .ok_or_else(|| invalid("invalid workspace TLS deadline"))?;
        Ok(())
    }

    /// Send close_notify within one operation budget, then close the socket even
    /// if the peer never acknowledges it. Drop also closes without waiting.
    pub(crate) fn close(mut self) -> io::Result<()> {
        self.begin_operation()?;
        self.stream.conn.send_close_notify();
        let result = self.stream.flush();
        let closed = self.stream.sock.socket.shutdown(Shutdown::Both);
        result.and(closed)
    }
}

impl Transport for TlsChannelTransport {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.begin_operation()?;
        self.stream.read(bytes)
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.begin_operation()?;
        self.stream.write_all(bytes)?;
        self.stream.flush()
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        Some(self.peer.clone())
    }
}

#[cfg(test)]
mod tests;
