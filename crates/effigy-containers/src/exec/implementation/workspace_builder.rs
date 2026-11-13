//! Workspace artifact builder verification and recovery.

use super::*;

/// Refuse a builder declaration that could activate any additional service.
pub fn verify_workspace_artifact_builder_declaration(
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerExecError> {
    for file in &policy.compose_files {
        let content = policy.read_compose(file).map_err(|error| {
            ContainerExecError::refused("read workspace builder declaration", error.to_string())
        })?;
        let compose: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerExecError::refused("parse workspace builder declaration", error.to_string())
        })?;
        let builder = compose
            .get("services")
            .and_then(|services| services.get("builder"));
        if compose.get("include").is_some()
            || builder.is_some_and(|builder| {
                builder.get("extends").is_some()
                    || builder.get("depends_on").is_some()
                    || builder.get("ports").is_some()
            })
        {
            return Err(ContainerExecError::refused(
                "verify workspace builder declaration",
                "workspace artifact builder cannot include, extend, depend on or publish other workloads",
            ));
        }
    }
    let services = declared_service_names(policy, &policy.compose_files)?;
    if services.len() != 1 || !services.contains("builder") {
        return Err(ContainerExecError::refused(
            "verify workspace builder declaration",
            "workspace artifact builder must declare only the builder service",
        ));
    }
    Ok(())
}

/// Strict builder execution authority: explicit checkout/scope labels, a full
/// immutable identity and a read-only source bind at /workspace. Unlike stack
/// reactivation this never adopts legacy unlabelled containers.
pub fn verify_workspace_artifact_builder(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<VerifiedComposeContainer, ContainerExecError> {
    let inspected = inspect_workspace_artifact_builder(repo_root, policy, row, deadline)?;
    match builder_preparation(Some(inspected), false, false)? {
        WorkspaceBuilderPreparation::Reuse(binding) => Ok(*binding),
        WorkspaceBuilderPreparation::Activate(_) => Err(ContainerExecError::refused(
            "inspect workspace builder",
            "workspace builder requires preparation; no execution binding granted",
        )),
    }
}

/// An owned builder's immutable identity and source, rechecked before recovery.
/// It grants no execution or mutation authority by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWorkspaceArtifactBuilder {
    pub id: String,
    pub source: PathBuf,
    pub never_started: bool,
    pub read_only: bool,
    pub mounts_owned: bool,
    pub remount_capability_dropped: bool,
}

/// One preparation step. Activation still goes through the maintained lifecycle
/// and rechecks the exact observed target immediately before it mutates anything.
#[derive(Debug)]
pub enum WorkspaceBuilderPreparation {
    Reuse(Box<VerifiedComposeContainer>),
    Activate(Option<ObservedWorkspaceArtifactBuilder>),
}

pub fn workspace_artifact_builder_preparation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    rows: &[RunningComposeContainer],
    deadline: Instant,
    retained: bool,
    no_change: bool,
) -> Result<WorkspaceBuilderPreparation, ContainerExecError> {
    let inspected = match rows {
        [] => None,
        [row] => {
            Some(inspect_workspace_artifact_builder(repo_root, policy, row, deadline)?)
        }
        _ => return Err(ContainerExecError::refused(
            "inspect workspace builder",
            "workspace artifact builder inventory must contain exactly one owned builder; no build attempted",
        )),
    };
    builder_preparation(inspected, retained, no_change)
}

/// Choose from already-decoded facts; this function performs no runtime effects.
fn builder_preparation(
    inspected: Option<InspectedWorkspaceBuilder>,
    retained: bool,
    no_change: bool,
) -> Result<WorkspaceBuilderPreparation, ContainerExecError> {
    let refuse = |message| ContainerExecError::refused("inspect workspace builder", message);
    let Some(inspected) = inspected else {
        if retained {
            return Err(refuse("workspace artifact builder record has no matching container; retained state was not removed or adopted"));
        }
        if no_change {
            return Err(refuse("no-change workspace artifact build refuses to create the builder container; the profile must already hold it"));
        }
        return Ok(WorkspaceBuilderPreparation::Activate(None));
    };
    match inspected.state {
        WorkspaceBuilderState::Stopped => {
            return Err(refuse(
                "workspace builder is stopped; no running execution binding",
            ))
        }
        WorkspaceBuilderState::Unknown => {
            return Err(refuse(
                "workspace builder state is unknown; no running execution binding",
            ))
        }
        WorkspaceBuilderState::Running | WorkspaceBuilderState::NeverStarted => {}
    }
    let observed = inspected.observed;
    if !observed.never_started && inspected.matching {
        return Ok(WorkspaceBuilderPreparation::Reuse(Box::new(
            inspected.binding,
        )));
    }
    if !inspected.source_owned {
        return Err(refuse("workspace builder source bind is not this checkout's own; its source bind is not adopted"));
    }
    // A matching never-started container used to fail the strict running check
    // and was refused. Preserve that boundary instead of granting start authority.
    if observed.never_started && inspected.matching {
        return Err(refuse(
            "workspace builder is never-started; no running execution binding",
        ));
    }
    if no_change {
        return Err(refuse("no-change workspace artifact build refuses an old, writable or mismatched owned builder before claim; it cannot create, replace or reconfigure it"));
    }
    Ok(WorkspaceBuilderPreparation::Activate(Some(observed)))
}

