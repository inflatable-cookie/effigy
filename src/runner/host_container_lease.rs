use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use effigy_cli::InternalContainerLeaseReaperArgs;
use effigy_containers::exec::shutdown_container;
use effigy_containers::EffectiveContainerPolicy;
use effigy_ui::style_text;
use effigy_ui::theme::{resolve_color_enabled, Theme};
use effigy_ui::OutputMode;
use sha2::{Digest, Sha256};
use std::io::IsTerminal;

use super::error::RunnerError;
use super::system_command::is_primary_service_running;

const HOST_CONTAINER_LEASE_HOME_DIR: &str = ".effigy/runtime/host-container-leases";
const HOST_CONTAINER_LEASE_FALLBACK_DIR: &str = ".effigy/runtime/host-container-leases";
const HOST_CONTAINER_LEASE_SCHEMA: &str = "effigy.host-container-lease.v1";
const DEFAULT_HOST_CONTAINER_LEASE_TIMEOUT_SECS: u64 = 300;
const DISABLE_HOST_CONTAINER_LEASE_ENV: &str = "EFFIGY_DISABLE_HOST_CONTAINER_LEASE";
const PARTIAL_ACTIVATION_SCHEMA: &str = "effigy.container-partial-activation.v2";
const HANDOVER_RECORD_SCHEMA: &str = "effigy.container-handover.v1";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct HostContainerLease {
    #[serde(default = "host_container_lease_schema")]
    schema: String,
    #[serde(default = "host_container_lease_schema_version")]
    schema_version: u8,
    token: String,
    container_name: String,
    profile: String,
    project_name: String,
    expires_at_epoch_ms: u128,
}

pub(super) fn has_active_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    let Some(lease) = load_host_container_lease(repo_root, policy)? else {
        return Ok(false);
    };
    Ok(lease.expires_at_epoch_ms > now_epoch_ms())
}

pub(in crate::runner) fn refresh_host_container_lease_for_task_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    system_was_running: bool,
) -> Result<bool, RunnerError> {
    if host_container_lease_disabled() {
        return Ok(false);
    }
    // A stack admitted by detached `container up` is persistent: a task that
    // reuses or restarts it must not attach an idle-shutdown lease, and an
    // older lease no longer applies.
    if detached_owner_recorded(repo_root, policy) {
        clear_host_container_lease(repo_root, policy)?;
        return Ok(false);
    }
    let had_active_lease = has_active_host_container_lease(repo_root, policy)?;
    if had_active_lease || !system_was_running {
        refresh_host_container_lease(repo_root, policy)?;
        return Ok(true);
    }
    Ok(false)
}

fn detached_owner_recorded(repo_root: &Path, policy: &EffectiveContainerPolicy) -> bool {
    persistent_owner_path(repo_root, policy).is_file()
}

/// An explicit `effigy container up` that left the stack running owns it
/// persistently. Task activation never writes this; `container down` clears it.
pub(in crate::runner) fn mark_persistent_container_owner(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    fs::create_dir_all(host_container_lease_dir(repo_root, policy))
        .map_err(|error| RunnerError::task_invocation_failed_write(repo_root, error))?;
    let path = persistent_owner_path(repo_root, policy);
    fs::write(&path, b"")
        .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))?;
    clear_host_container_lease(repo_root, policy)
}

pub(in crate::runner) fn clear_persistent_container_owner(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let path = persistent_owner_path(repo_root, policy);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RunnerError::task_invocation_failed_write(&path, error)),
    }
}

fn persistent_owner_path(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    host_container_lease_dir(repo_root, policy).join(format!(
        "{}.persistent",
        sanitize_lease_path_component(&policy.name)
    ))
}

pub(super) fn refresh_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    fs::create_dir_all(host_container_lease_dir(repo_root, policy))
        .map_err(|error| RunnerError::task_invocation_failed_write(repo_root, error))?;
    let token = format!(
        "{}-{}-{}",
        now_epoch_ms(),
        std::process::id(),
        policy.name.replace('/', "_")
    );
    let lease = HostContainerLease {
        schema: host_container_lease_schema(),
        schema_version: host_container_lease_schema_version(),
        token: token.clone(),
        container_name: policy.name.clone(),
        profile: policy.profile.clone(),
        project_name: policy.project_name.clone(),
        expires_at_epoch_ms: now_epoch_ms() + host_container_lease_timeout().as_millis(),
    };
    let encoded = serde_json::to_string_pretty(&lease)
        .map_err(|error| RunnerError::host_container_lease_encode(error.to_string()))?;
    let lease_path = host_container_lease_path(repo_root, policy);
    fs::write(&lease_path, encoded)
        .map_err(|error| RunnerError::task_invocation_failed_write(&lease_path, error))?;
    spawn_host_container_lease_reaper(repo_root, &policy.name, &token)?;
    Ok(())
}

