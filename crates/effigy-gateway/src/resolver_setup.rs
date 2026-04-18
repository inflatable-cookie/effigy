//! macOS resolver integration.
//!
//! On macOS, the system resolver checks `/etc/resolver/<tld>` for per-TLD
//! DNS overrides. This module manages that file so `*.test` domains resolve
//! through the effigy gateway's DNS server.
//!
//! The resolver file contains:
//! ```text
//! nameserver 127.0.0.1
//! port 15353
//! ```
//!
//! Writing to `/etc/resolver/` requires root privileges. The caller is
//! responsible for invoking this through an elevated path.

use std::path::{Path, PathBuf};

use crate::error::GatewayError;

/// Standard macOS resolver directory.
const RESOLVER_DIR: &str = "/etc/resolver";

/// Contents of a resolver file for a given port.
fn resolver_file_contents(port: u16) -> String {
    format!(
        "# Managed by Effigy gateway — do not edit manually.\n\
         # Remove this file with: effigy gateway down\n\
         nameserver 127.0.0.1\n\
         port {port}\n"
    )
}

/// Path to the resolver file for a given TLD.
pub fn resolver_path(tld: &str) -> PathBuf {
    PathBuf::from(RESOLVER_DIR).join(tld)
}

/// Check if the resolver file exists and points to the correct port.
pub fn is_resolver_configured(tld: &str, expected_port: u16) -> bool {
    let path = resolver_path(tld);
    if let Ok(content) = std::fs::read_to_string(&path) {
        content.contains(&format!("port {expected_port}"))
    } else {
        false
    }
}

/// Check if the resolver directory exists (macOS-specific).
pub fn resolver_dir_exists() -> bool {
    Path::new(RESOLVER_DIR).is_dir()
}

/// Generate the resolver file content for writing.
///
/// This function does NOT write the file (which requires root).
/// It returns the content and path so the caller can handle privilege
/// escalation.
pub fn resolver_file_spec(tld: &str, port: u16) -> ResolverSpec {
    ResolverSpec {
        path: resolver_path(tld),
        content: resolver_file_contents(port),
    }
}

/// Spec for a resolver file that needs to be written.
#[derive(Debug, Clone)]
pub struct ResolverSpec {
    /// Path where the file should be written.
    pub path: PathBuf,
    /// Content of the file.
    pub content: String,
}

impl ResolverSpec {
    /// Write the resolver file using sudo.
    ///
    /// The caller is expected to have already triggered the needed privilege
    /// escalation path before invoking this.
    pub fn install(&self) -> Result<(), GatewayError> {
        use std::process::{Command, Stdio};

        // Ensure the directory exists.
        if !resolver_dir_exists() {
            let output = Command::new("sudo")
                .args(["mkdir", "-p", RESOLVER_DIR])
                .output()
                .map_err(GatewayError::Io)?;
            if !output.status.success() {
                return Err(GatewayError::DnsBindError {
                    addr: RESOLVER_DIR.to_string(),
                    reason: render_sudo_failure("failed to create resolver directory", &output),
                });
            }
        }

        // Write the file via sudo tee.
        let mut child = Command::new("sudo")
            .args(["tee", self.path.to_str().unwrap_or("")])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(GatewayError::Io)?;

        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(self.content.as_bytes())?;
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(GatewayError::DnsBindError {
                addr: self.path.display().to_string(),
                reason: render_sudo_failure("failed to write resolver file", &output),
            });
        }

        Ok(())
    }

    /// Remove the resolver file using sudo.
    pub fn uninstall(&self) -> Result<(), GatewayError> {
        if !self.path.exists() {
            return Ok(());
        }

        let output = std::process::Command::new("sudo")
            .args(["rm", self.path.to_str().unwrap_or("")])
            .output()
            .map_err(GatewayError::Io)?;

        if !output.status.success() {
            return Err(GatewayError::DnsBindError {
                addr: self.path.display().to_string(),
                reason: render_sudo_failure("failed to remove resolver file", &output),
            });
        }

        Ok(())
    }
}

fn render_sudo_failure(context: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stderr.is_empty() {
        format!("{context}: {stderr}")
    } else if !stdout.is_empty() {
        format!("{context}: {stdout}")
    } else {
        context.to_string()
    }
}

#[cfg(test)]
#[path = "resolver_setup/tests.rs"]
mod tests;
