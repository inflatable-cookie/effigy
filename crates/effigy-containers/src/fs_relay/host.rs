//! One environment's relay on this host: observe the running mounts, hold the
//! environment's relay lock, run the engine and keep its record.

use super::{
    detect::{classify_backend, Backend},
    relay::FsEventRelayHandle,
    state::{
        fs_relay_dir, fs_relay_record_path, lock_path, stream_state_dir, write_record, RelayOwner,
        RelayRecord, RelayRecordTarget, FS_RELAY_RECORD_SCHEMA,
    },
    targets::{resolve_targets, ServiceMount},
};
use crate::{
    compose::{resolve_compose_backend_for_repo, ComposeBackend},
    exec::{
        inspect_containers_for_policy, list_running_compose_containers_for_policy,
        run_runtime_command_capture_for_policy_with_timeout,
        verify_owned_running_compose_identities,
    },
    BackendId, ContainerBackendDetection, ContainerManager, EffectiveContainerPolicy,
};
use nix::fcntl::{Flock, FlockArg};
use std::{
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

type RelayError = Box<dyn std::error::Error + Send + Sync>;

/// The budget for reading the running containers and their mounts.
const OBSERVE_TIMEOUT: Duration = Duration::from_secs(30);

/// A held relay. Dropping it stops the watchers and guest streams, removes its
/// record and releases the environment's relay lock.
pub struct FsRelaySession {
    handle: Option<FsEventRelayHandle>,
    repo_root: PathBuf,
    container: String,
    targets: usize,
    _lock: Flock<File>,
}

impl FsRelaySession {
    /// How many mount roots are relayed; zero means nothing writable is mounted.
    pub fn target_count(&self) -> usize {
        self.targets
    }
}

impl Drop for FsRelaySession {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.shutdown();
        }
        let _ = fs::remove_file(fs_relay_record_path(&self.repo_root, &self.container));
        let _ = fs::remove_dir_all(stream_state_dir(&self.repo_root, &self.container));
    }
}

pub enum FsRelayStart {
    Started(FsRelaySession),
    /// Another process already relays into this environment.
    AlreadyRunning,
}

/// Start the relay for one running environment. At most one process holds it.
pub fn start_fs_relay(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    owner: RelayOwner,
    reason: &str,
) -> Result<FsRelayStart, RelayError> {
    fs::create_dir_all(fs_relay_dir(repo_root))?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path(repo_root, &policy.name))?;
    let lock = match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
        Ok(lock) => lock,
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => return Ok(FsRelayStart::AlreadyRunning),
        Err((_, error)) => return Err(error.into()),
    };

    let mounts = observe_service_mounts(repo_root, policy)?;
    let working_dir = workspace_destination(repo_root, policy, &mounts);
    // Every supported VM backend runs one guest kernel per profile, so one
    // container's touch is seen by every sharer of the mount.
    let targets = resolve_targets(policy, &working_dir, &mounts, true)?;
    // The session owns the record from here, so a failed start cleans it up.
    let mut session = FsRelaySession {
        handle: None,
        repo_root: repo_root.to_path_buf(),
        container: policy.name.clone(),
        targets: targets.len(),
        _lock: lock,
    };
    let (state, reason) = if targets.is_empty() {
        ("inactive", "no_writable_mounts")
    } else {
        ("active", reason)
    };
    let record = RelayRecord {
        schema: FS_RELAY_RECORD_SCHEMA.to_owned(),
        container: policy.name.clone(),
        owner,
        pid: std::process::id(),
        state: state.to_owned(),
        reason: reason.to_owned(),
        targets: targets
            .iter()
            .map(|target| RelayRecordTarget {
                service: target.service.clone(),
                container: target.container.clone(),
                host: target.host.clone(),
                guest: target.guest.clone(),
            })
            .collect(),
    };
    if !targets.is_empty() {
        session.handle = Some(FsEventRelayHandle::start(
            &ContainerManager::defaults(),
            &runtime_detection(repo_root, policy),
            &policy.profile,
            "docker",
            targets,
            &policy.fs_events,
            &stream_state_dir(repo_root, &policy.name),
        )?);
    }
    write_record(repo_root, &record)?;
    Ok(FsRelayStart::Started(session))
}