pub(super) fn host_container_lease_timeout_duration() -> Duration {
    host_container_lease_timeout()
}

pub(in crate::runner) fn emit_host_container_lease_notice(container_name: &str) {
    let timeout = format_duration_short(host_container_lease_timeout_duration());
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stderr().is_terminal());
    eprintln!(
        "{} temporary container lease active for `{container_name}`; idle shutdown in {timeout} unless reused. Run `effigy container up --detach` to keep it running, or set {DISABLE_HOST_CONTAINER_LEASE_ENV}=1",
        style_text(color_enabled, Theme::default().label, "[info]")
    );
}

pub(super) fn clear_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let lease_path = host_container_lease_path(repo_root, policy);
    match fs::remove_file(&lease_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RunnerError::task_invocation_failed_write(
            &lease_path,
            error,
        )),
    }
}

/// Marks a detached bring-up whose Compose services started but whose route
/// registration did not finish. A later detached `container up` reads the mark
/// to reconcile routes without running `compose up` again. The mark lives
/// beside the scope's lease, keyed by profile, project and container name.
/// Immutable admission snapshot; publication must compare against this value,
/// never replace it with a baseline read after runtime observation.
#[derive(Debug)]
pub(in crate::runner) struct DetachedActivationContext {
    value: serde_json::Value,
    pub(in crate::runner) marker_required: bool,
}

pub(in crate::runner) fn capture_detached_activation_context(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<DetachedActivationContext, RunnerError> {
    Ok(DetachedActivationContext {
        value: partial_activation_context(repo_root, policy)?,
        marker_required: false,
    })
}

pub(in crate::runner) fn retained_detached_activation_context(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<DetachedActivationContext>, RunnerError> {
    let path = partial_activation_path(repo_root, policy);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&path, error)),
    };
    let value = serde_json::from_str(&raw)
        .map_err(|error| RunnerError::task_invocation_failed_parse(&path, error))?;
    let context = DetachedActivationContext {
        value,
        marker_required: true,
    };
    context.verify(repo_root, policy, true)?;
    Ok(Some(context))
}

impl DetachedActivationContext {
    pub(in crate::runner) fn from_materialization(
        policy: &EffectiveContainerPolicy,
        admission: &effigy_containers::policy::admission::PolicyAdmission,
    ) -> Result<Self, RunnerError> {
        Ok(Self {
            value: admission.context_for_policy(policy)?,
            marker_required: admission.marker_required(),
        })
    }
    pub(in crate::runner) fn verify(
        &self,
        repo_root: &Path,
        policy: &EffectiveContainerPolicy,
        require_marker: bool,
    ) -> Result<(), RunnerError> {
        let verified = (|| -> Result<bool, RunnerError> {
            if self.value != partial_activation_context(repo_root, policy)? {
                return Ok(false);
            }
            if require_marker {
                let path = partial_activation_path(repo_root, policy);
                let raw = fs::read_to_string(&path)
                    .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
                let recorded: serde_json::Value = serde_json::from_str(&raw)
                    .map_err(|error| RunnerError::task_invocation_failed_parse(&path, error))?;
                return Ok(recorded == self.value);
            }
            Ok(true)
        })();
        if !matches!(verified, Ok(true)) {
            return Err(RunnerError::task_invocation(
                "detached activation publication refused: admitted source/checkout/scope/Compose or marker context changed, missing or unreadable; no routes published. Restore the admitted inputs and marker before reconciliation",
            ));
        }
        Ok(())
    }
}

pub(in crate::runner) fn record_admitted_partial_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    admitted: &DetachedActivationContext,
) -> Result<(), RunnerError> {
    admitted.verify(repo_root, policy, false)?;
    fs::create_dir_all(host_container_lease_dir(repo_root, policy))
        .map_err(|error| RunnerError::task_invocation_failed_write(repo_root, error))?;
    let encoded = serde_json::to_string_pretty(&admitted.value)
        .map_err(|error| RunnerError::host_container_lease_encode(error.to_string()))?;
    // A new admission stays partial until its activation completes.
    remove_if_present(&activation_complete_path(repo_root, policy))?;
    let path = partial_activation_path(repo_root, policy);
    use std::io::Write;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return admitted.verify(repo_root, policy, true);
        }
        Err(error) => return Err(RunnerError::task_invocation_failed_write(&path, error)),
    };
    file.write_all(encoded.as_bytes())
        .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))
}

