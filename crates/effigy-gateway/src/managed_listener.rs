//! Generation-bound validation for managed host listener routes.

use std::fs;
use std::io;
use std::net::SocketAddr;

use crate::identity::{read_live_process_identity, GatewayStartIdentity};
use crate::routes::ManagedListenerRouteOwner;
use crate::socket_ownership::{process_listening_endpoints, GatewayEndpoint, GatewayTransport};

#[derive(serde::Deserialize)]
struct ManagedListenerAvailability {
    schema: String,
    owner: String,
    runtime_generation: String,
    generation: String,
    status: String,
}

const AVAILABILITY_SCHEMA: &str = "effigy.managed.host-listener-state.v1";

#[cfg(test)]
pub(crate) static OWNERSHIP_PROOFS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// A failed observation never grants another owner permission to replace the
/// listener. Only a positively absent process or changed generation does.
pub fn observe_managed_listener_owner(
    owner: &ManagedListenerRouteOwner,
) -> effigy_kernel::observe::Observed<bool> {
    use effigy_kernel::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
    for (pid, boot, start) in [
        (
            owner.supervisor_pid,
            &owner.supervisor_boot_identity,
            &owner.supervisor_start_identity,
        ),
        (
            owner.root_pid,
            &owner.root_boot_identity,
            &owner.root_start_identity,
        ),
        (
            owner.listener_pid,
            &owner.listener_boot_identity,
            &owner.listener_start_identity,
        ),
    ] {
        match read_live_process_identity(pid) {
            Ok(current) if current.boot_identity != *boot || current.start_identity != *start => {
                return Observed::Absent(AbsenceProof::StartIdentityDiffers { pid })
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) =>
            {
                return Observed::Absent(AbsenceProof::NoSuchProcess { pid })
            }
            Err(error) => return Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
        }
    }
    match verify_managed_listener_owner(owner) {
        Ok(()) => Observed::Verified(
            true,
            Evidence::new(
                "managed listener identity and exclusive socket".into(),
                None,
            )
            .expect("fixed source"),
        ),
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
}

/// Prove that a route's current TCP listener still belongs to its recorded
/// process generation and remains beneath the recorded managed child.
/// The engine calls this at publication. Background proxy workers call it
/// after connecting, before making the connection available to requests.
pub fn verify_managed_listener_owner(owner: &ManagedListenerRouteOwner) -> Result<(), io::Error> {
    #[cfg(test)]
    OWNERSHIP_PROOFS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let address = owner.address.parse::<SocketAddr>().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "managed listener route contains an invalid socket address",
        )
    })?;
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "managed listener route is not an assigned loopback TCP endpoint",
        ));
    }
    verify_listener_availability(owner)?;
    verify_process_identity(
        owner.supervisor_pid,
        &owner.supervisor_boot_identity,
        &owner.supervisor_start_identity,
    )?;
    verify_process_identity(
        owner.root_pid,
        &owner.root_boot_identity,
        &owner.root_start_identity,
    )?;
    if owner.root_pid != owner.supervisor_pid
        && !effigy_process::process_is_descendant_of(owner.root_pid, owner.supervisor_pid)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed child is no longer beneath its recorded supervisor",
        ));
    }
    verify_process_identity(
        owner.listener_pid,
        &owner.listener_boot_identity,
        &owner.listener_start_identity,
    )?;
    if owner.listener_pid != owner.root_pid
        && !effigy_process::process_is_descendant_of(owner.listener_pid, owner.root_pid)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed listener process is no longer beneath its recorded child generation",
        ));
    }
    let endpoints = process_listening_endpoints(owner.listener_pid)?;
    if !endpoints.contains(&GatewayEndpoint {
        transport: GatewayTransport::Tcp,
        addr: address,
    }) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "recorded managed process no longer owns the route listener",
        ));
    }
    verify_exclusive_listener_owner(address, owner.listener_pid)?;
    verify_listener_availability(owner)?;

    // Re-read identities and ancestry after socket inspection to close PID
    // reuse and reparenting windows around the kernel socket snapshot.
    verify_process_identity(
        owner.supervisor_pid,
        &owner.supervisor_boot_identity,
        &owner.supervisor_start_identity,
    )?;
    verify_process_identity(
        owner.root_pid,
        &owner.root_boot_identity,
        &owner.root_start_identity,
    )?;
    if owner.root_pid != owner.supervisor_pid
        && !effigy_process::process_is_descendant_of(owner.root_pid, owner.supervisor_pid)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed child ancestry changed during validation",
        ));
    }
    verify_process_identity(
        owner.listener_pid,
        &owner.listener_boot_identity,
        &owner.listener_start_identity,
    )?;
    if owner.listener_pid != owner.root_pid
        && !effigy_process::process_is_descendant_of(owner.listener_pid, owner.root_pid)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed listener process ancestry changed during validation",
        ));
    }
    verify_exclusive_listener_owner(address, owner.listener_pid)?;
    verify_listener_availability(owner)?;
    Ok(())
}

