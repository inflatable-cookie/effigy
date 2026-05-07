use std::collections::BTreeSet;
use std::path::Path;
use std::process::Output;

use effigy_catalog::volumes::{
    export_volume_command, import_volume_command, inspect_volumes_command, list_volumes_command,
    merge_runtime_volume_metadata, parse_inspect_volume_metadata_list, parse_listed_volume_names,
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

    let inspect_names = volumes
        .iter()
        .filter(|volume| listed_names.contains(&volume.name))
        .map(|volume| volume.name.clone())
        .collect::<Vec<_>>();
    let runtime = inspect_runtime_volume_metadata_batch(
        repo_root,
        &policy.profile,
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
