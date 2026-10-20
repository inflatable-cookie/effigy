use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    export_volume_command, import_volume_command, inspect_volumes_command, list_volumes_command,
    merge_runtime_volume_metadata, parse_inspect_volume_metadata_list,
    parse_inspect_volume_metadata_list_strict, parse_listed_volume_names, verify_volume_identity,
    DockerCommand, ManagedVolume, RuntimeVolumeMetadata, VolumeIdentity,
};
use effigy_containers::{ContainerDataTransferAction, EffectiveContainerPolicy};

use crate::EffigyRuntimeError;

pub(super) fn run_volume_transfer<F>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
    archive_path: &Path,
    action: ContainerDataTransferAction,
    run_runtime_volume_capture: &F,
) -> Result<(), EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let endpoint = policy_endpoint(repo_root, policy)?;
    let identities = volume_identities(
        repo_root,
        &endpoint,
        &policy.project_name,
        &[volume_name.to_owned()],
        run_runtime_volume_capture,
    )?;
    if let Some(VolumeIdentity::Lookalike(reason)) = identities.get(volume_name) {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "container `{}` refuses to transfer data: {reason}; it was not created for this environment",
            policy.name
        )));
    }
    let command = match action {
        ContainerDataTransferAction::Export => export_volume_command(volume_name, archive_path),
        ContainerDataTransferAction::Import => import_volume_command(volume_name, archive_path),
    };
    run_runtime_volume_capture(repo_root, &endpoint, &command)?;
    Ok(())
}

/// The environment's exact backend endpoint for volume actions.
pub(super) fn policy_endpoint(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<String, EffigyRuntimeError> {
    effigy_containers::backend::runtime_endpoint(repo_root, policy)
        .map_err(|error| EffigyRuntimeError::task_invocation(error.to_string()))
}

/// The labelled identity of each named volume on one endpoint. A listing or
/// inspection failure is an error, never an absence (R3).
pub fn volume_identities<F>(
    cwd: &Path,
    endpoint: &str,
    project_name: &str,
    names: &[String],
    run_runtime_volume_capture: &F,
) -> Result<BTreeMap<String, VolumeIdentity>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    if names.is_empty() {
        return Ok(BTreeMap::new());
    }
    let listed = run_runtime_volume_capture(cwd, endpoint, &list_volumes_command(project_name))?;
    let listed = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .collect::<BTreeSet<_>>();
    let present = names
        .iter()
        .filter(|name| listed.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    let metadata = if present.is_empty() {
        Vec::new()
    } else {
        let output = run_runtime_volume_capture(cwd, endpoint, &inspect_volumes_command(&present))?;
        parse_inspect_volume_metadata_list_strict(
            String::from_utf8_lossy(&output.stdout).as_ref(),
            &present,
        )
        .map_err(EffigyRuntimeError::task_invocation)?
    };
    Ok(names
        .iter()
        .map(|name| {
            let runtime = metadata.iter().find(|entry| &entry.name == name);
            (
                name.clone(),
                verify_volume_identity(project_name, name, runtime),
            )
        })
        .collect())
}

pub(super) fn hydrate_managed_volumes<F>(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    runtime_running: bool,
    run_runtime_volume_capture: &F,
) -> Result<Vec<ManagedVolume>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let mut volumes = policy.managed_volumes.clone();
    volumes.sort_by(|left, right| left.name.cmp(&right.name));
    if !runtime_running || volumes.is_empty() {
        return Ok(volumes);
    }

    let endpoint = policy_endpoint(repo_root, policy)?;
    let listed = run_runtime_volume_capture(
        repo_root,
        &endpoint,
        &list_volumes_command(&policy.project_name),
    )?;
    let listed_names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .collect::<BTreeSet<_>>();

    let inspect_names = volumes
        .iter()
        .filter(|volume| listed_names.contains(&volume.name))
        .map(|volume| volume.name.clone())
        .collect::<Vec<_>>();
    let runtime = inspect_runtime_volume_metadata_batch(
        repo_root,
        &endpoint,
        &inspect_names,
        run_runtime_volume_capture,
    )?;

    Ok(merge_runtime_volume_metadata(&volumes, &runtime))
}

pub(super) fn inspect_runtime_volume_metadata_batch<F>(
    cwd: &Path,
    profile: &str,
    names: &[String],
    run_runtime_volume_capture: &F,
) -> Result<Vec<RuntimeVolumeMetadata>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let Ok(output) = run_runtime_volume_capture(cwd, profile, &inspect_volumes_command(names))
    else {
        return Ok(Vec::new());
    };
    Ok(parse_inspect_volume_metadata_list(
        String::from_utf8_lossy(&output.stdout).as_ref(),
    ))
}