pub fn observed_workspace_artifact_builder(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<ObservedWorkspaceArtifactBuilder, ContainerExecError> {
    let inspected = inspect_workspace_artifact_builder(repo_root, policy, row, deadline)?;
    if matches!(
        inspected.state,
        WorkspaceBuilderState::Stopped | WorkspaceBuilderState::Unknown
    ) {
        return Err(ContainerExecError::refused(
            "inspect workspace builder",
            "workspace builder is not running or never-started",
        ));
    }
    Ok(inspected.observed)
}

const CHECKOUT_GENERATION_LABEL: &str = "com.effigy.checkout-generation";
const CHECKOUT_FENCE_LABEL: &str = "com.effigy.checkout-fence";
const BUILDER_CACHE_VOLUMES: [(&str, &str); 2] = [
    ("/root/.cargo/registry", "cargo-registry"),
    ("/root/.cargo/git", "cargo-git"),
];

/// Exactly the builder's two cargo caches, each the named volume of this
/// project, read-write, with nothing else mounted beside the `/workspace` bind.
fn builder_cache_volumes_are_owned(
    mounts: Option<&Vec<serde_json::Value>>,
    project_name: &str,
) -> bool {
    let Some(mounts) = mounts else { return false };
    mounts.len() == BUILDER_CACHE_VOLUMES.len() + 1
        && BUILDER_CACHE_VOLUMES.iter().all(|(destination, suffix)| {
            let name = format!("{project_name}-{suffix}");
            mounts.iter().any(|mount| {
                mount.get("Type").and_then(|value| value.as_str()) == Some("volume")
                    && mount.get("Destination").and_then(|value| value.as_str())
                        == Some(destination)
                    && mount.get("Name").and_then(|value| value.as_str()) == Some(name.as_str())
                    && mount.get("RW").and_then(|value| value.as_bool()) == Some(true)
            })
        })
}

/// Whether the compatibility record is unprivileged and adds nothing back.
/// A missing HostConfig is not proof of either.
fn host_is_unprivileged_without_add_back(record: Option<&serde_json::Value>) -> bool {
    let Some(host) = record.and_then(|record| record.get("HostConfig")) else {
        return false;
    };
    if host.get("Privileged").and_then(|value| value.as_bool()) != Some(false) {
        return false;
    }
    match host.get("CapAdd") {
        None | Some(serde_json::Value::Null) => true,
        Some(serde_json::Value::Array(caps)) => caps.is_empty(),
        _ => false,
    }
}

/// Docker's witness: HostConfig CapDrop names SYS_ADMIN, nothing is added back,
/// and the container is not privileged. Docker lists the names it was asked to
/// drop, so a missing HostConfig is not proof of the drop.
///
/// nerdctl's compatibility inspect cannot report this: it rebuilds CapDrop from
/// the default capability set only, and SYS_ADMIN is not in that set. Colima
/// nerdctl therefore uses [`remount_capability_witness`] instead.
fn builder_remount_capability_dropped(record: Option<&serde_json::Value>) -> bool {
    if !host_is_unprivileged_without_add_back(record) {
        return false;
    }
    record
        .and_then(|record| record.pointer("/HostConfig/CapDrop"))
        .and_then(|value| value.as_array())
        .is_some_and(|caps| {
            caps.iter().any(|cap| {
                cap.as_str().is_some_and(|name| {
                    name.eq_ignore_ascii_case("SYS_ADMIN")
                        || name.eq_ignore_ascii_case("CAP_SYS_ADMIN")
                })
            })
        })
}

/// Whether the exact owned builder proves the remount capability is gone on the
/// backend that runs it.
///
/// On Colima nerdctl the witness is the stored OCI spec of the same full ID. Its
/// process capability sets are what the builder runs with, and `nerdctl exec`
/// copies that stored `Process` (only `--privileged` rewrites the sets), so the
/// restriction the exec path inherits is the one observed here. Compose
/// `cap_drop` reaches the spec through `WithDroppedCapabilities`, which removes
/// the name from bounding, effective, permitted and inheritable.
///
/// A missing, malformed, or other-identity native record is an error. A complete
/// record that holds any process capability beyond containerd's default set
/// (CAP_SYS_ADMIN or other power) is `Ok(false)`.
fn remount_capability_witness(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    id: &str,
    record: Option<&serde_json::Value>,
    deadline: Instant,
) -> Result<bool, ContainerExecError> {
    const CONTEXT: &str = "inspect workspace builder";
    if !matches!(
        crate::compose::resolve_compose_backend_for_repo(repo_root, policy),
        crate::compose::ComposeBackend::ColimaNerdctl
    ) {
        return Ok(builder_remount_capability_dropped(record));
    }
    let remaining = crate::exec::process::remaining_until_deadline(deadline, CONTEXT)?;
    let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &inspect_native_spec_args(id),
        CONTEXT,
        deadline,
        remaining,
    )?;
    native_capability_absent(record, &output.stdout, id)
        .map_err(|reason| ContainerExecError::refused(CONTEXT, reason))
}

/// nerdctl builders run with containerd's default unix capabilities
/// (`defaultUnixCaps` in containerd v2.3.1 `pkg/oci/spec.go`, sha256
/// 7029327e775cf5bb1bfc632b45ecfdb63d7872201b973edf27e88866f907276c). Bounding,
/// effective and permitted default to this set; inheritable and ambient default
/// to empty. Any process capability beyond it is power added back.
const NERDCTL_DEFAULT_CAPS: [&str; 14] = [
    "CAP_CHOWN",
    "CAP_DAC_OVERRIDE",
    "CAP_FSETID",
    "CAP_FOWNER",
    "CAP_MKNOD",
    "CAP_NET_RAW",
    "CAP_SETGID",
    "CAP_SETUID",
    "CAP_SETFCAP",
    "CAP_SETPCAP",
    "CAP_NET_BIND_SERVICE",
    "CAP_SYS_CHROOT",
    "CAP_KILL",
    "CAP_AUDIT_WRITE",
];

/// Every capability name the kernel's uapi header defines
/// (`include/uapi/linux/capability.h`, v6.12, sha256
/// ee832adca0d6db0866ddbf1dd3426c5a0df63901bf255bde77e4d8305cf5b5e0). A name
/// outside this list is unknown capability data, not a harmless extra.
const LINUX_CAPABILITY_NAMES: [&str; 41] = [
    "CAP_CHOWN",
    "CAP_DAC_OVERRIDE",
    "CAP_DAC_READ_SEARCH",
    "CAP_FOWNER",
    "CAP_FSETID",
    "CAP_KILL",
    "CAP_SETGID",
    "CAP_SETUID",
    "CAP_SETPCAP",
    "CAP_LINUX_IMMUTABLE",
    "CAP_NET_BIND_SERVICE",
    "CAP_NET_BROADCAST",
    "CAP_NET_ADMIN",
    "CAP_NET_RAW",
    "CAP_IPC_LOCK",
    "CAP_IPC_OWNER",
    "CAP_SYS_MODULE",
    "CAP_SYS_RAWIO",
    "CAP_SYS_CHROOT",
    "CAP_SYS_PTRACE",
    "CAP_SYS_PACCT",
    "CAP_SYS_ADMIN",
    "CAP_SYS_BOOT",
    "CAP_SYS_NICE",
    "CAP_SYS_RESOURCE",
    "CAP_SYS_TIME",
    "CAP_SYS_TTY_CONFIG",
    "CAP_MKNOD",
    "CAP_LEASE",
    "CAP_AUDIT_WRITE",
    "CAP_AUDIT_CONTROL",
    "CAP_SETFCAP",
    "CAP_MAC_OVERRIDE",
    "CAP_MAC_ADMIN",
    "CAP_SYSLOG",
    "CAP_WAKE_ALARM",
    "CAP_BLOCK_SUSPEND",
    "CAP_AUDIT_READ",
    "CAP_PERFMON",
    "CAP_BPF",
    "CAP_CHECKPOINT_RESTORE",
];

