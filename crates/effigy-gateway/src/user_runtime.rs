//! User-owned gateway migration boundary.
//!
//! Task 155 qualification for the operator-selected route ("the gateway
//! restarts as me"): a finite source inventory of which gateway operations
//! still require an administrator, a read-only readiness gate that reports
//! missing setup without elevating, writing, or silently claiming DNS is
//! ready, and byte-exact migration preservation for the route table and
//! loopback registry.
//!
//! Authority: design and private qualification only. Nothing here elevates,
//! writes `/etc/resolver`, provisions interfaces, installs trust, signals a
//! foreign process, or starts a gateway. The exact loopback lowports
//! (`127.0.0.1:80`/`443`) cannot be bound without privilege (see
//! [`probe_loopback_lowport_once`] evidence and the XNU reserved-port branch
//! cited in architecture 034); the one missing capability is identified by
//! [`MISSING_CAPABILITY_ID`] and remains a design boundary, not a supported
//! start path.

use std::net::{Ipv4Addr, TcpListener};

/// Operations the gateway depends on, classified by who must perform them.
///
/// `Administrator` means the operation needs privilege (or an interactive
/// administrator prompt) and is never part of the unprivileged steady state.
/// `OperatorUser` means the authenticated operator's own files/state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupOwner {
    Administrator,
    OperatorUser,
}

/// One classified gateway operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivilegedOperation {
    /// Stable inventory id (also used in readiness reports).
    pub id: &'static str,
    /// Who must perform it.
    pub owner: SetupOwner,
    /// Why, in one bounded sentence.
    pub summary: &'static str,
}

struct InventoryEntry {
    id: &'static str,
    owner: SetupOwner,
    summary: &'static str,
}

const INVENTORY: &[InventoryEntry] = &[
    InventoryEntry {
        id: "loopback-aliases",
        owner: SetupOwner::Administrator,
        summary: "creating lo0 aliases for the bounded 127.1.0.1-127.1.0.50 pool needs privilege; the allocation registry file itself is operator user state",
    },
    InventoryEntry {
        id: "bootstrap-resolver",
        owner: SetupOwner::Administrator,
        summary: "writing /etc/resolver/test pointing at the gateway DNS port needs privilege; the user daemon never writes it",
    },
    InventoryEntry {
        id: "route-resolvers",
        owner: SetupOwner::Administrator,
        summary: "route-driven /etc/resolver/<public-suffix> files for non-test domains need privilege; the user daemon reports them missing and leaves them to the explicit setup path",
    },
    InventoryEntry {
        id: "lowport-bind",
        owner: SetupOwner::Administrator,
        summary: "binding TCP 127.0.0.1:80 and 127.0.0.1:443 needs privilege even on loopback (EACCES for unprivileged callers; XNU in_pcb checks reserved-port privilege for non-wildcard binds); DNS UDP 127.0.0.1:15353 is above 1024 and binds without privilege",
    },
    InventoryEntry {
        id: "ca-trust",
        owner: SetupOwner::Administrator,
        summary: "installing CA trust in the system store needs privilege; certificate files and a fixture CA directory are operator user state and existing trust must not be replaced incidentally",
    },
    InventoryEntry {
        id: "old-root-stop",
        owner: SetupOwner::Administrator,
        summary: "stopping the old root-owned gateway generation needs the existing fresh-generation verification and bounded interactive root lifecycle path; no PID-only signal and no successor start before confirmed absence",
    },
    InventoryEntry {
        id: "state-ownership",
        owner: SetupOwner::OperatorUser,
        summary: "gateway records, route table, loopback registry, certificates and logs stay owned by the operator with owner-only modes; migration performs no recursive ownership change and preserves ownership and metadata",
    },
];

/// Finite source inventory of gateway operations by required owner.
pub fn privilege_inventory() -> Vec<PrivilegedOperation> {
    INVENTORY
        .iter()
        .map(|entry| PrivilegedOperation {
            id: entry.id,
            owner: entry.owner,
            summary: entry.summary,
        })
        .collect()
}

/// The one missing capability for an unprivileged exact-endpoint gateway.
///
/// Loopback-only privileged listener handoff: a mechanism that pre-binds
/// TCP `127.0.0.1:80`/`443` (and only those loopback sockets) outside the
/// user daemon and hands ownership to it, with launchd-owned lifetime
/// (reboot restores it), no wildcard listener, no URL change, no packet
/// filter, and no new standing Effigy helper. This is a design boundary
/// requiring a separate operator ruling and genuine cross-boot proof; it is
/// not implemented here and no supported user start path depends on it.
pub const MISSING_CAPABILITY_ID: &str = "loopback-privileged-listener-handoff";

/// Injected setup evidence for the readiness gate.
///
/// Every field is injected by the caller (native evidence or an explicit
/// control); this gate performs no I/O, elevation, or mutation itself, so
/// native port permission evidence stays separate from injectable controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserRuntimeSetup {
    /// `/etc/resolver/test` points at the gateway DNS port.
    pub bootstrap_resolver_configured: bool,
    /// Route-driven `/etc/resolver/<suffix>` files exist for non-test routes.
    pub route_resolvers_configured: bool,
    /// The `127.1.0.1`-`127.1.0.50` alias range is present on `lo0`.
    pub loopback_aliases_configured: bool,
    /// Native proof that this UID may bind the exact loopback lowports.
    pub lowport_bind_permitted: bool,
    /// Client CA trust covers the gateway certificates.
    pub ca_trust_ready: bool,
    /// The old gateway generation probes confirmed absent.
    pub old_generation_absent: bool,
}

