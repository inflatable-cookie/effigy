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

/// Optional explicit mkcert program path used by elevated gateway runs.
///
/// The runner resolves this from a bounded set of trusted host directories
/// before privilege escalation so the root-owned gateway daemon does not need
/// to trust a caller-controlled `PATH`.
pub const MKCERT_BIN_ENV: &str = "EFFIGY_GATEWAY_MKCERT_BIN";

const SAFE_MKCERT_SEARCH_DIRS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

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
        mkcert_command()
            .arg("-help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Check whether the mkcert CA is installed in the system trust store.
    pub fn ca_installed() -> bool {
        if !mkcert_ca_exists() {
            return false;
        }

        let check = mkcert_command()
            .arg("-check")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output();
        if let Ok(output) = check {
            if output.status.success() {
                return true;
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("flag provided but not defined: -check") {
                return false;
            }
        }

        #[cfg(target_os = "macos")]
        {
            macos_mkcert_ca_installed()
        }

        #[cfg(not(target_os = "macos"))]
        {
            mkcert_ca_exists()
        }
    }

    /// Install the mkcert CA into the system trust store.
    ///
    /// This typically requires user interaction (sudo prompt).
    pub fn install_ca() -> Result<(), GatewayError> {
        let output =
            mkcert_command()
                .arg("-install")
                .output()
                .map_err(|e| GatewayError::TlsError {
                    domain: "<CA>".to_string(),
                    reason: format!("failed to run mkcert -install: {e}"),
                })?;

        if !output.status.success() {
            return Err(GatewayError::TlsError {
                domain: "<CA>".to_string(),
                reason: format!(
                    "mkcert -install failed with exit code {}{}",
                    output.status,
                    format_command_output(&output.stdout, &output.stderr)
                ),
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

        let CertPaths {
            cert: cert_path,
            key: key_path,
        } = self.cert_paths(domain);

        // Skip if certificate already exists.
        if cert_path.exists() && key_path.exists() {
            return Ok(CertPaths {
                cert: cert_path,
                key: key_path,
            });
        }

        let output = mkcert_command()
            .arg("-cert-file")
            .arg(&cert_path)
            .arg("-key-file")
            .arg(&key_path)
            .arg(domain)
            .output()
            .map_err(|e| GatewayError::TlsError {
                domain: domain.to_string(),
                reason: format!("failed to run mkcert: {e}"),
            })?;

        if !output.status.success() {
            return Err(GatewayError::TlsError {
                domain: domain.to_string(),
                reason: format!(
                    "mkcert failed with exit code {}{}",
                    output.status,
                    format_command_output(&output.stdout, &output.stderr)
                ),
            });
        }

        Ok(CertPaths {
            cert: cert_path,
            key: key_path,
        })
    }

    /// Load a certificate and key from disk for use with rustls.
    pub fn load_cert(&self, domain: &str) -> Result<CertPaths, GatewayError> {
        let CertPaths {
            cert: cert_path,
            key: key_path,
        } = self.cert_paths(domain);

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

    /// Return the expected certificate and key paths for a domain.
    pub fn cert_paths(&self, domain: &str) -> CertPaths {
        CertPaths {
            cert: self.certs_dir.join(format!("{domain}.pem")),
            key: self.certs_dir.join(format!("{domain}-key.pem")),
        }
    }

    /// Remove a certificate and key for a domain if they exist.
    pub fn remove_cert(&self, domain: &str) -> Result<(), GatewayError> {
        let paths = self.cert_paths(domain);
        remove_file_if_exists(&paths.cert)?;
        remove_file_if_exists(&paths.key)?;
        Ok(())
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
        let mut certs = crate::locks::write_tolerant(&self.certs);
        certs.insert(domain, cert);
    }

    /// Remove a certificate for a domain.
    pub fn remove_cert(&self, domain: &str) {
        let mut certs = crate::locks::write_tolerant(&self.certs);
        certs.remove(domain);
    }

    /// Remove all registered certificates.
    pub fn clear(&self) {
        let mut certs = crate::locks::write_tolerant(&self.certs);
        certs.clear();
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
        crate::locks::read_tolerant(&self.certs).len()
    }

    /// Check if a domain has a registered certificate.
    pub fn has_cert(&self, domain: &str) -> bool {
        crate::locks::read_tolerant(&self.certs).contains_key(domain)
    }

    /// Build a `rustls::ServerConfig` using this resolver.
    pub fn into_server_config(self) -> rustls::ServerConfig {
        let resolver = Arc::new(self);
        server_config_from_resolver(resolver)
    }
}

/// Build a `rustls::ServerConfig` using a shared resolver.
pub fn server_config_from_resolver(resolver: Arc<SniCertResolver>) -> rustls::ServerConfig {
    let _ = rustls::crypto::ring::default_provider().install_default();
    rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(resolver)
}

/// Reload a resolver from the certificate directory.
pub fn sync_sni_resolver_from_dir(
    resolver: &SniCertResolver,
    certs_dir: &Path,
) -> Result<usize, GatewayError> {
    resolver.clear();

    if !certs_dir.is_dir() {
        return Ok(0);
    }

    let entries = std::fs::read_dir(certs_dir).map_err(|e| GatewayError::TlsError {
        domain: certs_dir.display().to_string(),
        reason: format!("failed to read certs directory: {e}"),
    })?;
    let mut loaded = 0;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if filename.ends_with(".pem") && !filename.ends_with("-key.pem") {
            let domain = filename.trim_end_matches(".pem");
            let key_path = certs_dir.join(format!("{domain}-key.pem"));

            if key_path.exists() {
                match load_certified_key(&path, &key_path) {
                    Ok(cert) => {
                        resolver.add_cert(domain.to_string(), cert);
                        loaded += 1;
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

    Ok(loaded)
}

fn remove_file_if_exists(path: &Path) -> Result<(), GatewayError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(GatewayError::Io(error)),
    }
}

fn mkcert_ca_exists() -> bool {
    mkcert_root_ca_pem().is_some()
}

/// Resolve the directory mkcert stores its local CA in by invoking
/// `mkcert -CAROOT`. Returns `None` when mkcert is not on `PATH` or the
/// command fails.
pub fn mkcert_ca_root() -> Option<PathBuf> {
    let output = mkcert_command().arg("-CAROOT").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

/// Resolve the path to mkcert's local root CA PEM (`rootCA.pem` inside
/// the mkcert CAROOT). Returns `None` when mkcert is not installed or
/// the file does not exist on disk. Used by the container layer to
/// auto-mount the root into workspace catalogs so HTTPS calls from a
/// container back through the host gateway trust the local CA.
pub fn mkcert_root_ca_pem() -> Option<PathBuf> {
    let caroot = mkcert_ca_root()?;
    let pem = caroot.join("rootCA.pem");
    if pem.is_file() {
        Some(pem)
    } else {
        None
    }
}

fn mkcert_command() -> Command {
    if let Some(program) = resolved_mkcert_program() {
        return Command::new(program);
    }
    Command::new("mkcert")
}

/// Resolve the mkcert program from an explicit absolute override or a bounded
/// set of trusted install prefixes.
pub fn resolved_mkcert_program() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os(MKCERT_BIN_ENV).map(PathBuf::from) {
        if explicit.is_absolute() && explicit.is_file() {
            return Some(explicit);
        }
    }

    SAFE_MKCERT_SEARCH_DIRS
        .iter()
        .map(|dir| Path::new(dir).join("mkcert"))
        .find(|path| path.is_file())
}

#[cfg(target_os = "macos")]
fn macos_mkcert_ca_installed() -> bool {
    [
        "/Library/Keychains/System.keychain",
        "~/Library/Keychains/login.keychain-db",
    ]
    .into_iter()
    .any(|keychain| {
        Command::new("sh")
            .args([
                "-lc",
                &format!(
                    "security find-certificate -a -c mkcert {} >/dev/null 2>&1",
                    shell_quote(keychain)
                ),
            ])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_owned();
    }
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn format_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(stderr).trim().to_owned();
    let detail = [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
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
        let certs = crate::locks::read_tolerant(&self.certs);
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
    let _ = sync_sni_resolver_from_dir(&resolver, certs_dir)?;
    Ok(resolver)
}

#[cfg(test)]
#[path = "tls/tests.rs"]
mod tests;