/// Decide the nerdctl witness from one native inspect of the full `id`.
///
/// OCI omits an empty capability set (`omitempty`), so an absent set reads as
/// empty. An absent `bounding` set, a missing `process` or `capabilities`
/// object, a non-list set or any entry that is not an upper-case `CAP_` name is
/// malformed. The compatibility record must still be unprivileged with nothing
/// added back.
fn native_capability_absent(
    record: Option<&serde_json::Value>,
    native_output: &[u8],
    id: &str,
) -> Result<bool, String> {
    let raw: serde_json::Value = serde_json::from_slice(native_output)
        .map_err(|error| format!("native inspect output is not JSON: {error}"))?;
    let records = raw
        .as_array()
        .ok_or_else(|| "native inspect output is not a record list".to_owned())?;
    // One record for one ID: a second record is data this decision never saw.
    let [native] = records.as_slice() else {
        return Err(format!(
            "native inspect returned {} records; exactly one is required",
            records.len()
        ));
    };
    if native.get("ID").and_then(|value| value.as_str()) != Some(id) {
        return Err(
            "native inspect reports a different full container ID than the verified binding"
                .to_owned(),
        );
    }
    let capabilities = native
        .pointer("/Spec/process/capabilities")
        .and_then(|value| value.as_object())
        .ok_or_else(|| "native inspect has no Spec.process.capabilities".to_owned())?;
    const SETS: [&str; 5] = [
        "bounding",
        "effective",
        "inheritable",
        "permitted",
        "ambient",
    ];
    // OCI's LinuxCapabilities has exactly these five sets; any other key is
    // capability data this decision would otherwise silently ignore.
    if let Some(unknown) = capabilities
        .keys()
        .find(|key| !SETS.contains(&key.as_str()))
    {
        return Err(format!(
            "native capability data holds an unknown set `{unknown}`"
        ));
    }
    if !capabilities.contains_key("bounding") {
        return Err("native Spec.process.capabilities has no bounding set".to_owned());
    }
    let mut absent = true;
    for set in SETS {
        let Some(value) = capabilities.get(set) else {
            continue;
        };
        let names = value
            .as_array()
            .ok_or_else(|| format!("native capability set `{set}` is not a list"))?;
        for name in names {
            let name = name
                .as_str()
                .filter(|name| LINUX_CAPABILITY_NAMES.contains(name))
                .ok_or_else(|| format!("native capability set `{set}` holds an unknown entry"))?;
            absent &= NERDCTL_DEFAULT_CAPS.contains(&name);
        }
    }
    // This path reads the compatibility record strictly: a missing or malformed
    // field is not an unprivileged, add-back-free builder.
    let unprivileged_without_add_back = nerdctl_compat_add_back_witness(record)?;
    Ok(absent && unprivileged_without_add_back)
}

/// The nerdctl compatibility record must state `Privileged` as a boolean and
/// `CapAdd` as a list. Only `Privileged: false` with an empty `CapAdd` is
/// `Ok(true)`. A privileged builder or an added capability is a well-formed
/// `Ok(false)`, and a missing or malformed field is an error.
fn nerdctl_compat_add_back_witness(record: Option<&serde_json::Value>) -> Result<bool, String> {
    let host = record
        .and_then(|record| record.get("HostConfig"))
        .and_then(|value| value.as_object())
        .ok_or_else(|| "compatibility record has no HostConfig".to_owned())?;
    let privileged = host
        .get("Privileged")
        .and_then(|value| value.as_bool())
        .ok_or_else(|| {
            "compatibility HostConfig.Privileged is missing or not a boolean".to_owned()
        })?;
    let cap_add = host
        .get("CapAdd")
        .and_then(|value| value.as_array())
        .ok_or_else(|| "compatibility HostConfig.CapAdd is missing or not a list".to_owned())?;
    Ok(!privileged && cap_add.is_empty())
}

/// Native inspect of one full immutable ID. The compatibility record is only
/// the identity source; this read never follows a name.
fn inspect_native_spec_args(id: &str) -> [OsString; 3] {
    [
        OsString::from("inspect"),
        OsString::from("--mode=native"),
        OsString::from(id),
    ]
}

#[derive(Clone, Copy)]
enum WorkspaceBuilderState {
    NeverStarted,
    Stopped,
    Running,
    Unknown,
}

struct InspectedWorkspaceBuilder {
    state: WorkspaceBuilderState,
    binding: VerifiedComposeContainer,
    observed: ObservedWorkspaceArtifactBuilder,
    source_owned: bool,
    matching: bool,
}

/// Capture exactly once; the compatibility record supplies identity, mounts,
/// labels and state together. nerdctl's native capability witness is a separate
/// read of that same full ID, never a second compatibility observation.
fn inspect_workspace_artifact_builder(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    deadline: Instant,
) -> Result<InspectedWorkspaceBuilder, ContainerExecError> {
    let ownership = OwnershipContext::resolve(repo_root, policy)?;
    let remaining =
        crate::exec::process::remaining_until_deadline(deadline, "inspect workspace builder")?;
    let output = run_runtime_command_capture_for_policy_with_absolute_deadline(
        repo_root,
        policy,
        &inspect_owned_binding_args(row),
        "inspect workspace builder",
        deadline,
        remaining,
    )?;
    let raw = serde_json::from_slice(&output.stdout).map_err(|error| {
        ContainerExecError::refused(
            "inspect workspace builder",
            format!("inspect output is not JSON: {error}"),
        )
    })?;
    let backend = crate::compose::resolve_compose_backend_for_repo(repo_root, policy);
    let mut inspected = decode_workspace_builder(&ownership, policy, row, &raw, backend)?;
    inspected.observed.remount_capability_dropped = remount_capability_witness(
        repo_root,
        policy,
        inspected.binding.id(),
        raw.as_array().and_then(|records| records.first()),
        deadline,
    )?;
    inspected.matching &= inspected.observed.remount_capability_dropped;
    Ok(inspected)
}

