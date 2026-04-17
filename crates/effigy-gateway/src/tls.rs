//! TLS certificate management for HTTPS support.
//!
//! Uses `mkcert` for certificate generation and `rustls` for TLS
//! termination. Certificates are stored in `~/.effigy/gateway/certs/`.
//!
//! This module provides:
//!
//! - Certificate generation via mkcert
//! - Certificate loading for the proxy
//! - CA installation status checking
//! - SNI-based certificate resolver for multi-domain HTTPS

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};

use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;

use crate::error::GatewayError;

/// TLS certificate configuration.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// Directory where certificates are stored.
    pub certs_dir: PathBuf,
}

impl TlsConfig {
    /// Create a new TLS config with the given certificate directory.
    pub fn new(certs_dir: PathBuf) -> Self {
        Self { certs_dir }
    }

    /// Check whether mkcert is installed and available.
    pub fn mkcert_available() -> bool {
        Command::new("mkcert")
            .arg("-help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check whether the mkcert CA is installed in the system trust store.
    pub fn ca_installed() -> bool {
        // mkcert -check returns 0 if the CA is installed.
        Command::new("mkcert")
            .arg("-check")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Install the mkcert CA into the system trust store.
    ///
    /// This typically requires user interaction (sudo prompt).
    pub fn install_ca() -> Result<(), GatewayError> {
        let status = Command::new("mkcert")
            .arg("-install")
            .status()
            .map_err(|e| GatewayError::TlsError {
                domain: "<CA>".to_string(),
                reason: format!("failed to run mkcert -install: {e}"),
            })?;

        if !status.success() {
            return Err(GatewayError::TlsError {
                domain: "<CA>".to_string(),
                reason: format!("mkcert -install failed with exit code {status}"),
            });
        }

        Ok(())
    }

    /// Generate a certificate for the given domain.
    ///
    /// Returns the paths to the certificate and key files.
    pub fn generate_cert(&self, domain: &str) -> Result<CertPaths, GatewayError> {
        std::fs::create_dir_all(&self.certs_dir).map_err(|e| GatewayError::TlsError {
            domain: domain.to_string(),
            reason: format!("failed to create certs directory: {e}"),
        })?;

        let cert_path = self.certs_dir.join(format!("{domain}.pem"));
        let key_path = self.certs_dir.join(format!("{domain}-key.pem"));

        // Skip if certificate already exists.
        if cert_path.exists() && key_path.exists() {
            return Ok(CertPaths {
                cert: cert_path,
                key: key_path,
            });
        }

        let status = Command::new("mkcert")
            .arg("-cert-file")
            .arg(&cert_path)
            .arg("-key-file")
            .arg(&key_path)
            .arg(domain)
            .status()
            .map_err(|e| GatewayError::TlsError {
                domain: domain.to_string(),
                reason: format!("failed to run mkcert: {e}"),
            })?;

        if !status.success() {
            return Err(GatewayError::TlsError {
                domain: domain.to_string(),
                reason: format!("mkcert failed with exit code {status}"),
            });
        }

        Ok(CertPaths {
            cert: cert_path,
            key: key_path,
        })
    }

    /// Load a certificate and key from disk for use with rustls.
    pub fn load_cert(&self, domain: &str) -> Result<CertPaths, GatewayError> {
        let cert_path = self.certs_dir.join(format!("{domain}.pem"));
        let key_path = self.certs_dir.join(format!("{domain}-key.pem"));

        if !cert_path.exists() {
            return Err(GatewayError::TlsCertNotFound { path: cert_path });
        }
        if !key_path.exists() {
            return Err(GatewayError::TlsCertNotFound { path: key_path });
        }

        Ok(CertPaths {
            cert: cert_path,
            key: key_path,
        })
    }
}

/// Paths to a TLS certificate and its private key.
#[derive(Debug, Clone)]
pub struct CertPaths {
    /// Path to the PEM-encoded certificate.
    pub cert: PathBuf,
    /// Path to the PEM-encoded private key.
    pub key: PathBuf,
}

/// Load rustls certificate chain and private key from PEM files.
pub fn load_rustls_config(
    cert_path: &Path,
    key_path: &Path,
) -> Result<rustls::ServerConfig, GatewayError> {
    let cert_data = std::fs::read(cert_path).map_err(|e| GatewayError::TlsError {
        domain: cert_path.display().to_string(),
        reason: format!("failed to read cert: {e}"),
    })?;
    let key_data = std::fs::read(key_path).map_err(|e| GatewayError::TlsError {
        domain: key_path.display().to_string(),
        reason: format!("failed to read key: {e}"),
    })?;

    let certs = rustls_pemfile::certs(&mut cert_data.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| GatewayError::TlsError {
            domain: cert_path.display().to_string(),
            reason: format!("failed to parse cert PEM: {e}"),
        })?;

    let key = rustls_pemfile::private_key(&mut key_data.as_slice())
        .map_err(|e| GatewayError::TlsError {
            domain: key_path.display().to_string(),
            reason: format!("failed to parse key PEM: {e}"),
        })?
        .ok_or_else(|| GatewayError::TlsError {
            domain: key_path.display().to_string(),
            reason: "no private key found in PEM file".to_string(),
        })?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| GatewayError::TlsError {
            domain: cert_path.display().to_string(),
            reason: format!("failed to build TLS config: {e}"),
        })?;

    Ok(config)
}

/// Load a certificate chain and private key into a `CertifiedKey`.
fn load_certified_key(
    cert_path: &Path,
    key_path: &Path,
) -> Result<Arc<CertifiedKey>, GatewayError> {
    let cert_data = std::fs::read(cert_path).map_err(|e| GatewayError::TlsError {
        domain: cert_path.display().to_string(),
        reason: format!("failed to read cert: {e}"),
    })?;
    let key_data = std::fs::read(key_path).map_err(|e| GatewayError::TlsError {
        domain: key_path.display().to_string(),
        reason: format!("failed to read key: {e}"),
    })?;

    let certs = rustls_pemfile::certs(&mut cert_data.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| GatewayError::TlsError {
            domain: cert_path.display().to_string(),
            reason: format!("failed to parse cert PEM: {e}"),
        })?;

    let key_der = rustls_pemfile::private_key(&mut key_data.as_slice())
        .map_err(|e| GatewayError::TlsError {
            domain: key_path.display().to_string(),
            reason: format!("failed to parse key PEM: {e}"),
        })?
        .ok_or_else(|| GatewayError::TlsError {
            domain: key_path.display().to_string(),
            reason: "no private key found in PEM file".to_string(),
        })?;

    let signing_key = rustls::crypto::ring::sign::any_supported_type(&key_der).map_err(|e| {
        GatewayError::TlsError {
            domain: cert_path.display().to_string(),
            reason: format!("unsupported key type: {e}"),
        }
    })?;

    Ok(Arc::new(CertifiedKey::new(certs, signing_key)))
}

/// SNI-based certificate resolver for multi-domain HTTPS.
///
/// When the gateway serves multiple projects with TLS, each domain
/// needs its own certificate. This resolver maps SNI hostnames to
/// their `CertifiedKey` and returns the right one during the TLS
/// handshake.
///
/// Thread-safe and supports dynamic addition of certificates at runtime
/// (e.g., when a new TLS route is registered).
pub struct SniCertResolver {
    /// Domain → CertifiedKey mapping.
    certs: RwLock<HashMap<String, Arc<CertifiedKey>>>,

    /// Fallback cert to serve when no SNI match is found.
    fallback: Option<Arc<CertifiedKey>>,
}

impl std::fmt::Debug for SniCertResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.certs.read().map(|c| c.len()).unwrap_or(0);
        f.debug_struct("SniCertResolver")
            .field("cert_count", &count)
            .field("has_fallback", &self.fallback.is_some())
            .finish()
    }
}

