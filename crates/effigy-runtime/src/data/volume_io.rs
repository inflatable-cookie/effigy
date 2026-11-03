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

/// Complete, unambiguous runtime metadata for named volumes on one endpoint.
/// A spawn failure is an error, never an absence, and the strict parse refuses
/// malformed, partial or duplicate rows so missing evidence can never read as
/// absent metadata (contract 005: an inspection failure is an error).
pub(super) fn inspect_runtime_volume_metadata_batch_strict<F>(
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
    let output = run_runtime_volume_capture(cwd, profile, &inspect_volumes_command(names))?;
    let metadata = parse_inspect_volume_metadata_list_strict(
        String::from_utf8_lossy(&output.stdout).as_ref(),
        names,
    )
    .map_err(EffigyRuntimeError::task_invocation)?;
    let mut seen = BTreeSet::new();
    for entry in &metadata {
        if !seen.insert(entry.name.as_str()) {
            return Err(EffigyRuntimeError::task_invocation(format!(
                "volume inspect returned duplicate entries for `{}`",
                entry.name
            )));
        }
    }
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::process::Output;
    use std::sync::{Arc, Mutex};

    use super::{inspect_runtime_volume_metadata_batch_strict, volume_identities};
    use crate::EffigyRuntimeError;
    use effigy_catalog::volumes::{DockerCommand, VolumeIdentity};

    fn command_output(stdout: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    /// Listing decides presence; only present names are inspected, and the
    /// strict runtime labels decide Owned versus Lookalike.
    #[test]
    fn volume_identities_classifies_listed_and_absent_names_from_runtime_labels() {
        let calls = Arc::new(Mutex::new(Vec::<String>::new()));
        let calls_for_capture = Arc::clone(&calls);
        let capture = move |_cwd: &Path, _endpoint: &str, command: &DockerCommand| {
            let rendered = command.args.join(" ");
            calls_for_capture
                .lock()
                .expect("calls lock")
                .push(rendered.clone());
            let output = if rendered.contains("volume ls") {
                "web-project-demo-data\tdriver\t\n".to_owned()
            } else if rendered.contains("volume inspect") {
                "[{\"Name\":\"web-project-demo-data\",\"Mountpoint\":\"/var/lib/demo\",\"UsageData\":{\"Size\":2048},\"Labels\":{\"com.effigy.managed\":\"true\",\"com.effigy.project\":\"web-project\",\"com.effigy.volume-name\":\"web-project-demo-data\"}}]".to_owned()
            } else {
                String::new()
            };
            Ok(command_output(&output))
        };

        // An empty request must not touch the runtime at all.
        let empty = volume_identities(
            Path::new("/tmp/repo"),
            "endpoint",
            "web-project",
            &[],
            &capture,
        )
        .expect("empty request");
        assert!(empty.is_empty());
        assert!(calls.lock().expect("calls lock").is_empty());

        let identities = volume_identities(
            Path::new("/tmp/repo"),
            "endpoint",
            "web-project",
            &[
                "web-project-demo-data".to_owned(),
                "web-project-absent".to_owned(),
            ],
            &capture,
        )
        .expect("identities");

        assert_eq!(
            identities.get("web-project-demo-data"),
            Some(&VolumeIdentity::Owned)
        );
        assert_eq!(
            identities.get("web-project-absent"),
            Some(&VolumeIdentity::Absent)
        );
        // Exactly one listing and one inspect; the absent name triggers no extra probe.
        let calls = calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 2);
        assert!(calls.iter().any(|call| call.contains("volume ls")));
        assert!(calls.iter().any(|call| call.contains("volume inspect")));
    }

    fn inspect_output(stdout: &str) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn owned_entry(name: &str) -> String {
        format!(
            "{{\"Name\":\"{name}\",\"Mountpoint\":\"/var/lib/docker/volumes/{name}/_data\",\"Labels\":{{\"com.effigy.managed\":\"true\",\"com.effigy.project\":\"web-dev\",\"com.effigy.volume-name\":\"{name}\"}}}}"
        )
    }

    /// The strict batch records every requested name exactly once; spawn
    /// failures, omitted rows and duplicates are errors, never absence.
    #[test]
    fn strict_metadata_batch_refuses_incomplete_and_ambiguous_inspection() {
        let names = vec!["web-dev-a".to_owned(), "web-dev-b".to_owned()];

        // Complete inspection succeeds.
        let complete = inspect_runtime_volume_metadata_batch_strict(
            Path::new("."),
            "endpoint",
            &names,
            &|_cwd: &Path, _endpoint: &str, _command: &DockerCommand| {
                Ok(inspect_output(&format!(
                    "[{},{}]",
                    owned_entry("web-dev-a"),
                    owned_entry("web-dev-b")
                )))
            },
        )
        .expect("complete inspection");
        assert_eq!(complete.len(), 2);

        // A spawn failure propagates.
        let spawn_error = inspect_runtime_volume_metadata_batch_strict(
            Path::new("."),
            "endpoint",
            &names,
            &|_cwd: &Path, _endpoint: &str, _command: &DockerCommand| {
                Err(EffigyRuntimeError::task_invocation("volume inspect failed (1)"))
            },
        )
        .expect_err("spawn failure");
        assert!(spawn_error.to_string().contains("volume inspect failed"));

        // An omitted row is an error naming the missing volume.
        let partial = inspect_runtime_volume_metadata_batch_strict(
            Path::new("."),
            "endpoint",
            &names,
            &|_cwd: &Path, _endpoint: &str, _command: &DockerCommand| {
                Ok(inspect_output(&format!("[{}]", owned_entry("web-dev-a"))))
            },
        )
        .expect_err("partial inspection");
        assert!(partial.to_string().contains("web-dev-b"), "{partial}");

        // Duplicate rows for one name are ambiguous evidence.
        let duplicate = inspect_runtime_volume_metadata_batch_strict(
            Path::new("."),
            "endpoint",
            &names,
            &|_cwd: &Path, _endpoint: &str, _command: &DockerCommand| {
                Ok(inspect_output(&format!(
                    "[{},{},{}]",
                    owned_entry("web-dev-a"),
                    owned_entry("web-dev-a"),
                    owned_entry("web-dev-b")
                )))
            },
        )
        .expect_err("duplicate rows");
        assert!(duplicate.to_string().contains("duplicate entries"), "{duplicate}");

        // Malformed JSON is an error.
        let malformed = inspect_runtime_volume_metadata_batch_strict(
            Path::new("."),
            "endpoint",
            &names,
            &|_cwd: &Path, _endpoint: &str, _command: &DockerCommand| {
                Ok(inspect_output("not json"))
            },
        )
        .expect_err("malformed output");
        assert!(malformed.to_string().contains("JSON"), "{malformed}");

        // An empty request never touches the runtime.
        let empty = inspect_runtime_volume_metadata_batch_strict(
            Path::new("."),
            "endpoint",
            &[],
            &|_cwd: &Path, _endpoint: &str, _command: &DockerCommand| {
                Err(EffigyRuntimeError::task_invocation("must not spawn"))
            },
        )
        .expect("empty request");
        assert!(empty.is_empty());
    }
}
