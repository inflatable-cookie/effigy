//! macOS resolver integration.
//!
//! On macOS, the system resolver checks `/etc/resolver/<suffix>` for
//! per-suffix DNS overrides — any FQDN whose label-aligned tail matches the
//! filename gets routed to the listed nameserver. Effigy uses this both for
//! its managed TLD (typically `test`) and for individual public-domain
//! routes that a container manifest opts into via gateway routes (e.g.
//! `dev.cumberland.co.uk` so the route fronts the real public domain
//! locally instead of hitting upstream DNS).
//!
//! The resolver file contains:
//! ```text
//! nameserver 127.0.0.1
//! port 15353
//! ```
//!
//! Two write paths exist:
//!
//! - `ResolverSpec::install` / `uninstall` — the original sudo-shelling
//!   variants used by the privilege-escalation flow that brings the
//!   gateway up. Stays the right answer for the bootstrap TLD file
//!   because it runs from the unprivileged runner.
//! - `ResolverSpec::install_direct` / `uninstall_direct` — direct
//!   filesystem ops for use inside the gateway daemon, which already
//!   runs as root. Used by the per-route reconciler so route-driven
//!   resolver files appear and disappear without any extra elevation
//!   prompts.

use std::path::{Path, PathBuf};

use crate::error::GatewayError;

/// Standard macOS resolver directory.
const RESOLVER_DIR: &str = "/etc/resolver";

/// Magic substring written into managed resolver files so the daemon
/// (and the gateway-down sweep) can tell its own files from anything a
/// human or unrelated tool might have dropped into `/etc/resolver/`.
pub(crate) const MANAGED_HEADER_TAG: &str = "Managed by Effigy gateway";

/// Contents of a resolver file for a given port.
fn resolver_file_contents(port: u16) -> String {
    format!(
        "# {MANAGED_HEADER_TAG} — do not edit manually.\n\
         # Remove this file with: effigy gateway down\n\
         nameserver 127.0.0.1\n\
         port {port}\n"
    )
}

/// Whether the file at `path` carries the Effigy managed-by header.
///
/// Used by the gateway-down sweep to safely identify the set of resolver
/// files Effigy is responsible for, regardless of whether they were
/// written by the bootstrap (sudo) path or by the daemon's per-route
/// reconciler.
pub fn file_is_effigy_managed(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.contains(MANAGED_HEADER_TAG))
        .unwrap_or(false)
}

/// Enumerate every resolver file Effigy currently has under
/// `/etc/resolver/` (matched by the managed-by header).
///
/// Returns an empty `Vec` if the directory does not exist or cannot be
/// read; this is intentionally best-effort so callers can use it as a
/// cleanup driver.
pub fn enumerate_managed_resolver_files() -> Vec<PathBuf> {
    let dir = Path::new(RESOLVER_DIR);
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut managed = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if file_is_effigy_managed(&path) {
            managed.push(path);
        }
    }
    managed
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

    /// Write the resolver file directly (no sudo).
    ///
    /// Intended for the gateway daemon, which already runs as root via
    /// the elevation flow that started it. Skips the write if the file
    /// already has the desired content (idempotent reconciliation).
    pub fn install_direct(&self) -> Result<(), GatewayError> {
        if !resolver_dir_exists() {
            std::fs::create_dir_all(RESOLVER_DIR).map_err(GatewayError::Io)?;
        }
        if let Ok(existing) = std::fs::read_to_string(&self.path) {
            if existing == self.content {
                return Ok(());
            }
        }
        std::fs::write(&self.path, &self.content).map_err(GatewayError::Io)
    }

    /// Remove the resolver file directly (no sudo).
    ///
    /// Intended for the gateway daemon. Refuses to remove a file that
    /// does not carry the Effigy managed-by header, so a misconfigured
    /// path can never delete an unrelated user-authored resolver entry.
    pub fn uninstall_direct(&self) -> Result<(), GatewayError> {
        if !self.path.exists() {
            return Ok(());
        }
        if !file_is_effigy_managed(&self.path) {
            return Ok(());
        }
        std::fs::remove_file(&self.path).map_err(GatewayError::Io)
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

/// Compute the set of route-driven resolver suffixes that need
/// `/etc/resolver/<suffix>` files.
///
/// Filters out anything under `managed_tld` (those are covered by the
/// existing single bootstrap file the elevation flow installs) plus
/// duplicates across routes. Returns a sorted, lower-cased Vec for
/// deterministic reconcile diffs.
pub fn route_driven_resolver_suffixes<I, S>(route_domains: I, managed_tld: &str) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let managed_tld = managed_tld.to_lowercase();
    let mut seen = std::collections::BTreeSet::new();
    for domain in route_domains {
        let domain = domain.as_ref().trim_end_matches('.').to_lowercase();
        if domain.is_empty() {
            continue;
        }
        // Skip anything that the single bootstrap resolver file already covers.
        if domain == managed_tld {
            continue;
        }
        if domain.ends_with(&format!(".{managed_tld}")) {
            continue;
        }
        seen.insert(domain);
    }
    seen.into_iter().collect()
}

/// Reconcile `/etc/resolver/` against the desired set of route-driven
/// suffixes and the managed bootstrap TLD.
///
/// "Desired" is whatever `route_driven_resolver_suffixes` produces for
/// the live route set. Any Effigy-managed resolver file outside that
/// set (and outside the bootstrap TLD, which the runner manages) gets
/// removed. Missing files get written. Files we don't recognise are
/// left alone.
///
/// All filesystem operations are direct (no sudo) — this runs inside
/// the gateway daemon. Returns the (added, removed) path lists for
/// logging / tests; either may be empty on a no-op pass.
pub fn reconcile_route_resolver_files(
    desired_suffixes: &[String],
    managed_tld: &str,
    port: u16,
) -> Result<ReconcileOutcome, GatewayError> {
    let bootstrap_path = resolver_path(managed_tld);
    let mut added = Vec::new();
    let mut removed = Vec::new();

    // Add anything missing.
    for suffix in desired_suffixes {
        let spec = resolver_file_spec(suffix, port);
        let already_present = std::fs::read_to_string(&spec.path)
            .map(|existing| existing == spec.content)
            .unwrap_or(false);
        spec.install_direct()?;
        if !already_present {
            added.push(spec.path);
        }
    }

    // Remove anything Effigy used to manage that isn't desired anymore.
    let desired_paths: std::collections::BTreeSet<PathBuf> = desired_suffixes
        .iter()
        .map(|suffix| resolver_path(suffix))
        .collect();

    for managed in enumerate_managed_resolver_files() {
        if managed == bootstrap_path {
            // Bootstrap TLD file is owned by the runner-side elevation
            // flow; the daemon must not race it.
            continue;
        }
        if desired_paths.contains(&managed) {
            continue;
        }
        // Synthesise a spec-equivalent for the remove call so the
        // managed-header guard runs consistently.
        let spec = ResolverSpec {
            path: managed.clone(),
            content: String::new(),
        };
        spec.uninstall_direct()?;
        removed.push(managed);
    }

    Ok(ReconcileOutcome { added, removed })
}

/// Outcome of a single resolver-file reconcile pass.
#[derive(Debug, Clone, Default)]
pub struct ReconcileOutcome {
    /// Files newly written (or rewritten because content drifted).
    pub added: Vec<PathBuf>,
    /// Effigy-managed files removed because they no longer correspond
    /// to a registered route domain.
    pub removed: Vec<PathBuf>,
}

#[cfg(test)]
#[path = "resolver_setup/tests.rs"]
mod tests;
