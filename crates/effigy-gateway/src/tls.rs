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

use std::path::{Path, PathBuf};
use std::process::Command;

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