impl SniCertResolver {
    /// Create an empty resolver with no fallback.
    pub fn new() -> Self {
        Self {
            certs: RwLock::new(HashMap::new()),
            fallback: None,
        }
    }

    /// Create a resolver with a fallback certificate.
    pub fn with_fallback(fallback: Arc<CertifiedKey>) -> Self {
        Self {
            certs: RwLock::new(HashMap::new()),
            fallback: Some(fallback),
        }
    }

    /// Add a certificate for a domain.
    pub fn add_cert(&self, domain: String, cert: Arc<CertifiedKey>) {
        let mut certs = self.certs.write().expect("cert resolver lock poisoned");
        certs.insert(domain, cert);
    }

    /// Remove a certificate for a domain.
    pub fn remove_cert(&self, domain: &str) {
        let mut certs = self.certs.write().expect("cert resolver lock poisoned");
        certs.remove(domain);
    }

    /// Load and add a certificate from PEM files.
    pub fn load_and_add(
        &self,
        domain: &str,
        cert_path: &Path,
        key_path: &Path,
    ) -> Result<(), GatewayError> {
        let key = load_certified_key(cert_path, key_path)?;
        self.add_cert(domain.to_string(), key);
        Ok(())
    }

    /// Number of registered certificates.
    pub fn cert_count(&self) -> usize {
        self.certs
            .read()
            .expect("cert resolver lock poisoned")
            .len()
    }