fn decode_workspace_builder(
    ownership: &OwnershipContext,
    policy: &EffectiveContainerPolicy,
    row: &RunningComposeContainer,
    raw: &serde_json::Value,
    backend: crate::compose::ComposeBackend,
) -> Result<InspectedWorkspaceBuilder, ContainerExecError> {
    let refuse = |reason| ContainerExecError::refused("inspect workspace builder", reason);
    if row.service.as_deref() != Some("builder") || row.oneoff {
        return Err(refuse(
            "workspace artifact builder service identity is unverified".to_owned(),
        ));
    }
    let never_started = row.status.lifecycle == crate::exec::parse::ContainerLifecycle::Created;
    if never_started && !matches!(backend, crate::compose::ComposeBackend::ColimaNerdctl) {
        return Err(refuse(
            "the never-started shape is proven only for the Colima nerdctl backend".to_owned(),
        ));
    }
    let identity = crate::exec::parse::inspected_identity_from_value(raw, never_started)
        .map_err(|error| refuse(format!("binding unavailable: {error}")))?;
    let record = raw.as_array().and_then(|records| records.first());
    let labels = record.and_then(|record| record.pointer("/Config/Labels"));
    let (checkout, oneoff_label_ok) = compose_binding_labels(labels);
    let scope = labels
        .and_then(|labels| labels.get(crate::runtime::scope::SCOPE_LABEL))
        .map(|value| value.as_str().unwrap_or_default());
    let explicit_checkout = labels
        .and_then(|labels| labels.get("com.effigy.checkout"))
        .and_then(|value| value.as_str());
    if !ownership.inspected_identity_matches(policy, row, &identity, oneoff_label_ok, false)
        || !ownership.owns(checkout, scope)
        || !ownership.owns(explicit_checkout, scope)
        || (never_started && identity.name != format!("{}-builder-1", policy.project_name))
    {
        return Err(refuse(format!("binding unavailable: full-ID/project/service/oneoff/running/checkout/scope ownership does not match owned project `{}` at `{}`", policy.project_name, ownership.canonical.display())));
    }
    if never_started {
        use std::os::unix::fs::MetadataExt;
        let root = std::fs::metadata(&ownership.canonical)
            .map_err(|error| refuse(format!("the checkout is not readable: {error}")))?;
        let object_identity = |name: &str| {
            let value: serde_json::Value =
                serde_json::from_str(labels?.get(name)?.as_str()?).ok()?;
            Some((
                value.get("device")?.as_u64()?,
                value.get("inode")?.as_u64()?,
            ))
        };
        if object_identity(CHECKOUT_GENERATION_LABEL) != Some((root.dev(), root.ino())) {
            return Err(refuse(
                "binding unavailable: checkout generation is stale or unverified".to_owned(),
            ));
        }
        if object_identity(CHECKOUT_FENCE_LABEL).map(|(device, _)| device) != Some(root.dev()) {
            return Err(refuse(
                "binding unavailable: checkout fence is unverified".to_owned(),
            ));
        }
    }
    let mounts = record
        .and_then(|record| record.get("Mounts"))
        .and_then(|value| value.as_array());
    let (source, read_only) = workspace_builder_source_mount(mounts).ok_or_else(|| {
        refuse("workspace builder binds no single `/workspace` source".to_owned())
    })?;
    let mounts_owned = builder_cache_volumes_are_owned(mounts, &policy.project_name);
    let no_ports = workspace_builder_has_no_published_ports(record);
    let working_dir_matches = builder_working_directory_is_owned(
        &ownership.canonical,
        ownership.staged_render.as_ref(),
        record.and_then(|record| {
            record.pointer("/Config/Labels/com.docker.compose.project.working_dir")
        }),
    );
    if !no_ports
        || !working_dir_matches
        || (never_started && (!mounts_owned || source != ownership.canonical))
    {
        return Err(refuse("workspace builder source, cargo cache volumes, published ports or Compose working directory are unverified".to_owned()));
    }
    let state = if never_started {
        WorkspaceBuilderState::NeverStarted
    } else if identity.running && crate::exec::parse::compose_status_is_running(&identity.status) {
        WorkspaceBuilderState::Running
    } else if !identity.running
        && crate::exec::parse::ContainerStatus::from(identity.status.clone()).lifecycle
            == crate::exec::parse::ContainerLifecycle::Exited
    {
        WorkspaceBuilderState::Stopped
    } else {
        WorkspaceBuilderState::Unknown
    };
    let expected = expected_workspace_source(&ownership.canonical, policy)?;
    let matching = read_only && source == expected && mounts_owned;
    let source_owned = source == ownership.canonical || source == expected;
    let binding = VerifiedComposeContainer {
        row: row.clone(),
        id: identity.id.clone(),
        checkout: ownership.canonical.clone(),
        scope: ownership
            .scope_token
            .as_deref()
            .map(|scope| scope.trim().to_owned()),
        profile: policy.profile.clone(),
        backend,
        workspace_artifact_builder: true,
    };
    Ok(InspectedWorkspaceBuilder {
        state,
        binding,
        observed: ObservedWorkspaceArtifactBuilder {
            id: identity.id,
            source,
            never_started,
            read_only,
            mounts_owned,
            remount_capability_dropped: false,
        },
        source_owned,
        matching,
    })
}

/// The one `/workspace` source the builder's mounts resolve to, with nothing
/// mounted below it. A name, label or volume never substitutes for it, so a
/// builder belonging to another checkout's source (or one bound to a VM
/// directory that does not exist on the host) is never adopted.
///
/// Identification does not require the bind to be writable: an owned read-write
/// leftover is observed so the caller can refuse or recover it. Execution
/// authority still requires `RW=false`.
fn workspace_builder_source_mount(
    mounts: Option<&Vec<serde_json::Value>>,
) -> Option<(PathBuf, bool)> {
    let mounts = mounts?;
    let workspace_mounts = mounts
        .iter()
        .filter(|mount| {
            mount.get("Destination").and_then(|value| value.as_str()) == Some("/workspace")
        })
        .collect::<Vec<_>>();
    let shadowed = mounts.iter().any(|mount| {
        mount
            .get("Destination")
            .and_then(|value| value.as_str())
            .is_some_and(|destination| destination.starts_with("/workspace/"))
    });
    if shadowed {
        return None;
    }
    let [mount] = workspace_mounts.as_slice() else {
        return None;
    };
    if mount.get("Type").and_then(|value| value.as_str()) != Some("bind") {
        return None;
    }
    let read_only = mount.get("RW").and_then(|value| value.as_bool()) == Some(false);
    let source = mount
        .get("Source")
        .and_then(|value| value.as_str())
        .and_then(|value| Path::new(value).canonicalize().ok())?;
    Some((source, read_only))
}