fn runtime_detection(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> ContainerBackendDetection {
    let mut detection = ContainerBackendDetection::from_env_and_path();
    if detection.backend_override.is_none() {
        detection.backend_override =
            Some(match resolve_compose_backend_for_repo(repo_root, policy) {
                ComposeBackend::Docker => BackendId::docker_compose(),
                ComposeBackend::ColimaNerdctl => BackendId::colima_nerdctl(),
            });
    }
    detection
}

/// The bind mounts of this checkout's running service containers, as the
/// runtime reports them. Ownership is verified before anything is exec'd into.
fn observe_service_mounts(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<ServiceMount>, RelayError> {
    let deadline = Instant::now() + OBSERVE_TIMEOUT;
    let rows = list_running_compose_containers_for_policy(repo_root, policy)?
        .into_iter()
        .filter(|row| !row.oneoff && row.status.is_running() && row.service.is_some())
        .collect::<Vec<_>>();
    let bindings = verify_owned_running_compose_identities(repo_root, policy, &rows, deadline)?;
    let mut mounts = Vec::new();
    for binding in bindings {
        let Some(service) = binding.service() else {
            continue;
        };
        let output = inspect_containers_for_policy(
            repo_root,
            policy,
            &[binding.id().to_owned()],
            Some(deadline),
        )?;
        mounts.extend(parse_bind_mounts(service, binding.id(), &output.stdout)?);
    }
    Ok(mounts)
}

fn parse_bind_mounts(
    service: &str,
    container: &str,
    inspect: &[u8],
) -> Result<Vec<ServiceMount>, RelayError> {
    let records: serde_json::Value = serde_json::from_slice(inspect)?;
    let Some(record) = records.as_array().and_then(|records| records.first()) else {
        return Err("runtime inspect returned no container".into());
    };
    Ok(record["Mounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|mount| mount["Type"] == "bind")
        .filter_map(|mount| {
            Some(ServiceMount {
                service: service.to_owned(),
                container: container.to_owned(),
                host: host_source(mount["Source"].as_str()?),
                guest: PathBuf::from(mount["Destination"].as_str()?),
                writable: mount["RW"].as_bool().unwrap_or(false),
            })
        })
        .collect())
}

/// Docker Desktop reports a bind source under its VM's `/host_mnt` prefix.
fn host_source(source: &str) -> PathBuf {
    let path = PathBuf::from(source);
    match source.strip_prefix("/host_mnt") {
        Some(host) if !path.exists() && Path::new(host).exists() => PathBuf::from(host),
        _ => path,
    }
}

/// Where the checkout root is mounted in the guest. A declared working
/// directory may sit below that mount, or another service may mount the
/// checkout elsewhere, so the observed destination decides.
fn workspace_destination(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    mounts: &[ServiceMount],
) -> PathBuf {
    let working_dir = crate::load_container_exec_working_dir(repo_root, Some(&policy.name)).ok();
    let root = repo_root.canonicalize().ok();
    mounts
        .iter()
        .filter(|mount| mount.writable && root.is_some() && mount.host.canonicalize().ok() == root)
        .min_by_key(|mount| {
            (
                !working_dir
                    .as_deref()
                    .is_some_and(|dir| dir.starts_with(&mount.guest)),
                mount.service != policy.primary_service,
            )
        })
        .map(|mount| mount.guest.clone())
        .or(working_dir)
        .unwrap_or_else(|| repo_root.to_path_buf())
}

/// Observe whether this environment's containers already receive host file
/// events. Only a Docker endpoint can; Colima's containerd never does.
pub fn observe_fs_relay_backend(repo_root: &Path, policy: &EffectiveContainerPolicy) -> Backend {
    if resolve_compose_backend_for_repo(repo_root, policy) != ComposeBackend::Docker {
        return Backend::Other;
    }
    let read = |args: &[&str]| {
        let args = args.iter().map(OsString::from).collect::<Vec<_>>();
        run_runtime_command_capture_for_policy_with_timeout(
            repo_root,
            policy,
            &args,
            "runtime backend probe",
            Duration::from_secs(10),
        )
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|value| !value.is_empty())
    };
    let endpoint = std::env::var("DOCKER_HOST")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| read(&["context", "show"]));
    let operating_system = read(&["info", "--format", "{{.OperatingSystem}}"]);
    // A Linux Docker CLI can still point at a VM: Docker Desktop or Colima.
    let native_engine = operating_system
        .as_deref()
        .is_some_and(|name| !name.eq_ignore_ascii_case("Docker Desktop"))
        && !endpoint
            .as_deref()
            .is_some_and(|value| value.contains("colima"));
    classify_backend(
        cfg!(target_os = "linux"),
        native_engine,
        endpoint.as_deref(),
        operating_system.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_mounts_come_from_inspect_and_the_checkout_mount_names_the_workspace() {
        let dir = tempfile::tempdir().expect("fixture");
        let root = dir.path().canonicalize().expect("root");
        let inspect = serde_json::json!([{ "Mounts": [
            { "Type": "bind", "Source": root, "Destination": "/srv/app", "RW": true },
            { "Type": "bind", "Source": "/etc/hosts", "Destination": "/etc/hosts", "RW": false },
            { "Type": "volume", "Source": "/var/lib/volume", "Destination": "/data", "RW": true },
        ]}]);
        let mounts =
            parse_bind_mounts("app", "abc", inspect.to_string().as_bytes()).expect("parse");
        assert_eq!(mounts.len(), 2);
        assert!(mounts[0].writable && !mounts[1].writable);
        let policy = crate::policy::model::test_policy(&root);
        assert_eq!(
            workspace_destination(&root, &policy, &mounts),
            PathBuf::from("/srv/app")
        );
        assert!(parse_bind_mounts("app", "abc", b"[]").is_err());
    }
}
