//! Read-path trust verification for the gateway route table.
//!
//! The gateway daemon runs with elevated privilege and proxies to whatever
//! upstream each route names, so the route table file is a privilege boundary.
//! Before the daemon trusts the file, it verifies two things, per
//! `docs/contracts/033-gateway-route-table-trust-contract.md`:
//!
//! 1. **Permission** — the file must not be group- or other-writable, so a
//!    non-owner local user cannot tamper with it. (The gateway directory lives
//!    under the owner's home; this guards against a loosened file mode.)
//! 2. **Provenance** — the file must carry the Effigy-managed marker Effigy
//!    stamps on save, mirroring the managed-by marker used for resolver files.
//!
//! An absent file is trusted as an empty table (matching `RouteTable::load`).
//! A file that fails either check is untrusted, and the caller fails closed:
//! it keeps its last-known-good in-memory table rather than adopting the file.

use std::path::Path;

use crate::error::GatewayError;
use crate::routes::{RouteTable, ROUTE_TABLE_MANAGED_MARKER};

/// Verdict for whether the on-disk route table can be trusted by the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteTableTrust {
    /// File does not exist — an empty table is trusted.
    Absent,
    /// File is owner-scoped and carries the Effigy-managed marker.
    Trusted,
    /// File failed a permission or provenance check; do not adopt it.
    Untrusted { reason: String },
}

impl RouteTableTrust {
    /// True when the daemon may load the file (absent or verified).
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Absent | Self::Trusted)
    }

    /// The reason a table was rejected, if any.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Untrusted { reason } => Some(reason),
            _ => None,
        }
    }
}

/// Inspect the route table file's permission and provenance for the daemon.
pub fn inspect_route_table_trust(path: &Path) -> RouteTableTrust {
    if !path.exists() {
        return RouteTableTrust::Absent;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(meta) => {
                let mode = meta.permissions().mode() & 0o777;
                if mode & 0o022 != 0 {
                    return RouteTableTrust::Untrusted {
                        reason: format!(
                            "route table is group/other-writable (mode {mode:#o}); refusing to trust"
                        ),
                    };
                }
            }
            Err(error) => {
                return RouteTableTrust::Untrusted {
                    reason: format!("cannot stat route table: {error}"),
                };
            }
        }
    }

    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) => {
            return RouteTableTrust::Untrusted {
                reason: format!("cannot read route table: {error}"),
            };
        }
    };

    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(error) => {
            return RouteTableTrust::Untrusted {
                reason: format!("route table is not valid JSON: {error}"),
            };
        }
    };

    match value.get("_managed_by").and_then(|marker| marker.as_str()) {
        Some(marker) if marker == ROUTE_TABLE_MANAGED_MARKER => RouteTableTrust::Trusted,
        Some(other) => RouteTableTrust::Untrusted {
            reason: format!("route table carries an unrecognized managed marker `{other}`"),
        },
        None => RouteTableTrust::Untrusted {
            reason: "route table has no Effigy managed marker".to_string(),
        },
    }
}

/// Load the route table for the daemon, enforcing the read-path trust gate.
///
/// Returns `Some(table)` when the file is absent (empty table) or trusted, and
/// `None` when it is untrusted — the caller keeps its last-known-good in-memory
/// table (fail-closed). A rejection is logged with its reason.
pub fn load_trusted(path: &Path) -> Result<Option<RouteTable>, GatewayError> {
    match inspect_route_table_trust(path) {
        RouteTableTrust::Absent | RouteTableTrust::Trusted => Ok(Some(RouteTable::load(path)?)),
        RouteTableTrust::Untrusted { reason } => {
            tracing::warn!(
                path = %path.display(),
                reason = %reason,
                "refusing to trust route table; keeping last-known-good table"
            );
            Ok(None)
        }
    }
}

#[cfg(test)]
#[path = "trust/tests.rs"]
mod tests;