/// Read-only verdict of the user-runtime readiness gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserRuntimeReadiness {
    /// True only when every setup fact holds and no capability is missing.
    pub may_start_unprivileged: bool,
    /// Inventory ids of the missing setup facts.
    pub missing_setup: Vec<&'static str>,
    /// Set exactly when unprivileged lowport bind is unavailable.
    pub missing_capability: Option<&'static str>,
}

/// Check whether an unprivileged user-owned gateway may start.
///
/// Pure and side-effect free: it never elevates, writes, probes, or claims
/// DNS ready. A `false` verdict names the missing setup; callers must refuse
/// the start and preserve all records.
pub fn check_user_runtime_readiness(setup: &UserRuntimeSetup) -> UserRuntimeReadiness {
    let mut missing_setup = Vec::new();
    if !setup.bootstrap_resolver_configured {
        missing_setup.push("bootstrap-resolver");
    }
    if !setup.route_resolvers_configured {
        missing_setup.push("route-resolvers");
    }
    if !setup.loopback_aliases_configured {
        missing_setup.push("loopback-aliases");
    }
    if !setup.lowport_bind_permitted {
        missing_setup.push("lowport-bind");
    }
    if !setup.ca_trust_ready {
        missing_setup.push("ca-trust");
    }
    if !setup.old_generation_absent {
        missing_setup.push("old-generation-absent");
    }
    let missing_capability = if setup.lowport_bind_permitted {
        None
    } else {
        Some(MISSING_CAPABILITY_ID)
    };
    UserRuntimeReadiness {
        may_start_unprivileged: missing_setup.is_empty(),
        missing_setup,
        missing_capability,
    }
}

/// Render a readiness verdict with fixed labels only.
///
/// No paths, digests, PIDs, or raw errors are included.
pub fn render_readiness_diagnosis(readiness: &UserRuntimeReadiness) -> String {
    if readiness.may_start_unprivileged {
        return "user-runtime=ready".to_owned();
    }
    let mut rendered = format!(
        "user-runtime=refused missing-setup={}",
        readiness.missing_setup.join(",")
    );
    if let Some(capability) = readiness.missing_capability {
        rendered.push_str(&format!(" missing-capability={capability}"));
    }
    rendered
}

/// Outcome of the single native loopback lowport bind proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowportBindOutcome {
    /// This UID bound the loopback lowport (socket closed immediately).
    Permitted,
    /// The kernel denied the bind; carries the exact `errno`.
    Denied {
        /// Raw `errno` from the failed bind (e.g. `EACCES` = 13).
        errno: i32,
    },
    /// The port is already held; not permission to try another endpoint.
    InUse,
}

/// Why a bind proof was refused without touching the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowportProbeRefusal {
    /// The current gateway endpoints are never probe targets.
    ReservedEndpoint,
    /// Only `1..1024` may be probed.
    NotALowport,
}

/// Bind `127.0.0.1:<port>` exactly once and close only that socket.
///
/// Guards: the current gateway endpoints (`80`, `443`, `15353`) are never
/// bound, only loopback, only one attempt, no retry loop, no wildcard bind.
/// A denied or colliding result is evidence of an unsupported gate, not
/// permission to probe further endpoints.
pub fn probe_loopback_lowport_once(port: u16) -> Result<LowportBindOutcome, LowportProbeRefusal> {
    if matches!(port, 80 | 443 | 15353) {
        return Err(LowportProbeRefusal::ReservedEndpoint);
    }
    if !(1..1024).contains(&port) {
        return Err(LowportProbeRefusal::NotALowport);
    }
    match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => {
            drop(listener);
            Ok(LowportBindOutcome::Permitted)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
            Ok(LowportBindOutcome::InUse)
        }
        Err(error) => Ok(LowportBindOutcome::Denied {
            errno: error.raw_os_error().unwrap_or(-1),
        }),
    }
}

/// Byte snapshot of the migration inputs that must be preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSnapshot {
    /// Exact `routes.json` bytes (envelope, ownership, metadata included).
    pub routes_bytes: Vec<u8>,
    /// Exact `loopback-ips.json` bytes.
    pub registry_bytes: Vec<u8>,
}

/// Snapshot the migration inputs before any transition step.
pub fn snapshot_migration_inputs(
    routes_bytes: Vec<u8>,
    registry_bytes: Vec<u8>,
) -> MigrationSnapshot {
    MigrationSnapshot {
        routes_bytes,
        registry_bytes,
    }
}

/// Verify the current files still equal the snapshot byte-for-byte.
///
/// Any concurrent edit, replaced parent, or partial write fails closed:
/// callers must hold the migration and preserve the evidence.
pub fn verify_migration_preserved(
    before: &MigrationSnapshot,
    routes_now: &[u8],
    registry_now: &[u8],
) -> bool {
    before.routes_bytes.as_slice() == routes_now && before.registry_bytes.as_slice() == registry_now
}

/// Named old-to-new gateway generation transition.
///
/// Both generations are named so reviewers can confirm the old exact
/// generation stopped before the new user generation started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationTransition {
    /// Recorded PID of the stopped old (root) generation.
    pub old_pid: u32,
    /// Record digest of the stopped old generation.
    pub old_digest: String,
    /// Recorded PID of the started new (user) generation.
    pub new_pid: u32,
    /// Record digest of the started new generation.
    pub new_digest: String,
}

/// Render a generation transition with bounded label-only fields.
pub fn render_generation_transition(transition: &GenerationTransition) -> String {
    format!(
        "old-generation pid={} digest={} -> new-generation pid={} digest={}",
        transition.old_pid, transition.old_digest, transition.new_pid, transition.new_digest,
    )
}

#[cfg(test)]
#[path = "user_runtime/tests.rs"]
mod tests;