    /// Check if a domain has a registered certificate.
    pub fn has_cert(&self, domain: &str) -> bool {
        self.certs
            .read()
            .expect("cert resolver lock poisoned")
            .contains_key(domain)
    }

    /// Build a `rustls::ServerConfig` using this resolver.
    pub fn into_server_config(self) -> rustls::ServerConfig {
        let resolver = Arc::new(self);
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_cert_resolver(resolver)
    }
}

impl Default for SniCertResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolvesServerCert for SniCertResolver {
    fn resolve(&self, client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        // Extract the SNI hostname from the ClientHello.
        let sni = client_hello.server_name()?;

        // Look up the cert for this domain.
        let certs = self.certs.read().expect("cert resolver lock poisoned");
        if let Some(cert) = certs.get(sni) {
            return Some(Arc::clone(cert));
        }

        // Try a wildcard match: *.example.test matches foo.example.test.
        if let Some(dot_pos) = sni.find('.') {
            let wildcard = format!("*{}", &sni[dot_pos..]);
            if let Some(cert) = certs.get(&wildcard) {
                return Some(Arc::clone(cert));
            }
        }

        // Fall back to the default cert if configured.
        self.fallback.as_ref().map(Arc::clone)
    }
}

/// Load all certificates from a directory and build an SNI resolver.
///
/// Scans the directory for `<domain>.pem` and `<domain>-key.pem` pairs.
/// Each pair is loaded and registered under the domain name.
pub fn build_sni_resolver_from_dir(certs_dir: &Path) -> Result<SniCertResolver, GatewayError> {
    let resolver = SniCertResolver::new();

    if !certs_dir.is_dir() {
        return Ok(resolver);
    }

    let entries = std::fs::read_dir(certs_dir).map_err(|e| GatewayError::TlsError {
        domain: certs_dir.display().to_string(),
        reason: format!("failed to read certs directory: {e}"),
    })?;

    // Collect cert files (exclude key files).
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Match <domain>.pem but not <domain>-key.pem.
        if filename.ends_with(".pem") && !filename.ends_with("-key.pem") {
            let domain = filename.trim_end_matches(".pem");
            let key_path = certs_dir.join(format!("{domain}-key.pem"));

            if key_path.exists() {
                match load_certified_key(&path, &key_path) {
                    Ok(cert) => {
                        resolver.add_cert(domain.to_string(), cert);
                    }
                    Err(e) => {
                        tracing::warn!(
                            domain = domain,
                            error = %e,
                            "skipping cert with load error"
                        );
                    }
                }
            }
        }
    }

    Ok(resolver)
}

#[cfg(test)]
#[path = "tls/tests.rs"]
mod tests;