/// Where this environment's builder source must be: the canonical checkout, or
/// the checkout's owned staged source when the runtime profile cannot reach the
/// checkout's own host path (effigy#715). Both the render and this proof resolve
/// the same plan, so a render cannot bind a source the proof would not accept.
pub fn expected_workspace_source(
    canonical_checkout: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<PathBuf, ContainerExecError> {
    let stage = crate::artifact_source::policy_stage(policy).map_err(|error| {
        ContainerExecError::refused("resolve workspace builder source", error.to_string())
    })?;
    let Some(plan) = stage else {
        return Ok(canonical_checkout.to_path_buf());
    };
    plan.source.canonicalize().map_err(|error| {
        ContainerExecError::refused(
            "resolve workspace builder source",
            format!(
                "the owned staged source `{}` this checkout binds is not readable: {error}",
                plan.source.display()
            ),
        )
    })
}

/// The builder declares no route, so an inspected record with published
/// associations is not the private builder. A missing `NetworkSettings.Ports`
/// object is not proof of "no ports".
fn workspace_builder_has_no_published_ports(record: Option<&serde_json::Value>) -> bool {
    record
        .and_then(|record| record.pointer("/NetworkSettings/Ports"))
        .and_then(|value| value.as_object())
        .is_some_and(|ports| ports.is_empty())
}

fn builder_working_directory_is_owned(
    checkout: &Path,
    staged: Option<&crate::artifact_source::StagePlan>,
    label: Option<&serde_json::Value>,
) -> bool {
    let Some(label) = label else { return true };
    let Some(path) = label
        .as_str()
        .and_then(|value| Path::new(value).canonicalize().ok())
    else {
        return false;
    };
    if path == checkout {
        return true;
    }
    if staged.is_some_and(|plan| plan.owns_staged_render_directory(&path)) {
        return true;
    }
    if !path.is_dir() {
        return false;
    }
    let Ok(relative) = path.strip_prefix(checkout.join(".effigy/state/objects")) else {
        return false;
    };
    let Some(digest) = relative
        .components()
        .next()
        .and_then(|part| part.as_os_str().to_str())
    else {
        return false;
    };
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod legacy_admission_tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::OwnershipContext;
    use crate::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerSecretDelivery,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };

    fn policy(project: &str) -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            render: None,
            repo_root: PathBuf::from("/repo"),
            name: "web".to_owned(),
            driver: ManifestContainerDriver::Colima,
            startup: ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Direct,
            compose_files: vec![],
            managed_volumes: vec![],
            borrowed_volumes: vec![],
            data_peers: Vec::new(),
            data_from: None,
            shared_services: vec![],
            project_name: project.to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
            runtime_host_key: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: vec![],
            service_aliases: vec![],
            declared_ports: vec![],
            ports_declared_explicitly: false,
            declared_mounts: vec![],
            declared_media_mounts: vec![],
            workspace_source: crate::artifact_source::WorkspaceSource::Checkout,
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: ManifestContainerSecretDelivery::ComposeEnv,
            secret_runtime_dir: None,
            source_secret_runtime_for_deferrals: false,
            workspace_user: None,
            workspace_home: None,
            on_task_exit: ManifestContainerOnTaskExit::Stop,
            shutdown: ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: vec![],
        }
    }

    fn unscoped_context(checkout: &Path) -> OwnershipContext {
        OwnershipContext {
            declared: BTreeSet::from(["app".to_owned()]),
            canonical: checkout.to_path_buf(),
            scope_token: None,
            staged_render: None,
        }
    }

    #[test]
    fn shutdown_uses_full_ids_and_checkout_scope_labels_without_inspect() {
        let private = tempfile::tempdir().expect("private shutdown labels");
        let root = private.path().canonicalize().expect("canonical checkout");
        let mut context = unscoped_context(&root);
        let policy = policy("acme");
        let id = "a".repeat(64);
        for checkout_column in [
            format!("{}\t", root.display()),
            format!("\t{}", root.display()),
        ] {
            let row = format!("{id}\tacme\t{checkout_column}\t\n");
            context
                .verify_shutdown_listing(&policy, row.as_bytes())
                .expect("Docker working_dir or nerdctl checkout label");
        }
        context
            .verify_shutdown_listing(&policy, b"")
            .expect("already gone");
        for row in [
            format!("short\tacme\t{}\t\t\n", root.display()),
            format!("{id}\tforeign\t{}\t\t\n", root.display()),
            format!("{id}\tacme\t/unknown-checkout\t\t\n"),
            format!("{id}\tacme\t{}\t\tforeign-scope\n", root.display()),
        ] {
            assert!(
                context
                    .verify_shutdown_listing(&policy, row.as_bytes())
                    .is_err(),
                "{row}"
            );
        }
        context.scope_token = Some("owned-scope".to_owned());
        let rendered = format!(
            "{id}\tacme\t/immutable/render\t{}\towned-scope\n",
            root.display()
        );
        context
            .verify_shutdown_listing(&policy, rendered.as_bytes())
            .expect("scoped render uses checkout label");
        let foreign = format!("{id}\tacme\t{}\t/foreign\towned-scope\n", root.display());
        assert!(context
            .verify_shutdown_listing(&policy, foreign.as_bytes())
            .is_err());
        let row = format!("{id}\tacme\t{}\t\towned-scope\n", root.display());
        context
            .verify_shutdown_listing(&policy, row.as_bytes())
            .expect("matching scoped ownership");
    }

    /// The retained nerdctl inspect shape an operator's report captured for the
    /// private builder (effigy#710), with every identity value derived from a
    /// private fixture checkout: full 64-character id, `Name` without a leading
    /// slash, `State.Status`/`Running`/`Restarting`, the bind plus both cargo
    /// caches and an empty `NetworkSettings.Ports`. No live container, engine
    /// or VM is read.
    fn retained_nerdctl_inspect(
        project: &str,
        checkout: &Path,
        workspace_source: &Path,
    ) -> serde_json::Value {
        let checkout_label = checkout.display().to_string();
        let source = workspace_source.display().to_string();
        serde_json::json!({
            "Id": "80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce",
            "Created": "2026-11-02T21:25:31.41161783Z",
            "Path": "bash",
            "Args": ["-lc", "while true; do sleep 3600; done"],
            "State": {
                "Status": "running",
                "Running": true,
                "Paused": false,
                "Restarting": true,
                "Pid": 983,
                "ExitCode": 0,
                "Error": "",
                "StartedAt": "2026-11-08T11:29:18.538605086Z",
                "FinishedAt": "",
            },
            "Image": "sha256:2f8c5ff844de522282140bc51b634eb1da22df359e701197f0b0016416f41217",
            "Name": format!("{project}-builder-1"),
            "RestartCount": 0,
            "Driver": "runc",
            "Platform": "linux/arm64/v8",
            "Mounts": [
                {"Type": "bind", "Source": source, "Destination": "/workspace", "Mode": "", "RW": true, "Propagation": ""},
                {"Type": "volume", "Name": format!("{project}-cargo-registry"), "Source": format!("/var/lib/nerdctl/1935db59/volumes/default/{project}-cargo-registry/_data"), "Destination": "/root/.cargo/registry", "Driver": "local", "Mode": "", "RW": true, "Propagation": ""},
                {"Type": "volume", "Name": format!("{project}-cargo-git"), "Source": format!("/var/lib/nerdctl/1935db59/volumes/default/{project}-cargo-git/_data"), "Destination": "/root/.cargo/git", "Driver": "local", "Mode": "", "RW": true, "Propagation": ""},
            ],
            "Config": {
                "Hostname": "builder",
                "Image": format!("docker.io/library/{project}-builder:latest"),
                "WorkingDir": "/workspace",
                "Labels": {
                    "com.docker.compose.project": project,
                    "com.docker.compose.service": "builder",
                    "com.effigy.checkout": checkout_label,
                    "nerdctl/name": format!("{project}-builder-1"),
                    "nerdctl/platform": "linux/arm64/v8",
                    "nerdctl/state-dir": "/var/lib/nerdctl/1935db59/containers/default/80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce",
                    "containerd.io/restart.policy": "unless-stopped",
                    "containerd.io/restart.status": "running",
                },
            },
            // The live report: nerdctl's compatibility view rebuilds CapDrop from
            // the default capability set, so a dropped SYS_ADMIN is never listed.
            "HostConfig": {
                "Privileged": false,
                "CapAdd": [],
                "CapDrop": [],
            },
            "NetworkSettings": {
                "Bridge": "",
                "SandboxID": "0d5d1c0a8d9dbfc0f9d3d6f6d8ba44a44ed8d5b2f7b3f2f5d9f7c1a9e6d3b8f0",
                "HairpinMode": false,
                "Ports": {},
                "Gateway": "",
                "IPAddress": "10.4.2.3",
                "IPPrefixLen": 24,
                "MacAddress": "aa:4b:8a:7c:7e:6c",
            },
        })
    }

    fn builder_row(project: &str) -> super::RunningComposeContainer {
        super::RunningComposeContainer {
            container_name: format!("{project}-builder-1"),
            status: "Up".into(),
            ports: vec![],
            project_name: Some(project.to_owned()),
            working_dir: None,
            service: Some("builder".to_owned()),
            oneoff: false,
        }
    }

    fn builder_context(checkout: &Path) -> OwnershipContext {
        OwnershipContext {
            declared: BTreeSet::from(["builder".to_owned()]),
            canonical: checkout.to_path_buf(),
            scope_token: None,
            staged_render: None,
        }
    }

    #[test]
    fn retained_nerdctl_shape_proves_an_owned_current_builder() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let context = builder_context(&checkout);
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let binding = context
            .verify_inspected(
                &checkout,
                &policy(project),
                &builder_row(project),
                serde_json::json!([record]).to_string().as_bytes(),
            )
            .expect("the retained nerdctl shape is an owned running builder");
        assert_eq!(
            binding.id(),
            "80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce"
        );
        assert_eq!(binding.scope, None);
        assert_eq!(
            super::workspace_builder_source_mount(
                record.get("Mounts").and_then(|value| value.as_array())
            )
            .as_ref()
            .map(|(source, _)| source.as_path()),
            Some(checkout.as_path())
        );
        assert!(super::workspace_builder_has_no_published_ports(Some(
            &record
        )));
        assert!(super::builder_cache_volumes_are_owned(
            record.get("Mounts").and_then(|value| value.as_array()),
            project
        ));
        // Docker's witness cannot be met by the nerdctl compatibility shape...
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        // ...the native OCI spec of the same full ID is the witness that is.
        let id = record["Id"].as_str().expect("id");
        assert_eq!(
            super::native_capability_absent(
                Some(&record),
                &native_inspect(id, default_capabilities()),
                id
            ),
            Ok(true)
        );
    }

    /// The OCI process capability sets nerdctl stores for the builder. Compose
    /// `cap_drop: [SYS_ADMIN]` on a container never granted SYS_ADMIN leaves
    /// these sets at the default 14 capabilities.
    fn default_capabilities() -> serde_json::Value {
        let defaults = [
            "CAP_CHOWN",
            "CAP_DAC_OVERRIDE",
            "CAP_FSETID",
            "CAP_FOWNER",
            "CAP_MKNOD",
            "CAP_NET_RAW",
            "CAP_SETGID",
            "CAP_SETUID",
            "CAP_SETFCAP",
            "CAP_SETPCAP",
            "CAP_NET_BIND_SERVICE",
            "CAP_SYS_CHROOT",
            "CAP_KILL",
            "CAP_AUDIT_WRITE",
        ];
        serde_json::json!({
            "bounding": defaults,
            "effective": defaults,
            "permitted": defaults,
        })
    }

    /// One native inspect of `id`, as `nerdctl container inspect --mode=native`
    /// prints it: the containerd-native record with its OCI `Spec`.
    fn native_inspect(id: &str, capabilities: serde_json::Value) -> Vec<u8> {
        serde_json::json!([{
            "ID": id,
            "Spec": {"process": {"capabilities": capabilities}},
        }])
        .to_string()
        .into_bytes()
    }

    #[test]
    fn running_inspect_refuses_an_extra_host_bind_and_a_missing_sys_admin_drop() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["Mounts"]
            .as_array_mut()
            .expect("mounts")
            .push(serde_json::json!({
                "Type": "bind",
                "Source": "/opt/extra",
                "Destination": "/opt/extra",
                "RW": true
            }));
        assert!(!super::builder_cache_volumes_are_owned(
            record.get("Mounts").and_then(|value| value.as_array()),
            project
        ));
        // Docker's compatibility record names the drop; without it, refuse.
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["HostConfig"]["CapDrop"] = serde_json::json!(["SYS_ADMIN"]);
        assert!(super::builder_remount_capability_dropped(Some(&record)));
        record["HostConfig"]["CapDrop"] = serde_json::json!([]);
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        record["HostConfig"]["CapDrop"] = serde_json::json!(["SYS_ADMIN"]);
        record["HostConfig"]["Privileged"] = serde_json::json!(true);
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        record["HostConfig"]["Privileged"] = serde_json::json!(false);
        record["HostConfig"]["CapAdd"] = serde_json::json!(["SYS_PTRACE"]);
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
        record.as_object_mut().expect("record").remove("HostConfig");
        assert!(!super::builder_remount_capability_dropped(Some(&record)));
    }

    #[test]
    fn native_witness_refuses_sys_admin_in_any_process_capability_set() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id");
        // The exec path inherits the stored sets, so any one of them holding
        // SYS_ADMIN is the remount capability still being available.
        for set in [
            "bounding",
            "effective",
            "inheritable",
            "permitted",
            "ambient",
        ] {
            let mut capabilities = default_capabilities();
            capabilities[set] = serde_json::json!(["CAP_CHOWN", "CAP_SYS_ADMIN"]);
            assert_eq!(
                super::native_capability_absent(
                    Some(&record),
                    &native_inspect(id, capabilities.clone()),
                    id
                ),
                Ok(false),
                "{set} SYS_ADMIN"
            );
            // A known non-default capability outside bounding is power too.
            capabilities[set] = serde_json::json!(["CAP_CHOWN", "CAP_SYS_PTRACE"]);
            assert_eq!(
                super::native_capability_absent(
                    Some(&record),
                    &native_inspect(id, capabilities),
                    id
                ),
                Ok(false),
                "{set}"
            );
        }
    }

    #[test]
    fn native_witness_refuses_privilege_and_add_back() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let id = "80b63d97762b5a4858fe316356c54ac41007ddb19309a88028cf1b41a8613cce";
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["HostConfig"]["Privileged"] = serde_json::json!(true);
        assert_eq!(
            super::native_capability_absent(
                Some(&record),
                &native_inspect(id, default_capabilities()),
                id
            ),
            Ok(false)
        );
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record["HostConfig"]["CapAdd"] = serde_json::json!(["SYS_PTRACE"]);
        assert_eq!(
            super::native_capability_absent(
                Some(&record),
                &native_inspect(id, default_capabilities()),
                id
            ),
            Ok(false)
        );
    }

    #[test]
    fn native_witness_refuses_missing_malformed_or_replaced_identity() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id");
        let other = "b".repeat(64);
        let reject = |output: Vec<u8>, case: &str| {
            assert!(
                super::native_capability_absent(Some(&record), &output, id).is_err(),
                "{case} must refuse"
            );
        };
        reject(
            native_inspect(&other, default_capabilities()),
            "replaced id",
        );
        reject(b"not json".to_vec(), "non-JSON");
        reject(b"[]".to_vec(), "no record");
        reject(
            serde_json::json!([{"ID": id}]).to_string().into_bytes(),
            "no Spec",
        );
        reject(
            native_inspect(id, serde_json::json!({"effective": ["CAP_CHOWN"]})),
            "no bounding set",
        );
        reject(
            native_inspect(id, serde_json::json!(["CAP_CHOWN"])),
            "capabilities is a list",
        );
        let mut scalar = default_capabilities();
        scalar["effective"] = serde_json::json!("CAP_CHOWN");
        reject(native_inspect(id, scalar), "scalar set");
        for entry in [
            "SYS_ADMIN",
            "cap_sys_admin",
            "CAP_Sys_Admin",
            "",
            "CAP_SYS_ADMINX",
            "CAP_BOGUS",
        ] {
            let mut capabilities = default_capabilities();
            capabilities["bounding"] = serde_json::json!(["CAP_CHOWN", entry]);
            reject(native_inspect(id, capabilities), entry);
        }
    }

    #[test]
    fn retained_nerdctl_shape_refuses_another_checkouts_source_without_adoption() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        // The retained builder effigy#710 collided with: the same project
        // spelling, a different checkout's real source and no mount of this
        // checkout. The foreign source is a derived private directory, never
        // the retained checkout itself.
        let foreign = private.path().join("other-checkout");
        std::fs::create_dir_all(&foreign).expect("foreign checkout");
        let foreign = foreign.canonicalize().expect("canonical foreign checkout");
        let project = "effigy-artifact-builder-v1";
        let context = builder_context(&checkout);
        let record = retained_nerdctl_inspect(project, &foreign, &foreign);
        let error = context
            .verify_inspected(
                &checkout,
                &policy(project),
                &builder_row(project),
                serde_json::json!([record]).to_string().as_bytes(),
            )
            .expect_err("a builder of another checkout's source is never adopted");
        assert!(
            error.to_string().contains("does not match owned project"),
            "{error}"
        );
        assert_eq!(
            super::workspace_builder_source_mount(
                record.get("Mounts").and_then(|value| value.as_array())
            )
            .as_ref()
            .map(|(source, _)| source.as_path()),
            Some(foreign.as_path())
        );
    }

    #[test]
    fn native_witness_refuses_extra_records_and_unknown_capability_sets() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id");
        // A second record for the same ID is not the one the decision read.
        let one = native_inspect(id, default_capabilities());
        let mut two: Vec<serde_json::Value> =
            serde_json::from_slice(&one).expect("one native record");
        two.push(two[0].clone());
        assert!(super::native_capability_absent(
            Some(&record),
            &serde_json::to_vec(&two).expect("two records"),
            id
        )
        .is_err());
        // An unknown set beside the five OCI sets may hold SYS_ADMIN and must
        // not be ignored.
        for key in ["ambientExtra", "Bounding", "bounding_extra"] {
            let mut capabilities = default_capabilities();
            capabilities[key] = serde_json::json!(["CAP_SYS_ADMIN"]);
            assert!(
                super::native_capability_absent(
                    Some(&record),
                    &native_inspect(id, capabilities),
                    id
                )
                .is_err(),
                "{key}"
            );
        }
    }

    #[test]
    fn nerdctl_compat_add_back_must_be_present_and_strict() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let record = retained_nerdctl_inspect(project, &checkout, &checkout);
        let id = record["Id"].as_str().expect("id").to_owned();
        let output = native_inspect(&id, default_capabilities());
        // Missing or malformed compatibility fields are refused, never read as
        // an add-back-free builder that recovery could replace.
        type Mutation = fn(&mut serde_json::Value);
        let malformed: [(&str, Mutation); 5] = [
            ("CapAdd missing", |row| {
                row["HostConfig"]
                    .as_object_mut()
                    .expect("host")
                    .remove("CapAdd");
            }),
            ("CapAdd null", |row| {
                row["HostConfig"]["CapAdd"] = serde_json::Value::Null
            }),
            ("CapAdd string", |row| {
                row["HostConfig"]["CapAdd"] = serde_json::json!("CAP_SYS_PTRACE")
            }),
            ("Privileged missing", |row| {
                row["HostConfig"]
                    .as_object_mut()
                    .expect("host")
                    .remove("Privileged");
            }),
            ("HostConfig missing", |row| {
                row.as_object_mut().expect("record").remove("HostConfig");
            }),
        ];
        for (case, mutate) in malformed {
            let mut record = record.clone();
            mutate(&mut record);
            assert!(
                super::native_capability_absent(Some(&record), &output, &id).is_err(),
                "{case} must refuse"
            );
        }
        // Well-formed and present: an empty list is the witness; a privileged
        // builder or an added capability is a definite Ok(false), not an error.
        assert_eq!(
            super::native_capability_absent(Some(&record), &output, &id),
            Ok(true)
        );
    }

    #[test]
    fn inspect_refusals_name_the_concrete_cause_and_never_claim_a_signal() {
        let private = tempfile::tempdir().expect("private checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-artifact-builder-v1-co-0123456789ab";
        let context = builder_context(&checkout);
        let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
        record
            .as_object_mut()
            .expect("record")
            .get_mut("State")
            .and_then(|state| state.as_object_mut())
            .expect("state")
            .remove("Status");
        let error = context
            .verify_inspected(
                &checkout,
                &policy(project),
                &builder_row(project),
                serde_json::json!([record]).to_string().as_bytes(),
            )
            .expect_err("an incomplete inspect record is not proof of identity");
        assert_eq!(
            error.to_string(),
            "inspect owned binding refused: binding unavailable: canonical container identity is unverified: inspect record missing State.Status"
        );
    }

    #[test]
    fn builder_decoder_refuses_identity_source_and_created_ownership_permutations() {
        use std::os::unix::fs::MetadataExt;
        let private = tempfile::tempdir().expect("checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-builder-table";
        let mut context = builder_context(&checkout);
        context.scope_token = Some("owned-scope".to_owned());
        let metadata = std::fs::metadata(&checkout).expect("checkout identity");
        for created in [false, true] {
            let mut base = retained_nerdctl_inspect(project, &checkout, &checkout);
            base["Mounts"][0]["RW"] = serde_json::json!(false);
            base["Config"]["Labels"]["com.effigy.scope"] = serde_json::json!("owned-scope");
            if created {
                base["State"] = serde_json::json!({"Status":"", "Running":false, "Paused":false, "Restarting":false, "Pid":0, "StartedAt":"", "FinishedAt":"", "Error":""});
                for name in [
                    super::CHECKOUT_GENERATION_LABEL,
                    super::CHECKOUT_FENCE_LABEL,
                ] {
                    base["Config"]["Labels"][name] = serde_json::json!(
                        serde_json::json!({"device":metadata.dev(),"inode":metadata.ino()})
                            .to_string()
                    );
                }
            }
            let mut row = builder_row(project);
            if created {
                row.status = "Created".into();
            }
            let decode = |record: serde_json::Value| {
                super::decode_workspace_builder(
                    &context,
                    &policy(project),
                    &row,
                    &serde_json::json!([record]),
                    crate::compose::ComposeBackend::ColimaNerdctl,
                )
            };
            assert!(decode(base.clone()).is_ok(), "baseline created={created}");
            for case in [
                "scope",
                "missing_scope",
                "project",
                "service",
                "oneoff",
                "checkout",
                "unlabelled",
                "working_dir",
                "short_id",
                "name",
                "ports",
                "missing_bind",
                "shadow_bind",
                "foreign_bind",
            ] {
                let mut record = base.clone();
                match case {
                    "scope" => record["Config"]["Labels"]["com.effigy.scope"] = serde_json::json!("foreign"),
                    "missing_scope" => { record["Config"]["Labels"].as_object_mut().unwrap().remove("com.effigy.scope"); }
                    "project" => record["Config"]["Labels"]["com.docker.compose.project"] = serde_json::json!("foreign"),
                    "service" => record["Config"]["Labels"]["com.docker.compose.service"] = serde_json::json!("app"),
                    "oneoff" => record["Config"]["Labels"]["com.docker.compose.oneoff"] = serde_json::json!("True"),
                    "checkout" => record["Config"]["Labels"]["com.effigy.checkout"] = serde_json::json!("/nonexistent/foreign"),
                    "unlabelled" => { record["Config"]["Labels"].as_object_mut().unwrap().remove("com.effigy.checkout"); }
                    "working_dir" => record["Config"]["Labels"]["com.docker.compose.project.working_dir"] = serde_json::json!("/nonexistent/foreign"),
                    "short_id" => record["Id"] = serde_json::json!("a".repeat(12)),
                    "name" => record["Name"] = serde_json::json!("successor-builder-1"),
                    "ports" => record["NetworkSettings"]["Ports"] = serde_json::json!({"80/tcp":[{"HostIp":"127.0.0.1","HostPort":"12345"}]}),
                    "missing_bind" => record["Mounts"] = serde_json::json!([]),
                    "shadow_bind" => record["Mounts"].as_array_mut().unwrap().push(serde_json::json!({"Type":"bind", "Source":checkout, "Destination":"/workspace/src", "RW":true})),
                    "foreign_bind" => record["Mounts"][0]["Source"] = serde_json::json!("/nonexistent/foreign"),
                    _ => unreachable!(),
                }
                assert!(decode(record).is_err(), "created={created}: {case}");
            }
            if created {
                for case in [
                    "generation",
                    "fence",
                    "missing_generation",
                    "cache_name",
                    "extra_mount",
                ] {
                    let mut record = base.clone();
                    match case {
                        "generation" => record["Config"]["Labels"][super::CHECKOUT_GENERATION_LABEL] = serde_json::json!(serde_json::json!({"device":metadata.dev(),"inode":metadata.ino()+1}).to_string()),
                        "fence" => record["Config"]["Labels"][super::CHECKOUT_FENCE_LABEL] = serde_json::json!(serde_json::json!({"device":metadata.dev()+1,"inode":metadata.ino()}).to_string()),
                        "missing_generation" => { record["Config"]["Labels"].as_object_mut().unwrap().remove(super::CHECKOUT_GENERATION_LABEL); }
                        "cache_name" => record["Mounts"][1]["Name"] = serde_json::json!("foreign-cache"),
                        "extra_mount" => record["Mounts"].as_array_mut().unwrap().push(serde_json::json!({"Type":"volume", "Name":"extra", "Destination":"/extra", "RW":true})),
                        _ => unreachable!(),
                    }
                    assert!(decode(record).is_err(), "created: {case}");
                }
            }
        }
    }

    #[test]
    fn builder_decoder_refuses_non_builder_listing_even_if_declared() {
        let private = tempfile::tempdir().expect("checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-builder-table";
        let mut context = builder_context(&checkout);
        context.declared.insert("app".to_owned());
        for (service, oneoff) in [(Some("app"), false), (None, false), (Some("builder"), true)] {
            let mut row = builder_row(project);
            row.service = service.map(str::to_owned);
            row.oneoff = oneoff;
            let mut record = retained_nerdctl_inspect(project, &checkout, &checkout);
            record["Config"]["Labels"]["com.docker.compose.service"] = serde_json::json!(service);
            assert!(
                super::decode_workspace_builder(
                    &context,
                    &policy(project),
                    &row,
                    &serde_json::json!([record]),
                    crate::compose::ComposeBackend::ColimaNerdctl,
                )
                .is_err(),
                "service={service:?}, oneoff={oneoff}"
            );
        }
    }

    #[test]
    fn builder_preparation_table_keeps_state_and_recovery_boundaries() {
        use super::{WorkspaceBuilderPreparation as Step, WorkspaceBuilderState as State};
        let private = tempfile::tempdir().expect("checkout");
        let checkout = private.path().canonicalize().expect("canonical checkout");
        let project = "effigy-builder-table";
        let context = builder_context(&checkout);
        for no_change in [false, true] {
            for retained in [false, true] {
                let result = super::builder_preparation(None, retained, no_change);
                assert_eq!(
                    matches!(result, Ok(Step::Activate(None))),
                    !retained && !no_change
                );
            }
            for state in [
                State::NeverStarted,
                State::Stopped,
                State::Running,
                State::Unknown,
            ] {
                for defect in [
                    "none",
                    "writable",
                    "mounts",
                    "capabilities",
                    "old_source",
                    "foreign_source",
                ] {
                    let record = retained_nerdctl_inspect(project, &checkout, &checkout);
                    let mut inspected = super::decode_workspace_builder(
                        &context,
                        &policy(project),
                        &builder_row(project),
                        &serde_json::json!([record]),
                        crate::compose::ComposeBackend::ColimaNerdctl,
                    )
                    .expect("owned record");
                    inspected.state = state;
                    inspected.observed.never_started = matches!(state, State::NeverStarted);
                    inspected.observed.read_only = defect != "writable";
                    inspected.observed.mounts_owned = defect != "mounts";
                    inspected.observed.remount_capability_dropped = defect != "capabilities";
                    inspected.source_owned = defect != "foreign_source";
                    inspected.matching = defect == "none";
                    let result = super::builder_preparation(Some(inspected), false, no_change);
                    let reuse = matches!(state, State::Running) && defect == "none";
                    let activate = matches!(state, State::Running | State::NeverStarted)
                        && defect != "none"
                        && defect != "foreign_source"
                        && !no_change;
                    assert_eq!(
                        matches!(&result, Ok(Step::Reuse(_))),
                        reuse,
                        "reuse {defect}"
                    );
                    assert_eq!(
                        matches!(&result, Ok(Step::Activate(Some(_)))),
                        activate,
                        "activate {defect}"
                    );
                    assert_eq!(result.is_err(), !reuse && !activate, "refusal {defect}");
                }
            }
        }
    }
}