#[cfg(test)]
pub(in crate::runner) fn record_partial_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    fs::create_dir_all(host_container_lease_dir(repo_root, policy))
        .map_err(|error| RunnerError::task_invocation_failed_write(repo_root, error))?;
    let marker = partial_activation_context(repo_root, policy)?;
    let encoded = serde_json::to_string_pretty(&marker)
        .map_err(|error| RunnerError::host_container_lease_encode(error.to_string()))?;
    let marker_path = partial_activation_path(repo_root, policy);
    fs::write(&marker_path, encoded)
        .map_err(|error| RunnerError::task_invocation_failed_write(&marker_path, error))
}

/// A retained artifact builder may execute against its verified source bind
/// without claiming that its old Compose inputs have been activated. Keep the
/// admission untouched, and require every immutable context field to match.
pub(in crate::runner) fn workspace_builder_admission_retained(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<serde_json::Value>, RunnerError> {
    let path = partial_activation_path(repo_root, policy);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&path, error)),
    };
    let mut recorded: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| RunnerError::task_invocation_failed_parse(&path, error))?;
    let original = recorded.clone();
    if !recorded
        .get("inputs")
        .and_then(|inputs| inputs.as_array())
        .is_some_and(|inputs| !inputs.is_empty())
    {
        return Err(RunnerError::task_invocation("workspace artifact builder retained admission inputs are malformed; no build attempted"));
    }
    let mut current = partial_activation_context(repo_root, policy)?;
    if let (Some(recorded), Some(current)) = (recorded.as_object_mut(), current.as_object_mut()) {
        // Only input digests may differ. This is execution authority, never a
        // replacement admission or evidence that current Compose was applied.
        recorded.remove("inputs");
        current.remove("inputs");
    }
    if recorded != current {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder retained admission identity is unverified; no activation or build attempted",
        ));
    }
    Ok(Some(original))
}

