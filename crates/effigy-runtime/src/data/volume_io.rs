use std::collections::BTreeSet;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    export_volume_command, import_volume_command, inspect_volume_command, list_volumes_command,
    merge_runtime_volume_metadata, parse_inspect_volume_metadata, parse_listed_volume_names,
    DockerCommand, ManagedVolume, RuntimeVolumeMetadata,
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
    let command = match action {
        ContainerDataTransferAction::Export => export_volume_command(volume_name, archive_path),
        ContainerDataTransferAction::Import => import_volume_command(volume_name, archive_path),
    };
    run_runtime_volume_capture(repo_root, &policy.profile, &command)?;
    Ok(())
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

    let listed = run_runtime_volume_capture(
        repo_root,
        &policy.profile,
        &list_volumes_command(&policy.project_name),
    )?;
    let listed_names = parse_listed_volume_names(String::from_utf8_lossy(&listed.stdout).as_ref())
        .into_iter()
        .collect::<BTreeSet<_>>();

    let mut runtime = Vec::new();
    for volume in &volumes {
        if !listed_names.contains(&volume.name) {
            continue;
        }
        let output = run_runtime_volume_capture(
            repo_root,
            &policy.profile,
            &inspect_volume_command(&volume.name),
        )?;
        if let Some(metadata) =
            parse_inspect_volume_metadata(String::from_utf8_lossy(&output.stdout).as_ref())
        {
            runtime.push(metadata);
        }
    }

    Ok(merge_runtime_volume_metadata(&volumes, &runtime))
}

pub(super) fn inspect_runtime_volume_metadata<F>(
    cwd: &Path,
    profile: &str,
    name: &str,
    run_runtime_volume_capture: &F,
) -> Result<Option<RuntimeVolumeMetadata>, EffigyRuntimeError>
where
    F: Fn(&Path, &str, &DockerCommand) -> Result<Output, EffigyRuntimeError>,
{
    let Some(metadata) = run_runtime_volume_capture(cwd, profile, &inspect_volume_command(name))
        .ok()
        .and_then(|output| {
            parse_inspect_volume_metadata(String::from_utf8_lossy(&output.stdout).as_ref())
        })
    else {
        return Ok(None);
    };
    Ok(Some(metadata))
}