/// Cheap revocation guard for connections already bound to a verified peer.
/// No process inventories, ancestry scans, socket scans or subprocesses.
pub(crate) fn verify_cached_listener_identity(
    owner: &ManagedListenerRouteOwner,
) -> Result<(), io::Error> {
    verify_listener_availability(owner)?;
    verify_process_identity(
        owner.supervisor_pid,
        &owner.supervisor_boot_identity,
        &owner.supervisor_start_identity,
    )?;
    verify_process_identity(
        owner.root_pid,
        &owner.root_boot_identity,
        &owner.root_start_identity,
    )?;
    verify_process_identity(
        owner.listener_pid,
        &owner.listener_boot_identity,
        &owner.listener_start_identity,
    )
}

fn verify_exclusive_listener_owner(
    address: SocketAddr,
    expected_pid: u32,
) -> Result<(), io::Error> {
    crate::socket_ownership::verify_process_listener_exclusive(
        expected_pid,
        &GatewayEndpoint {
            transport: GatewayTransport::Tcp,
            addr: address,
        },
    )
}

fn verify_listener_availability(owner: &ManagedListenerRouteOwner) -> Result<(), io::Error> {
    let path = std::path::Path::new(&owner.availability_file);
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 1_048_576 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "managed listener availability record is not a bounded regular file",
        ));
    }
    let bytes = fs::read(path)?;
    let state: ManagedListenerAvailability = serde_json::from_slice(&bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("managed listener availability record is invalid: {error}"),
        )
    })?;
    if state.schema != AVAILABILITY_SCHEMA
        || state.owner != owner.owner
        || state.runtime_generation != owner.runtime_generation
        || state.generation != owner.generation
        || state.status != "ready"
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed listener generation is not currently ready",
        ));
    }
    Ok(())
}

fn verify_process_identity(
    pid: u32,
    expected_boot: &str,
    expected_start: &GatewayStartIdentity,
) -> Result<(), io::Error> {
    // Proof seam for an unobservable identity: macOS can refuse a process
    // identity read with EPERM (papercut 2ecdcaff). The injected control below
    // shows the refusal can never become an absence waiver.
    #[cfg(feature = "endpoint-store-faults")]
    effigy_kernel::fault_injection::point("gateway.managed-listener.identity")?;
    let current = read_live_process_identity(pid)?;
    if current.boot_identity != expected_boot || &current.start_identity != expected_start {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed listener process identity changed",
        ));
    }
    Ok(())
}

#[cfg(all(test, feature = "endpoint-store-faults"))]
mod tests {
    use super::*;
    use effigy_kernel::observe::Observed;
    use std::path::Path;

    /// A route whose identity observation is refused by the kernel. The
    /// recorded process is this live test process, so nothing about the
    /// generation is absent: only the observation failed.
    fn refused_identity_owner(dir: &Path) -> ManagedListenerRouteOwner {
        let pid = std::process::id();
        let identity = read_live_process_identity(pid).expect("fixture identity");
        let availability_file = dir.join("listener.json");
        fs::write(
            &availability_file,
            serde_json::json!({
                "schema": AVAILABILITY_SCHEMA,
                "owner": "fixture|web|app",
                "runtime_generation": "runtime-generation",
                "generation": "generation",
                "status": "ready",
            })
            .to_string(),
        )
        .expect("write availability record");
        ManagedListenerRouteOwner {
            run: None,
            owner: "fixture|web|app".to_owned(),
            runtime_generation: "runtime-generation".to_owned(),
            generation: "generation".to_owned(),
            supervisor_pid: pid,
            supervisor_boot_identity: identity.boot_identity.clone(),
            supervisor_start_identity: identity.start_identity.clone(),
            root_pid: pid,
            root_boot_identity: identity.boot_identity.clone(),
            root_start_identity: identity.start_identity.clone(),
            listener_pid: pid,
            listener_boot_identity: identity.boot_identity,
            listener_start_identity: identity.start_identity,
            address: "127.0.0.1:9".to_owned(),
            route_tls: false,
            availability_file: availability_file.display().to_string(),
        }
    }

    fn with_permission_denied<T>(run: impl FnOnce() -> T) -> T {
        effigy_kernel::fault_injection::with_hook(
            |point| {
                assert_eq!(point, "gateway.managed-listener.identity");
                Err(io::Error::from_raw_os_error(libc::EPERM))
            },
            run,
        )
    }

    /// An unobservable listener identity stays an unknown refusal. It is never
    /// an absence proof, so it can never license another owner to replace the
    /// listener (papercut 2ecdcaff).
    #[test]
    fn unobservable_listener_identity_refuses_and_stays_unknown() {
        let dir = tempfile::tempdir().expect("fixture dir");
        let owner = refused_identity_owner(dir.path());
        let error = with_permission_denied(|| verify_managed_listener_owner(&owner))
            .expect_err("an unobservable identity must refuse");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied, "{error}");

        match with_permission_denied(|| observe_managed_listener_owner(&owner)) {
            Observed::Unknown(_) => {}
            other => panic!("an unobservable identity must stay Unknown, got {other:?}"),
        }
    }
}