/// Drop a recorded detached admission whose inputs have since changed, once
/// every running container of the project is proven to belong to this
/// checkout. The next activation then admits the current inputs afresh.
pub(in crate::runner) fn retire_stale_detached_admission(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let marker_path = partial_activation_path(repo_root, policy);
    let raw = match fs::read_to_string(&marker_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(RunnerError::task_invocation_failed_read(
                &marker_path,
                error,
            ))
        }
    };
    let recorded = serde_json::from_str::<serde_json::Value>(&raw).ok();
    if recorded.is_some() && recorded == partial_activation_context(repo_root, policy).ok() {
        return Ok(());
    }
    // A partial activation is reconciled only against its original inputs.
    if !activation_complete_path(repo_root, policy).is_file() {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` has a partial detached activation recorded against different inputs; it is not reconciled with the current declaration. Nothing was started, recreated or published. Run `effigy container down`, then `effigy container up --detach` to activate the current inputs",
            policy.name
        )));
    }
    ensure_running_stack_admits_reactivation(repo_root, policy)?;
    clear_partial_activation(repo_root, policy)
}

/// Refuse a fresh `compose up` over any project container this checkout cannot
/// prove is its own, running or stopped, so another checkout's stack is never
/// started, recreated or replaced.
pub(in crate::runner) fn ensure_running_stack_admits_reactivation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let refusal = |reason: String| {
        RunnerError::task_invocation(format!(
            "container `{}` cannot be re-activated: {reason}. Nothing was started, recreated or published. If these containers belong to this checkout, run `effigy container down`, then `effigy container up --detach`; otherwise stop the other checkout's stack first",
            policy.name
        ))
    };
    // Include stopped rows: a project-wide `compose up` can start or recreate a
    // stopped declared sibling, so its ownership must be proven too.
    let rows =
        effigy_containers::exec::list_compose_containers_for_policy_including_stopped_with_deadline(
            repo_root,
            policy,
            Some(deadline),
        )
        .map_err(|error| refusal(format!("project containers could not be listed ({error})")))?;
    effigy_containers::exec::running_stack_admits_reactivation(repo_root, policy, &rows, deadline)
        .map_err(|error| refusal(error.to_string()))
}

pub(in crate::runner) fn partial_activation_recorded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    let marker_path = partial_activation_path(repo_root, policy);
    match fs::read_to_string(&marker_path) {
        Ok(raw) => {
            let marker: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|error| RunnerError::task_invocation_failed_parse(&marker_path, error))?;
            if marker != partial_activation_context(repo_root, policy)? {
                return Err(RunnerError::task_invocation(
                    "partial activation context changed or is unverified; reconciliation refused without compose up, restart or teardown. Restore the admitted checkout/Compose inputs before retrying, or explicitly stop the scope with `effigy container down`",
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(RunnerError::task_invocation_failed_read(
            &marker_path,
            error,
        )),
    }
}

fn partial_activation_context(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<serde_json::Value, RunnerError> {
    let canonical_root = fs::canonicalize(repo_root)
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?;
    let loaded =
        effigy_manifest::load_task_manifest_with_inspection(&repo_root.join("effigy.toml"))
            .map_err(|_| RunnerError::task_invocation("original declaration unavailable"))?;
    let mut paths = loaded
        .evaluation_order
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    if let Some(config) = loaded
        .manifest
        .containers
        .as_ref()
        .and_then(|containers| containers.environments.get(&policy.name))
    {
        if let Some(path) = &config.compose_file {
            paths.insert(repo_root.join(path));
        }
    }
    paths.extend(policy.compose_files.iter().map(|path| repo_root.join(path)));
    let mut inputs = Vec::new();
    for path in paths {
        let canonical = fs::canonicalize(&path)
            .map_err(|error| RunnerError::task_invocation_failed_read(&path, error))?;
        let bytes = fs::read(&canonical)
            .map_err(|error| RunnerError::task_invocation_failed_read(&canonical, error))?;
        inputs.push(serde_json::json!({"path":canonical,"sha256":Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>()}));
    }
    let scope = effigy_core::worktree_scope::resolve(repo_root)
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?
        .map(|scope| {
            fs::read_to_string(
                scope
                    .token_dir
                    .join(effigy_core::worktree_scope::SCOPE_FILE),
            )
        })
        .transpose()
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?;
    Ok(serde_json::json!({
        "schema": PARTIAL_ACTIVATION_SCHEMA,
        "scope_token": scope,
        "repo_root": canonical_root,
        "container_name": policy.name,
        "profile": policy.profile,
        "project_name": policy.project_name,
        "primary_service": policy.primary_service,
        "backend": super::exec_command::runtime_backend_identity_for_policy(repo_root, policy),
        "compose_source": match policy.compose_source {
            effigy_containers::EffectiveComposeSource::Direct => "direct",
            effigy_containers::EffectiveComposeSource::Generated => "generated",
        },
        "inputs": inputs,
    }))
}

pub(in crate::runner) fn clear_partial_activation(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    remove_if_present(&activation_complete_path(repo_root, policy))?;
    remove_if_present(&partial_activation_path(repo_root, policy))?;
    remove_if_present(&handover_record_path(repo_root, policy))
}

/// Record that the admitted detached activation finished readiness and
/// publication, so later changed inputs may start a fresh admission instead
/// of being refused as partial-activation drift. A completed activation also
/// resolves any handover stop recorded for this environment.
pub(in crate::runner) fn mark_detached_activation_complete(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    remove_if_present(&handover_record_path(repo_root, policy))?;
    let path = activation_complete_path(repo_root, policy);
    fs::write(&path, b"").map_err(|error| RunnerError::task_invocation_failed_write(&path, error))
}

/// Record, before a handover stops this environment, that the stop was a
/// deliberate handover to another environment of the same checkout.
/// Retain the handover's provenance until `down` or a completed activation
/// clears it. Recovery also accepts owned stopped stacks without this record.
pub(in crate::runner) fn record_environment_handover(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    stopped_by: &str,
    detail: &str,
) -> Result<(), RunnerError> {
    let scope = effigy_core::worktree_scope::resolve(repo_root)
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?
        .map(|scope| {
            fs::read_to_string(
                scope
                    .token_dir
                    .join(effigy_core::worktree_scope::SCOPE_FILE),
            )
        })
        .transpose()
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?;
    let record = serde_json::json!({
        "schema": HANDOVER_RECORD_SCHEMA,
        "scope_token": scope,
        "repo_root": fs::canonicalize(repo_root)
            .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?,
        "container_name": policy.name,
        "profile": policy.profile,
        "project_name": policy.project_name,
        "stopped_by": stopped_by,
        "detail": detail,
        "recorded_at": chrono::Utc::now().to_rfc3339(),
    });
    fs::create_dir_all(host_container_lease_dir(repo_root, policy))
        .map_err(|error| RunnerError::task_invocation_failed_write(repo_root, error))?;
    let path = handover_record_path(repo_root, policy);
    fs::write(
        &path,
        serde_json::to_string_pretty(&record)
            .map_err(|error| RunnerError::host_container_lease_encode(error.to_string()))?,
    )
    .map_err(|error| RunnerError::task_invocation_failed_write(&path, error))
}

/// The recorded handover stop for this environment, when one is still
/// unresolved. A record whose context no longer matches this checkout's scope
/// or identity is stale and reads as absent.
#[cfg(test)]
pub(in crate::runner) fn environment_handover_recorded(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<serde_json::Value>, RunnerError> {
    let path = handover_record_path(repo_root, policy);
    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&path, error)),
    };
    let record: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| RunnerError::task_invocation_failed_parse(&path, error))?;
    if record.get("schema").and_then(|value| value.as_str()) != Some(HANDOVER_RECORD_SCHEMA)
        || record
            .get("container_name")
            .and_then(|value| value.as_str())
            != Some(&policy.name)
        || record.get("project_name").and_then(|value| value.as_str())
            != Some(policy.project_name.as_str())
    {
        return Ok(None);
    }
    let scope = effigy_core::worktree_scope::resolve(repo_root)
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?
        .map(|scope| {
            fs::read_to_string(
                scope
                    .token_dir
                    .join(effigy_core::worktree_scope::SCOPE_FILE),
            )
        })
        .transpose()
        .map_err(|error| RunnerError::task_invocation_failed_read(repo_root, error))?;
    if record.get("scope_token").and_then(|value| value.as_str()) != scope.as_deref() {
        return Ok(None);
    }
    Ok(Some(record))
}

/// Drop a resolved handover record; absence is fine.
pub(in crate::runner) fn clear_environment_handover_record(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    remove_if_present(&handover_record_path(repo_root, policy))
}

/// An activation attempt in progress is partial until it completes, whether it
/// runs Compose up or reconciles an existing record.
pub(in crate::runner) fn mark_detached_activation_pending(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), RunnerError> {
    remove_if_present(&activation_complete_path(repo_root, policy))
}

fn remove_if_present(path: &Path) -> Result<(), RunnerError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(RunnerError::task_invocation_failed_write(path, error)),
    }
}

fn activation_complete_path(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    host_container_lease_dir(repo_root, policy).join(format!(
        "{}.complete",
        sanitize_lease_path_component(&policy.name)
    ))
}

fn partial_activation_path(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    host_container_lease_dir(repo_root, policy).join(format!(
        "{}.partial.json",
        sanitize_lease_path_component(&policy.name)
    ))
}

fn handover_record_path(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    host_container_lease_dir(repo_root, policy).join(format!(
        "{}.handover.json",
        sanitize_lease_path_component(&policy.name)
    ))
}

pub(in crate::runner) fn run_internal_container_lease_reaper(
    args: InternalContainerLeaseReaperArgs,
) -> Result<String, RunnerError> {
    reap_host_container_lease(&args.repo_root, &args.container_name, &args.token)?;
    Ok(String::new())
}

fn spawn_host_container_lease_reaper(
    repo_root: &Path,
    container_name: &str,
    token: &str,
) -> Result<(), RunnerError> {
    if std::env::var("EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER")
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
    {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|error| {
        RunnerError::host_container_lease_reaper_bootstrap(format!(
            "failed to resolve current executable for host container lease reaper: {error}"
        ))
    })?;
    let mut child = ProcessCommand::new(executable);
    child
        .arg("__container-lease-reaper")
        .arg("--repo-root")
        .arg(repo_root)
        .arg("--container")
        .arg(container_name)
        .arg("--token")
        .arg(token)
        .env("EFFIGY_INTERNAL_SUPPRESS_HEADER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    child
        .spawn()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: "__container-lease-reaper".to_owned(),
            error,
        })?;
    Ok(())
}

fn reap_host_container_lease(
    repo_root: &Path,
    container_name: &str,
    expected_token: &str,
) -> Result<(), RunnerError> {
    loop {
        let policy = match effigy_containers::load_container_policy(repo_root, Some(container_name))
        {
            Ok(policy) => policy,
            Err(_) => return Ok(()),
        };
        let Some(lease) = load_host_container_lease(repo_root, &policy)? else {
            return Ok(());
        };
        if lease.token != expected_token {
            return Ok(());
        }
        let now = now_epoch_ms();
        if lease.expires_at_epoch_ms > now {
            thread::sleep(Duration::from_millis(
                (lease.expires_at_epoch_ms - now).min(u128::from(u64::MAX)) as u64,
            ));
            continue;
        }
        // The stack gained a persistent detached owner while this lease ran.
        if detached_owner_recorded(repo_root, &policy) {
            clear_host_container_lease(repo_root, &policy)?;
            return Ok(());
        }
        if is_primary_service_running(repo_root, &policy)? {
            let _ = shutdown_container(repo_root, &policy);
        }
        clear_host_container_lease(repo_root, &policy)?;
        return Ok(());
    }
}

fn load_host_container_lease(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Option<HostContainerLease>, RunnerError> {
    let lease_path = host_container_lease_path(repo_root, policy);
    let raw = match fs::read_to_string(&lease_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::task_invocation_failed_read(&lease_path, error)),
    };
    let lease = serde_json::from_str::<HostContainerLease>(&raw)
        .map_err(|error| RunnerError::task_invocation_failed_parse(&lease_path, error))?;
    Ok(Some(lease))
}

fn host_container_lease_dir(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    host_container_lease_dir_for(repo_root, &policy.profile, &policy.project_name)
}

fn host_container_lease_dir_for(repo_root: &Path, profile: &str, project_name: &str) -> PathBuf {
    if let Some(home) = host_container_lease_home_dir() {
        return home
            .join(sanitize_lease_path_component(profile))
            .join(sanitize_lease_path_component(project_name));
    }
    repo_root.join(HOST_CONTAINER_LEASE_FALLBACK_DIR)
}

/// Ends persistent ownership for every container in a retired project scope.
/// `retire` works from a scope record and does not know each container name, so
/// it clears every `<name>.persistent` marker under the scope's profile/project
/// lease directory.
pub(in crate::runner) fn clear_persistent_container_owners_for_scope(
    repo_root: &Path,
    profiles: &[String],
    project_names: &[String],
) -> Result<(), RunnerError> {
    for profile in profiles {
        for project_name in project_names {
            let dir = host_container_lease_dir_for(repo_root, profile, project_name);
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(RunnerError::task_invocation_failed_read(&dir, error)),
            };
            for entry in entries {
                let entry =
                    entry.map_err(|error| RunnerError::task_invocation_failed_read(&dir, error))?;
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("persistent") {
                    continue;
                }
                match fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(RunnerError::task_invocation_failed_write(&path, error));
                    }
                }
            }
        }
    }
    Ok(())
}

fn host_container_lease_path(repo_root: &Path, policy: &EffectiveContainerPolicy) -> PathBuf {
    host_container_lease_dir(repo_root, policy).join(format!(
        "{}.json",
        sanitize_lease_path_component(&policy.name)
    ))
}

fn host_container_lease_timeout() -> Duration {
    let seconds = std::env::var("EFFIGY_HOST_CONTAINER_LEASE_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_HOST_CONTAINER_LEASE_TIMEOUT_SECS);
    Duration::from_secs(seconds)
}

fn host_container_lease_disabled() -> bool {
    std::env::var(DISABLE_HOST_CONTAINER_LEASE_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "yes"))
}

fn format_duration_short(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs.is_multiple_of(60) && secs >= 60 {
        let mins = secs / 60;
        if mins == 1 {
            return "1 minute".to_owned();
        }
        return format!("{mins} minutes");
    }
    if secs == 1 {
        return "1 second".to_owned();
    }
    format!("{secs} seconds")
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn host_container_lease_schema() -> String {
    HOST_CONTAINER_LEASE_SCHEMA.to_owned()
}

fn host_container_lease_schema_version() -> u8 {
    1
}

fn sanitize_lease_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

pub(in crate::runner) fn detached_admission_marker_root(repo_root: &Path) -> PathBuf {
    host_container_lease_home_dir()
        .unwrap_or_else(|| repo_root.join(HOST_CONTAINER_LEASE_FALLBACK_DIR))
}

fn host_container_lease_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = std::env::var_os("EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME") {
        return Some(
            PathBuf::from(path)
                .join("runtime")
                .join("host-container-leases"),
        );
    }

    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(HOST_CONTAINER_LEASE_HOME_DIR))
}

#[cfg(test)]
pub(crate) fn run_host_container_lease_reaper_for_tests(
    repo_root: &Path,
    container_name: &str,
    token: &str,
) -> Result<(), RunnerError> {
    reap_host_container_lease(repo_root, container_name, token)
}

#[cfg(test)]
pub(crate) fn read_host_container_lease_token_for_tests(
    repo_root: &Path,
    container_name: &str,
) -> Result<Option<String>, RunnerError> {
    let policy = effigy_containers::load_container_policy(repo_root, Some(container_name))
        .map_err(RunnerError::from)?;
    Ok(load_host_container_lease(repo_root, &policy)?.map(|lease| lease.token))
}

#[cfg(test)]
mod tests {
    use super::refresh_host_container_lease_for_task_activation;
    use crate::runner::test_support::effective_container_policy;

    fn test_policy() -> effigy_containers::EffectiveContainerPolicy {
        effective_container_policy("web", "demo-web-dev", "app", "/tmp/docker-compose.yml")
    }

    #[test]
    fn task_activation_refreshes_lease_when_runtime_was_started() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-started-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
        ]);

        let refreshed =
            refresh_host_container_lease_for_task_activation(&repo_root, &test_policy(), false)
                .expect("refresh lease");
        assert!(refreshed);
    }

    #[test]
    fn task_activation_skips_lease_refresh_for_already_running_unleased_runtime() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-existing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
        ]);

        let refreshed =
            refresh_host_container_lease_for_task_activation(&repo_root, &test_policy(), true)
                .expect("refresh lease");
        assert!(!refreshed);
    }

    #[test]
    fn task_activation_never_leases_a_detached_owned_stack() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-detached-owner-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
        ]);
        let policy = test_policy();
        // An earlier task lease, then an explicit `container up` takes over.
        super::refresh_host_container_lease(&repo_root, &policy).expect("earlier lease");
        super::mark_persistent_container_owner(&repo_root, &policy)
            .expect("explicit container up owns the stack");

        for system_was_running in [false, true] {
            let refreshed = refresh_host_container_lease_for_task_activation(
                &repo_root,
                &policy,
                system_was_running,
            )
            .expect("refresh lease");
            assert!(!refreshed, "a detached-owned stack is never leased");
        }
        assert!(
            !super::host_container_lease_path(&repo_root, &policy).exists(),
            "the earlier temporary lease no longer applies"
        );
    }

    #[test]
    fn task_activation_skips_lease_refresh_when_disabled_by_env() {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-disabled-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        let _env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
            (
                super::DISABLE_HOST_CONTAINER_LEASE_ENV,
                Some("1".to_owned()),
            ),
        ]);

        let refreshed =
            refresh_host_container_lease_for_task_activation(&repo_root, &test_policy(), false)
                .expect("refresh lease");
        assert!(!refreshed);
    }

    fn reaper_fixture(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-reaper-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        std::fs::write(
            repo_root.join("effigy.toml"),
            "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"docker-compose.yml\"\nprimary_service = \"app\"\n",
        )
        .expect("write manifest");
        std::fs::write(
            repo_root.join("docker-compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");
        let lease_home = repo_root.join("lease-home");
        std::fs::create_dir_all(&lease_home).expect("mkdir lease home");
        (repo_root, lease_home)
    }

    fn write_expired_lease(
        repo_root: &std::path::Path,
        policy: &effigy_containers::EffectiveContainerPolicy,
        token: &str,
    ) {
        let lease = super::HostContainerLease {
            schema: super::host_container_lease_schema(),
            schema_version: super::host_container_lease_schema_version(),
            token: token.to_owned(),
            container_name: policy.name.clone(),
            profile: policy.profile.clone(),
            project_name: policy.project_name.clone(),
            expires_at_epoch_ms: 0,
        };
        let path = super::host_container_lease_path(repo_root, policy);
        std::fs::create_dir_all(path.parent().expect("lease dir")).expect("mkdir lease dir");
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&lease).expect("encode lease"),
        )
        .expect("write lease");
    }

    fn lease_env(lease_home: &std::path::Path) -> crate::contract_test_support::EnvGuard {
        crate::contract_test_support::EnvGuard::set_many(&[
            (
                "EFFIGY_TEST_HOST_CONTAINER_LEASE_HOME",
                Some(lease_home.display().to_string()),
            ),
            (
                "EFFIGY_DISABLE_HOST_CONTAINER_LEASE_REAPER",
                Some("1".to_owned()),
            ),
        ])
    }

    /// A stopped liveness script: the reaper records every probe it issues, so
    /// an empty operation list proves it never reached the teardown path.
    fn stopped_liveness_script() -> crate::runner::scripted_doctor::ScriptedGuard {
        use crate::runner::scripted_doctor::{install, ScriptedAnswer, ScriptedPhase};
        install(Box::new(|request| match request.phase {
            ScriptedPhase::ColimaStatus => ScriptedAnswer::ok("status: Stopped\n"),
            _ => ScriptedAnswer::failed(127, "unexpected scripted launch"),
        }))
    }

    #[test]
    fn reaper_skips_persistently_owned_stack_before_probing_liveness() {
        use crate::runner::scripted_doctor::ScriptedPhase;

        let (repo_root, lease_home) = reaper_fixture("persistent");
        let _env = lease_env(&lease_home);
        let policy = effigy_containers::load_container_policy(&repo_root, None).expect("policy");
        let token = "explicit-up-generation";
        write_expired_lease(&repo_root, &policy, token);
        super::mark_persistent_container_owner(&repo_root, &policy).expect("mark owner");
        let script = stopped_liveness_script();

        super::run_host_container_lease_reaper_for_tests(&repo_root, &policy.name, token)
            .expect("reaper returns without stopping the owned stack");

        assert!(
            !super::host_container_lease_path(&repo_root, &policy).exists(),
            "the stale task lease no longer applies"
        );
        assert!(
            super::persistent_owner_path(&repo_root, &policy).exists(),
            "the explicit owner marker survives the reaper"
        );
        assert!(
            script.runtime().operations().is_empty(),
            "an explicitly owned stack must not be probed for teardown"
        );
        assert!(script
            .runtime()
            .operations_for(ScriptedPhase::ColimaStatus)
            .is_empty());
    }

    #[test]
    fn retiring_a_scope_clears_only_its_persistent_owners() {
        let (repo_root, lease_home) = reaper_fixture("retire-scope");
        let _env = lease_env(&lease_home);
        let policy = effigy_containers::load_container_policy(&repo_root, None).expect("policy");
        super::mark_persistent_container_owner(&repo_root, &policy).expect("mark scoped owner");
        let root = super::detached_admission_marker_root(&repo_root);
        let unrelated = root
            .join(&policy.profile)
            .join("unretired-project")
            .join("web.persistent");
        std::fs::create_dir_all(unrelated.parent().expect("dir")).expect("mkdir");
        std::fs::write(&unrelated, b"").expect("write unrelated marker");

        super::clear_persistent_container_owners_for_scope(
            &repo_root,
            std::slice::from_ref(&policy.profile),
            std::slice::from_ref(&policy.project_name),
        )
        .expect("clear retired scope owners");

        assert!(
            !super::persistent_owner_path(&repo_root, &policy).exists(),
            "the retired scope loses persistent ownership"
        );
        assert!(
            unrelated.exists(),
            "an unrelated project's marker is untouched"
        );
    }

    #[test]
    fn reaper_still_probes_and_clears_a_task_started_stack() {
        use crate::runner::scripted_doctor::ScriptedPhase;

        let (repo_root, lease_home) = reaper_fixture("task-started");
        let _env = lease_env(&lease_home);
        let policy = effigy_containers::load_container_policy(&repo_root, None).expect("policy");
        let token = "task-generation";
        write_expired_lease(&repo_root, &policy, token);
        let script = stopped_liveness_script();

        super::run_host_container_lease_reaper_for_tests(&repo_root, &policy.name, token)
            .expect("reaper reclaims the expired task lease");

        assert!(
            !super::host_container_lease_path(&repo_root, &policy).exists(),
            "the expired lease is cleared"
        );
        assert!(
            !super::persistent_owner_path(&repo_root, &policy).exists(),
            "a task-started stack never gains persistent ownership"
        );
        assert_eq!(
            script
                .runtime()
                .operations_for(ScriptedPhase::ColimaStatus)
                .len(),
            1,
            "a task-started stack is probed before teardown"
        );
    }

    fn generated_workspace_checkout(label: &str) -> std::path::PathBuf {
        let repo_root = std::env::temp_dir().join(format!(
            "effigy-host-lease-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&repo_root).expect("mkdir repo");
        std::fs::write(
            repo_root.join("effigy.toml"),
            r#"
[systems]
default = "dev"

[systems.dev]
container = "web"
default_workspace = "app"
working_dir = "/workspace-root/app"
user = "dev"
home = "/home/dev"
mounts = []

[systems.dev.workspaces.app]

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.web]
catalog = "nginx"
variant = "default"
"#,
        )
        .expect("write manifest");
        repo_root
    }

    /// The exact publication seam of a managed detached activation: a strict
    /// admission verify against runtime Compose inputs while another process
    /// resolves the same policy and rewrites those inputs. The managed shell
    /// and exec readiness probes do exactly this once per second during a
    /// headless `dev:container` startup, and a truncating rewrite let the
    /// verify observe a torn input and refuse publication (#197). Atomic
    /// replacement keeps every read on one complete generation.
    #[test]
    fn concurrent_policy_resolutions_never_tear_admitted_compose_inputs() {
        let repo_root = generated_workspace_checkout("verify-tear");
        let home = tempfile::tempdir().expect("temporary fixture home");
        let containers_home = home.path().join(".effigy");
        // The writer thread resolves the policy through the same private home;
        // env (unlike a thread-local) reaches the spawned writer.
        let _env = crate::contract_test_support::EnvGuard::set_many(&[(
            "EFFIGY_TEST_CONTAINERS_HOME",
            Some(containers_home.display().to_string()),
        )]);
        effigy_containers::with_test_effigy_home(&containers_home, || {
            let policy = effigy_containers::load_container_policy(&repo_root, None)
                .expect("admission policy resolution");
            let context = super::capture_detached_activation_context(&repo_root, &policy)
                .expect("capture admitted activation context");
            context
                .verify(&repo_root, &policy, false)
                .expect("fresh context verifies before the race");

            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let writer_stop = stop.clone();
            let writer_root = repo_root.clone();
            let writer = std::thread::spawn(move || {
                while !writer_stop.load(std::sync::atomic::Ordering::Relaxed) {
                    effigy_containers::load_container_policy(&writer_root, None)
                        .expect("writer policy resolution");
                }
            });
            let mut clean_reads = 0usize;
            let failure = loop {
                match context.verify(&repo_root, &policy, false) {
                    Ok(()) => clean_reads += 1,
                    Err(error) => break Some(error),
                }
                if clean_reads >= 2000 {
                    break None;
                }
            };
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            writer.join().expect("writer joins");
            assert!(
                failure.is_none(),
                "publication verify raced a concurrent policy rewrite after {clean_reads} clean reads: {}",
                failure.expect("failure is set").to_string().replace('\n', " ")
            );
        });
    }
}
