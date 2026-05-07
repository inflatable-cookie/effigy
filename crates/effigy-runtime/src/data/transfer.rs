use std::path::Path;

use effigy_catalog::volumes::ManagedVolume;
use effigy_containers::{
    ContainerDataTransferAction, EffectiveComposeSource, EffectiveContainerPolicy,
};

use crate::EffigyRuntimeError;

pub(super) fn ensure_generated_data_path(
    policy: &EffectiveContainerPolicy,
    action: &str,
) -> Result<(), EffigyRuntimeError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Err(EffigyRuntimeError::task_invocation(format!(
            "container `{}` uses direct `compose_file` ownership; `data {action}` is supported only on the generated-compose path",
            policy.name
        )));
    }
    Ok(())
}

pub(super) fn resolve_managed_volume(
    policy: &EffectiveContainerPolicy,
    volume_name: &str,
) -> Result<ManagedVolume, EffigyRuntimeError> {
    let Some(volume) = policy
        .managed_volumes
        .iter()
        .find(|volume| volume.name == volume_name)
        .cloned()
    else {
        let available = policy
            .managed_volumes
            .iter()
            .map(|volume| format!("`{}`", volume.name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(EffigyRuntimeError::task_invocation(format!(
            "managed volume `{volume_name}` is not owned by container `{}` (available: {available})",
            policy.name
        )));
    };
    Ok(volume)
}

pub(super) fn validate_transfer_path(
    archive_path: &Path,
    action: ContainerDataTransferAction,
) -> Result<(), EffigyRuntimeError> {
    match action {
        ContainerDataTransferAction::Export => {
            let parent = archive_path.parent().unwrap_or(Path::new("."));
            if !parent.is_dir() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "export path parent directory does not exist: {}",
                    parent.display()
                )));
            }
        }
        ContainerDataTransferAction::Import => {
            if !archive_path.is_file() {
                return Err(EffigyRuntimeError::task_invocation(format!(
                    "import archive not found: {}",
                    archive_path.display()
                )));
            }
        }
    }
    Ok(())
}
