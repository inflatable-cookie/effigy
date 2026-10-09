//! Generation-bound validation for managed host listener routes.

use std::fs;
use std::io;
use std::net::SocketAddr;

use crate::identity::{read_live_process_identity, GatewayStartIdentity};
use crate::legacy::{process_listening_endpoints, GatewayEndpoint, GatewayTransport};
use crate::routes::ManagedListenerRouteOwner;

#[derive(serde::Deserialize)]
struct ManagedListenerAvailability {
    schema: String,
    owner: String,
    runtime_generation: String,
    generation: String,
    status: String,
}

const AVAILABILITY_SCHEMA: &str = "effigy.managed.host-listener-state.v1";

/// Prove that a route's current TCP listener still belongs to its recorded
/// process generation and remains beneath the recorded managed child.
/// Callers use this after connecting and before forwarding request bytes.
pub fn verify_managed_listener_owner(owner: &ManagedListenerRouteOwner) -> Result<(), io::Error> {
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

fn verify_exclusive_listener_owner(
    address: SocketAddr,
    expected_pid: u32,
) -> Result<(), io::Error> {
    let owners = crate::legacy::listening_process_ids(&GatewayEndpoint {
        transport: GatewayTransport::Tcp,
        addr: address,
    })?;
    if owners.as_slice() != [expected_pid] {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("managed endpoint {address} is shared or has an unowned listener: {owners:?}"),
        ));
    }
    Ok(())
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
    let current = read_live_process_identity(pid)?;
    if current.boot_identity != expected_boot || &current.start_identity != expected_start {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "managed listener process identity changed",
        ));
    }
    Ok(())
}
