use std::ffi::OsString;
use std::path::{Path, PathBuf};

use effigy_containers::ContainerAction;
use effigy_containers::EffectiveContainerPolicy;
use effigy_core::shell::shell_quote;
use effigy_env::secret::SecretString;
use effigy_manifest::{
    ManifestContainerSecretDelivery, ManifestSecretKeyConfig, ManifestSecretTarget,
    ManifestSecretsBackend,
};

use crate::runner::error::RunnerError;

const SECRETS_REQUIRED_ENV: &str = "EFFIGY_SECRETS_REQUIRED";
use crate::runner::exec_command::run_verified_compose_exec_plan;
use crate::runner::manifest::{load_task_manifest, TASK_MANIFEST_FILE};

#[derive(Debug, Clone)]
pub(in crate::runner) struct ResolvedContainerSecretRuntime {
    pub delivery: ManifestContainerSecretDelivery,
    pub env: Vec<(String, SecretString)>,
    /// One line per declared container key this run could not deliver: the
    /// key, why it was not delivered, and what to do about it. A bring-up that
    /// delivers nothing this way never writes an empty runtime file silently.
    pub warnings: Vec<String>,
}

impl ResolvedContainerSecretRuntime {
    fn delivered(
        delivery: ManifestContainerSecretDelivery,
        env: Vec<(String, SecretString)>,
        warnings: Vec<String>,
    ) -> Self {
        Self {
            delivery,
            env,
            warnings,
        }
    }

    /// Nothing to deliver. `reason` and `fix` describe one undeliverable key;
    /// every declared container key gets one warning line.
    fn undelivered(
        delivery: ManifestContainerSecretDelivery,
        keys: &[(&String, &ManifestSecretKeyConfig)],
        reason: &str,
        fix: &str,
    ) -> Self {
        Self::delivered(
            delivery,
            Vec::new(),
            keys.iter()
                .map(|(name, _)| {
                    format!("container secret `{name}` was not delivered: {reason}; {fix}")
                })
                .collect(),
        )
    }
}

// Written once so every undelivered-key warning names the same remedy the
// user can act on.
const UNLOCK_FIX: &str = "run `effigy secrets unlock` at a terminal, or rerun `effigy dev` on one";

/// The reason line for a run that holds a local-dev key but is not a `dev`
/// session: the key is a `dev` factor, so a direct command's warning says so
/// instead of claiming the vault is locked (effigy#1036).
const LOCAL_DEV_SCOPE_REASON: &str =
    "the local-dev key only unlocks `effigy dev`, and this run is not a dev session";
const LOCAL_DEV_SCOPE_FIX: &str =
    "for direct commands run `effigy secrets unlock` once at a terminal";

/// The environment variables the one apply delivers through `--env-file`.
/// Both `container up` and a task's environment ensure pass exactly these, so
/// the rendered config hash never flips between the two paths (effigy#371).
pub(in crate::runner) fn compose_env(
    runtime: &ResolvedContainerSecretRuntime,
) -> Vec<(String, OsString)> {
    match runtime.delivery {
        ManifestContainerSecretDelivery::ComposeEnv => runtime
            .env
            .iter()
            .map(|(key, value)| (key.clone(), OsString::from(value.expose())))
            .collect(),
        ManifestContainerSecretDelivery::RuntimeFiles => Vec::new(),
    }
}

/// Resolve the secrets a task's environment apply delivers, reporting keys it
/// could not deliver instead of failing the task. The resolved runtime is
/// returned whole: the activation writes its runtime files once the primary
/// service is verified, so a stack reused from a bring-up that delivered less
/// still receives what this session can resolve (effigy#1036).
pub(in crate::runner) fn resolve_secret_runtime_for_apply(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<ResolvedContainerSecretRuntime, RunnerError> {
    let runtime = resolve_container_secret_runtime(repo_root, policy, secrets_required())?;
    for warning in &runtime.warnings {
        eprintln!("[warn] {warning}");
    }
    Ok(runtime)
}

pub(super) fn secrets_required() -> bool {
    std::env::var(SECRETS_REQUIRED_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(in crate::runner) fn resolve_container_secret_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    force_unlock: bool,
) -> Result<ResolvedContainerSecretRuntime, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let delivery = policy.secret_delivery;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(ResolvedContainerSecretRuntime::delivered(
            delivery,
            Vec::new(),
            Vec::new(),
        ));
    };
    let container_keys = secrets
        .keys
        .iter()
        .filter(|(_, key)| key.targets.contains(&ManifestSecretTarget::Containers))
        .collect::<Vec<_>>();
    if container_keys.is_empty() {
        return Ok(ResolvedContainerSecretRuntime::delivered(
            delivery,
            Vec::new(),
            Vec::new(),
        ));
    }

    let required_names = container_keys
        .iter()
        .filter(|(_, key)| key.required)
        .map(|(name, _)| (*name).clone())
        .collect::<Vec<_>>();
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        if required_names.is_empty() {
            return Ok(ResolvedContainerSecretRuntime::undelivered(
                delivery,
                &container_keys,
                "`[secrets].backend` is not `\"effigy-vault\"`",
                "select the effigy-vault backend and store the value",
            ));
        }
        return Err(RunnerError::task_invocation(
            "required container secrets need `[secrets].backend = \"effigy-vault\"`",
        ));
    }

    let vault_path = crate::runner::secret_vault::resolve_shared_effigy_vault_path(
        repo_root,
        secrets,
        "container secret injection",
    )?;
    if !vault_path.exists() {
        if required_names.is_empty() {
            return Ok(ResolvedContainerSecretRuntime::undelivered(
                delivery,
                &container_keys,
                &format!("the vault is missing at {}", vault_path.display()),
                "run `effigy secrets init` and store the value",
            ));
        }
        return Err(RunnerError::task_invocation(format!(
            "required container secrets are declared but the vault is missing at {}",
            vault_path.display()
        )));
    }

    let optional_only = required_names.is_empty() && !force_unlock;
    // Every route a dev session can reach: the local-dev key, the passphrase
    // this session already holds (prompted, inherited from its parent, or
    // sealed in the local unlock credential), or one prompt now.
    let payload = if crate::runner::secret_session::local_dev_secret_access_active() {
        match crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path) {
            Ok(payload) => Some(payload),
            Err(local_dev_error) => {
                let Some(passphrase) =
                    crate::runner::secret_session::read_local_dev_upgrade_passphrase(
                        optional_only,
                        "Vault passphrase (one-time local-dev setup): ",
                        "local-dev container secrets need one passphrase unlock to create the unattended dev key, and secret input requires an interactive TTY",
                    )?
                else {
                    return Ok(ResolvedContainerSecretRuntime::undelivered(
                        delivery,
                        &container_keys,
                        &format!(
                            "the local-dev key could not read the vault ({}), and this run has no terminal to ask for its passphrase",
                            local_dev_error.human_error().what_failed()
                        ),
                        UNLOCK_FIX,
                    ));
                };
                let write = crate::runner::secret_vault::begin_vault_write(&vault_path)?;
                let payload = crate::runner::secret_vault::read_effigy_vault_payload(
                    &vault_path,
                    passphrase.expose(),
                )?;
                crate::runner::secret_vault::write_effigy_vault_payload(
                    &write,
                    &payload,
                    passphrase.expose(),
                )?;
                Some(payload)
            }
        }
    } else if optional_only {
        // Optional keys never block an interactive session on a prompt: what
        // this run already holds is delivered, and anything else is reported
        // instead of waiting on input nobody asked for (effigy#1036).
        match crate::runner::secret_session::held_vault_passphrase(&vault_path)? {
            Some(passphrase) => Some(crate::runner::secret_vault::read_effigy_vault_payload(
                &vault_path,
                passphrase.expose(),
            )?),
            None => None,
        }
    } else {
        match crate::runner::secret_session::read_vault_passphrase(
            &vault_path,
            false,
            "Vault passphrase: ",
            "container secrets require an unlocked vault passphrase and secret input requires an interactive TTY",
        )? {
            Some(passphrase) => Some(crate::runner::secret_vault::read_effigy_vault_payload(
                &vault_path,
                passphrase.expose(),
            )?),
            None => None,
        }
    };
    let Some(payload) = payload else {
        if crate::runner::secret_session::local_dev_key_present(&vault_path) {
            return Ok(ResolvedContainerSecretRuntime::undelivered(
                delivery,
                &container_keys,
                LOCAL_DEV_SCOPE_REASON,
                LOCAL_DEV_SCOPE_FIX,
            ));
        }
        return Ok(ResolvedContainerSecretRuntime::undelivered(
            delivery,
            &container_keys,
            "the vault is locked and this run has no terminal to ask for its passphrase",
            UNLOCK_FIX,
        ));
    };
    let mut injected = Vec::new();
    let mut warnings = Vec::new();
    let mut missing_required = Vec::new();
    for (name, key) in container_keys {
        match payload.records.get(name.as_str()) {
            Some(record) => injected.push((
                effigy_manifest::secret_env_name(name),
                SecretString::new(record.value.expose().to_owned()),
            )),
            None if key.required => missing_required.push(name.to_owned()),
            None => warnings.push(format!(
                "container secret `{name}` was not delivered: it is declared but not stored in the vault; run `effigy secrets set {name}`"
            )),
        }
    }
    if !missing_required.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "required container secret(s) missing from the vault: {}",
            missing_required.join(", ")
        )));
    }
    Ok(ResolvedContainerSecretRuntime::delivered(
        delivery, injected, warnings,
    ))
}

/// Whether this bring-up has anything to write into the runtime files. An
/// empty delivery is never written: a declared key that could not be delivered
/// already produced its own warning in [`resolve_container_secret_runtime`],
/// and writing `runtime.env` empty (or `runtime.json` as `{}`) would leave the
/// service reading "no secrets configured" with nothing said about it.
pub(in crate::runner) fn writes_runtime_secret_files(
    runtime: &ResolvedContainerSecretRuntime,
) -> bool {
    runtime.delivery == ManifestContainerSecretDelivery::RuntimeFiles && !runtime.env.is_empty()
}

/// Write this delivery's runtime files, and say what a service that is
/// already running will not pick up.
///
/// These files are read when a process starts, so a delivery that changes a
/// value under a live service leaves that service on the old one. The files are
/// read back first, so the warning names only the keys whose value actually
/// changed; the outstanding keys are then recorded with the service's start
/// identity, so every later invocation keeps reporting them until that service
/// restarts instead of the warning vanishing with the next apply
/// (effigy#1036). Warnings name keys, never values.
pub(in crate::runner) fn materialize_container_secret_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    runtime: &ResolvedContainerSecretRuntime,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<Vec<String>, RunnerError> {
    if !writes_runtime_secret_files(runtime) {
        return Ok(Vec::new());
    }
    let Some(runtime_dir) = policy.secret_runtime_dir.as_deref() else {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses `secrets.delivery = \"runtime-files\"` but no runtime_dir was configured",
            policy.name
        )));
    };
    let runtime_env_path = format!("{runtime_dir}/runtime.env");
    let runtime_json_path = format!("{runtime_dir}/runtime.json");

    // Read before rewriting: this is what the running service reads, and the
    // difference is what it must be told to reload. The recorded start
    // identity is what makes an outstanding warning survive later applies.
    let previous = read_primary_service_file(repo_root, policy, &runtime_json_path, binding)?;
    let observed = inspect_primary_service(repo_root, policy, binding, runtime_dir);
    let refresh_path = runtime_secret_refresh_path(repo_root, policy);
    let pending = refresh_path
        .as_deref()
        .and_then(recorded_runtime_secret_refresh);
    let outstanding = outstanding_refresh_keys(
        &observed.start,
        pending.as_ref(),
        previous.as_deref(),
        runtime,
    );

    run_primary_service_shell_command(
        repo_root,
        policy,
        &format!("mkdir -p {dir}", dir = shell_quote(runtime_dir)),
        "prepare container secret runtime dir",
        binding,
    )?;

    let runtime_env = TempSecretFile::write(
        "runtime.env",
        render_runtime_env_file(&runtime.env).as_bytes(),
    )?;
    let runtime_json = TempSecretFile::write(
        "runtime.json",
        render_runtime_json_file(&runtime.env).as_bytes(),
    )?;

    write_file_into_primary_service(
        repo_root,
        policy,
        &runtime_env.path,
        runtime_env_path.as_str(),
        "write container secret runtime env",
        binding,
    )?;
    write_file_into_primary_service(
        repo_root,
        policy,
        &runtime_json.path,
        runtime_json_path.as_str(),
        "write container secret runtime json",
        binding,
    )?;

    let workspace_user = policy.workspace_user.as_deref().unwrap_or("dev");
    run_primary_service_shell_command(
        repo_root,
        policy,
        &format!(
            "workspace_group=\"$(id -gn {user} 2>/dev/null || echo {user})\"; \
             chown {user}:\"$workspace_group\" {dir} {runtime_env} {runtime_json}; \
             chmod 700 {dir}; \
             chmod 600 {runtime_env} {runtime_json}",
            user = shell_quote(workspace_user),
            dir = shell_quote(runtime_dir),
            runtime_env = shell_quote(runtime_env_path.as_str()),
            runtime_json = shell_quote(runtime_json_path.as_str()),
        ),
        "finalize container secret runtime files",
        binding,
    )?;

    // Record, or retire, what the running service still owes: a warning that
    // vanished on the next apply would leave an operator who missed one line
    // believing the service had reloaded.
    if let Some(path) = refresh_path.as_deref() {
        persist_runtime_secret_refresh(path, &observed.start.container, &outstanding);
    }
    Ok(runtime_secret_refresh_warnings(
        policy,
        runtime_dir,
        &outstanding.keys,
        observed.storage,
    ))
}

/// The identity of the process that is running the primary service now: the
/// container and when its current process started, as the runtime reports it.
/// A restart changes the start time, which retires an outstanding warning.
/// `started_at` is `None` when the runtime did not report one, which is not the
/// same fact as "it started again": nothing retires on an absent start time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::runner) struct ContainerStartIdentity {
    pub container: String,
    pub started_at: Option<String>,
}

/// Whether the runtime directory keeps its files across a container restart.
/// A tmpfs is cleared by the restart that would reload the service, so the
/// remedy must not depend on the files surviving one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::runner) enum RuntimeDirStorage {
    /// A tmpfs inside the container: cleared whenever the container restarts.
    Ephemeral,
    /// A bind mount or volume: the files survive a restart.
    Persistent,
    /// The runtime did not say, or mounts the directory nowhere: the warning
    /// states both consequences instead of guessing.
    #[default]
    Unknown,
}

/// What one inspect of the running container says about a delivery.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::runner) struct PrimaryServiceObservation {
    pub start: ContainerStartIdentity,
    pub storage: RuntimeDirStorage,
}

fn inspect_primary_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
    runtime_dir: &str,
) -> PrimaryServiceObservation {
    let mut observation = PrimaryServiceObservation {
        start: ContainerStartIdentity {
            container: binding.id().to_owned(),
            started_at: None,
        },
        storage: RuntimeDirStorage::Unknown,
    };
    let Ok(output) = effigy_containers::exec::inspect_containers_for_policy(
        repo_root,
        policy,
        &[binding.id().to_owned()],
        None,
    ) else {
        return observation;
    };
    let Ok(records) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return observation;
    };
    let Some(record) = records
        .as_array()
        .and_then(|records| records.first())
        .cloned()
    else {
        return observation;
    };
    observation.start.started_at = record
        .get("State")
        .and_then(|state| state.get("StartedAt"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|started_at| !started_at.is_empty())
        .map(str::to_owned);
    observation.storage = runtime_dir_storage(&record, runtime_dir);
    observation
}

/// Whether `runtime_dir` is a tmpfs inside this container, a persistent mount,
/// or something the runtime did not describe.
fn runtime_dir_storage(record: &serde_json::Value, runtime_dir: &str) -> RuntimeDirStorage {
    let runtime_dir = runtime_dir.trim_end_matches('/');
    let covers = |destination: &str| {
        let destination = destination.trim_end_matches('/');
        !destination.is_empty()
            && (runtime_dir == destination
                || runtime_dir
                    .strip_prefix(destination)
                    .is_some_and(|rest| rest.starts_with('/')))
    };
    let mut storage = RuntimeDirStorage::Unknown;
    let tmpfs_keys = record
        .get("HostConfig")
        .and_then(|host| host.get("Tmpfs"))
        .and_then(serde_json::Value::as_object)
        .into_iter()
        .flat_map(|tmpfs| tmpfs.keys().map(String::as_str))
        .collect::<Vec<_>>();
    let tmpfs_mounts = record
        .get("Mounts")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|mount| mount.get("Type").and_then(serde_json::Value::as_str) == Some("tmpfs"))
        .filter_map(|mount| mount.get("Destination").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();
    let tmpfs = tmpfs_keys.into_iter().chain(tmpfs_mounts).any(covers);
    if tmpfs {
        return RuntimeDirStorage::Ephemeral;
    }
    let persistent = record
        .get("Mounts")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|mount| {
            matches!(
                mount.get("Type").and_then(serde_json::Value::as_str),
                Some("bind") | Some("volume")
            )
        })
        .filter_map(|mount| mount.get("Destination").and_then(serde_json::Value::as_str))
        .any(covers);
    if persistent {
        storage = RuntimeDirStorage::Persistent;
    }
    storage
}

/// The runtime file the primary service holds, read without failing when it is
/// absent: an unreadable file is "nothing delivered yet", which is what a
/// reused stack that never received this session's keys looks like.
fn read_primary_service_file(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    container_path: &str,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<Option<String>, RunnerError> {
    let command = format!("cat {} 2>/dev/null || true", shell_quote(container_path));
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        vec![
            OsString::from("exec"),
            OsString::from("-T"),
            OsString::from(policy.primary_service.as_str()),
            OsString::from("sh"),
            OsString::from("-lc"),
            OsString::from(command.clone()),
        ],
        ContainerAction::Exec,
        "read container secret runtime",
    )
    .map_err(RunnerError::from)?;
    let output = run_verified_compose_exec_plan(policy, &plan, binding, true, None, None)?;
    if !output.status.success() {
        return Ok(None);
    }
    let document = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok((!document.trim().is_empty()).then_some(document))
}

/// The keys a runtime-file delivery changes under a service that is already
/// running.
///
/// `previous` is the `runtime.json` the primary service holds, or `None` when
/// it holds none. A key is reported when its value differs, or when the
/// service holds no file at all; a delivery that writes exactly what the
/// service already reads reports nothing. The lines name keys and the restart
/// that loads them, never a value.
pub(in crate::runner) fn outstanding_refresh_keys(
    start: &ContainerStartIdentity,
    pending: Option<&RuntimeSecretRefresh>,
    held: Option<&str>,
    runtime: &ResolvedContainerSecretRuntime,
) -> OutstandingRefresh {
    let mut outstanding = OutstandingRefresh {
        keys: Vec::new(),
        started_at: start.started_at.clone(),
    };
    if runtime.env.is_empty() {
        return outstanding;
    }
    let note = |keys: &mut Vec<String>, key: &str| {
        if !keys.iter().any(|known| known == key) {
            keys.push(key.to_owned());
        }
    };
    // A warning raised while this same process was running still stands: the
    // service has not restarted since, so it still owes those keys. Retirement
    // needs proof of a restart, so an absent start time on either side keeps
    // the warning instead of quietly dropping it.
    if let Some(pending) = pending {
        let replaced = pending.container != start.container;
        let restarted = match (&pending.started_at, &start.started_at) {
            (Some(recorded), Some(current)) => recorded != current,
            _ => false,
        };
        if !replaced && !restarted {
            for key in &pending.keys {
                note(&mut outstanding.keys, key);
            }
            outstanding.started_at = match (&pending.started_at, &start.started_at) {
                // The runtime reports nothing now: keep the anchor the record
                // already had, so the restart that finally happens is provable.
                (Some(recorded), None) => Some(recorded.clone()),
                // Adopt the first start time the runtime does report as the
                // baseline: a later change is then provable.
                (None, Some(current)) => Some(current.clone()),
                (recorded, _) => recorded.clone(),
            };
        }
    }
    let held_document: serde_json::Map<String, serde_json::Value> = held
        .and_then(|document| serde_json::from_str(document).ok())
        .unwrap_or_default();
    for (key, value) in &runtime.env {
        let changed = held_document
            .get(key)
            .and_then(serde_json::Value::as_str)
            .is_none_or(|held| held != value.expose());
        if changed {
            note(&mut outstanding.keys, key);
        }
    }
    outstanding
}

/// The keys a running service still owes, and the start time the record is
/// anchored to when it is written back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::runner) struct OutstandingRefresh {
    pub keys: Vec<String>,
    pub started_at: Option<String>,
}

/// What a running service still owes after a runtime-file delivery, recorded
/// with the start identity it belongs to: keys only, never values.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(in crate::runner) struct RuntimeSecretRefresh {
    pub container: String,
    /// The start time this record is anchored to. `None` means the runtime did
    /// not report one when the record was written; nothing retires the record
    /// on an absent start time.
    pub started_at: Option<String>,
    pub keys: Vec<String>,
}

/// Where an outstanding refresh for this environment is recorded. It lives
/// beside the checkout's runtime state, so a later invocation reports it
/// without consulting the service again.
fn runtime_secret_refresh_path(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Option<PathBuf> {
    let state = repo_root.join(".effigy/runtime/container-secrets");
    Some(state.join(format!("{}.refresh.json", policy.project_name)))
}

fn recorded_runtime_secret_refresh(path: &Path) -> Option<RuntimeSecretRefresh> {
    let document = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&document).ok()
}

/// Record the outstanding keys, or retire the record when nothing is owed. A
/// state directory that cannot be written costs the next invocation's reminder,
/// never this delivery.
fn persist_runtime_secret_refresh(path: &Path, container: &str, outstanding: &OutstandingRefresh) {
    if outstanding.keys.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let record = RuntimeSecretRefresh {
        container: container.to_owned(),
        started_at: outstanding.started_at.clone(),
        keys: outstanding.keys.clone(),
    };
    if let Ok(document) = serde_json::to_string_pretty(&record) {
        let _ = std::fs::write(path, document);
    }
}

/// The one line that tells an operator what a running service still owes.
///
/// The remedy has to hold for the files' storage: a tmpfs under `runtime_dir`
/// is cleared by any container restart, so recommending one would undo the
/// delivery and repeat the problem. Every form names the reload that keeps the
/// files, and the resolving command a recreate needs afterwards.
pub(in crate::runner) fn runtime_secret_refresh_warnings(
    policy: &EffectiveContainerPolicy,
    runtime_dir: &str,
    outstanding: &[String],
    storage: RuntimeDirStorage,
) -> Vec<String> {
    if outstanding.is_empty() {
        return Vec::new();
    }
    let service = policy.primary_service.as_str();
    let remedy = match storage {
        RuntimeDirStorage::Ephemeral => format!(
            "`{runtime_dir}` is a tmpfs in this container, so restarting it clears these files: reload `{service}`'s own process inside the running container, and rerun `effigy dev` or the command that resolved these keys after any restart"
        ),
        RuntimeDirStorage::Persistent => format!(
            "restart `{service}` in place so it reads them (a stack recreate starts it before these files exist again, so rerun `effigy dev` or the command that resolved these keys after one)"
        ),
        RuntimeDirStorage::Unknown => format!(
            "reload `{service}`'s own process inside the running container; if `{runtime_dir}` is a tmpfs, any restart clears these files, so rerun `effigy dev` or the command that resolved these keys after one"
        ),
    };
    vec![format!(
        "container secret(s) {} were written to {runtime_dir}/runtime.json after `{service}` started, so `{service}` keeps the values it read until it reloads them, and this warning stays until it does; {remedy}",
        outstanding.join(", "),
    )]
}

pub(in crate::runner) fn container_secret_runtime_env_path(
    policy: &EffectiveContainerPolicy,
) -> Option<String> {
    if policy.secret_delivery != ManifestContainerSecretDelivery::RuntimeFiles
        || !policy.source_secret_runtime_for_deferrals
    {
        return None;
    }
    policy
        .secret_runtime_dir
        .as_ref()
        .map(|dir| format!("{dir}/runtime.env"))
}

fn render_runtime_env_file(entries: &[(String, SecretString)]) -> String {
    let mut rendered = String::new();
    for (key, value) in entries {
        rendered.push_str(key);
        rendered.push('=');
        rendered.push_str(&shell_quote(value.expose()));
        rendered.push('\n');
    }
    rendered
}

fn render_runtime_json_file(entries: &[(String, SecretString)]) -> String {
    let mut payload = serde_json::Map::new();
    for (key, value) in entries {
        payload.insert(
            key.clone(),
            serde_json::Value::String(value.expose().to_owned()),
        );
    }
    serde_json::to_string_pretty(&payload).expect("serialize container secret runtime json")
}

fn run_primary_service_shell_command(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    command: &str,
    label: &str,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        vec![
            OsString::from("exec"),
            OsString::from("-T"),
            OsString::from(policy.primary_service.as_str()),
            OsString::from("sh"),
            OsString::from("-lc"),
            OsString::from(command),
        ],
        ContainerAction::Exec,
        label,
    )
    .map_err(RunnerError::from)?;
    let output = run_verified_compose_exec_plan(policy, &plan, binding, true, None, None)?;
    if output.status.success() {
        return Ok(());
    }
    Err(RunnerError::TaskCommandFailure {
        command: command.to_owned(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

fn write_file_into_primary_service(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    host_source: &Path,
    container_dest: &str,
    label: &str,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        vec![
            OsString::from("exec"),
            OsString::from("-T"),
            OsString::from(policy.primary_service.as_str()),
            OsString::from("sh"),
            OsString::from("-lc"),
            OsString::from(format!("cat > {}", shell_quote(container_dest))),
        ],
        ContainerAction::Exec,
        label,
    )
    .map_err(RunnerError::from)?;
    let output =
        run_verified_compose_exec_plan(policy, &plan, binding, true, Some(host_source), None)?;
    if output.status.success() {
        return Ok(());
    }
    Err(RunnerError::TaskCommandFailure {
        command: label.to_owned(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Host-side staging for one secret runtime file. Created exclusively with an
/// unpredictable name and owner-only mode before any secret byte is written;
/// removed on drop.
struct TempSecretFile {
    // Held so the file lives until the copy completes.
    _file: tempfile::NamedTempFile,
    path: PathBuf,
}

impl TempSecretFile {
    fn write(label: &str, bytes: &[u8]) -> Result<Self, RunnerError> {
        use std::io::Write;
        let failure = |error: std::io::Error| {
            RunnerError::task_invocation(format!(
                "failed to write temporary container secret file: {error}"
            ))
        };
        let mut file = tempfile::Builder::new()
            .prefix(&format!("effigy-container-secret-runtime-{label}-"))
            .tempfile()
            .map_err(failure)?;
        file.write_all(bytes).map_err(failure)?;
        file.flush().map_err(failure)?;
        let path = file.path().to_path_buf();
        Ok(Self { _file: file, path })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        container_secret_runtime_env_path, materialize_container_secret_runtime,
        outstanding_refresh_keys, recorded_runtime_secret_refresh, render_runtime_env_file,
        render_runtime_json_file, resolve_container_secret_runtime, runtime_dir_storage,
        runtime_secret_refresh_path, runtime_secret_refresh_warnings, writes_runtime_secret_files,
        ContainerStartIdentity, ResolvedContainerSecretRuntime, RuntimeDirStorage,
        RuntimeSecretRefresh,
    };
    use effigy_containers::{EffectiveComposeSource, EffectiveContainerPolicy};
    use effigy_env::secret::SecretString;
    use effigy_manifest::ManifestContainerSecretDelivery;
    use effigy_secrets::{SecretValue, VaultPlaintextPayload, VaultSecretRecord};
    use std::fs;
    use std::path::{Path, PathBuf};

    #[test]
    fn container_secret_env_resolves_declared_container_target_values() {
        let root = temp_repo("container-secret-env");
        fs::write(
            root.join("effigy.toml"),
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["containers"]

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
"#,
        )
        .expect("write manifest");
        write_test_vault(
            &root,
            "vault-passphrase",
            &[("database_url", "postgres://secret-value")],
        );
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let runtime = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), false)
            .expect("resolve secrets");

        assert_eq!(
            runtime.delivery,
            ManifestContainerSecretDelivery::RuntimeFiles
        );
        assert_eq!(runtime.env.len(), 1);
        assert_eq!(runtime.env[0].0, "DATABASE_URL");
        assert_eq!(runtime.env[0].1.expose(), "postgres://secret-value");
        assert!(runtime.warnings.is_empty());
        assert!(writes_runtime_secret_files(&runtime));
    }

    #[test]
    fn container_secret_env_blocks_missing_required_before_startup() {
        let root = temp_repo("container-secret-missing");
        fs::write(
            root.join("effigy.toml"),
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.database_url]
required = true
targets = ["containers"]

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
"#,
        )
        .expect("write manifest");
        write_test_vault(&root, "vault-passphrase", &[]);
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let error = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), false)
            .expect_err("missing should fail");

        assert!(error
            .to_string()
            .contains("required container secret(s) missing from the vault"));
    }

    #[test]
    fn container_secret_env_forced_unlock_loads_optional_container_values() {
        for present in [false, true] {
            let root = temp_repo("container-secret-force-required");
            fs::write(
                root.join("effigy.toml"),
                r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.api_token]
required = false
targets = ["containers"]

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
"#,
            )
            .expect("write manifest");
            write_test_vault(
                &root,
                "vault-passphrase",
                if present {
                    &[("api_token", "tok_secret")]
                } else {
                    &[]
                },
            );
            let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

            let runtime =
                resolve_container_secret_runtime(&root, &test_runtime_files_policy(), true)
                    .expect("resolve secrets");

            assert_eq!(
                runtime
                    .env
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.expose()))
                    .collect::<Vec<_>>(),
                if present {
                    vec![("API_TOKEN", "tok_secret")]
                } else {
                    vec![]
                }
            );
            assert_eq!(writes_runtime_secret_files(&runtime), present);
            assert_eq!(runtime.warnings.is_empty(), present);
        }
    }

    /// An optional-only project must still receive its values: the vault a dev
    /// session opened with the local-dev key is enough, with no passphrase in
    /// sight.
    #[test]
    fn container_secret_runtime_dev_key_session_delivers_optional_values() {
        let root = temp_repo("container-secret-dev-key");
        write_optional_only_container_manifest(&root);
        write_dev_key_vault(
            &root,
            &[
                ("openai_api_key", "sk-dev-value"),
                ("recaptcha_secret", "rc-dev-value"),
                ("mailchimp_api_key", "mc-dev-value"),
            ],
        );
        let _test_passphrase = ScopedEnvVar::remove("EFFIGY_TEST_SECRETS_PASSPHRASE");
        let _internal_passphrase = ScopedEnvVar::remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE");
        crate::runner::secret_session::forget_session_passphrase_for_tests();
        let _local_dev = crate::runner::secret_session::activate_local_dev_secret_access(true);

        let runtime = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), false)
            .expect("a dev-key session resolves the optional container values");

        assert!(
            runtime.warnings.is_empty(),
            "unexpected warnings: {}",
            runtime.warnings.join(" | ")
        );
        assert_eq!(
            runtime
                .env
                .iter()
                .map(|(key, value)| (key.as_str(), value.expose()))
                .collect::<std::collections::BTreeMap<_, _>>(),
            std::collections::BTreeMap::from([
                ("OPENAI_API_KEY", "sk-dev-value"),
                ("RECAPTCHA_SECRET", "rc-dev-value"),
                ("MAILCHIMP_API_KEY", "mc-dev-value"),
            ])
        );
        assert!(
            writes_runtime_secret_files(&runtime),
            "a delivered runtime must be written"
        );
    }

    /// A declared key that cannot be delivered says which key and why, names
    /// the fix, and never reaches the runtime files as an empty value.
    #[test]
    fn container_secret_runtime_warns_for_each_undelivered_optional_key() {
        let root = temp_repo("container-secret-undeliverable");
        write_optional_only_container_manifest(&root);
        write_test_vault(&root, "vault-passphrase", &[]);
        let _test_passphrase = ScopedEnvVar::remove("EFFIGY_TEST_SECRETS_PASSPHRASE");
        let _internal_passphrase = ScopedEnvVar::remove("EFFIGY_INTERNAL_SECRET_PASSPHRASE");
        crate::runner::secret_session::forget_session_passphrase_for_tests();

        let runtime = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), false)
            .expect("optional keys must not fail the bring-up");

        assert!(runtime.env.is_empty());
        assert!(
            !writes_runtime_secret_files(&runtime),
            "an undelivered runtime must not be written empty"
        );
        assert_eq!(
            runtime.warnings.len(),
            3,
            "{}",
            runtime.warnings.join(" | ")
        );
        for name in ["openai_api_key", "recaptcha_secret", "mailchimp_api_key"] {
            let warning = runtime
                .warnings
                .iter()
                .find(|warning| warning.contains(name))
                .unwrap_or_else(|| {
                    panic!(
                        "no warning names `{name}`: {}",
                        runtime.warnings.join(" | ")
                    )
                });
            assert!(warning.contains("was not delivered"), "{warning}");
            assert!(warning.contains("locked"), "{warning}");
            assert!(warning.contains("effigy secrets unlock"), "{warning}");
        }
    }

    /// A machine that holds the local-dev key but is running a direct command
    /// says exactly that: the key unlocks `effigy dev`, not this run, and
    /// `effigy secrets unlock` is the fix. The vault is not "locked" for a
    /// machine that can read it (effigy#1036).
    #[test]
    fn container_secret_runtime_names_the_local_dev_scope_for_a_direct_command() {
        let root = temp_repo("container-secret-local-dev-scope");
        write_optional_only_container_manifest(&root);
        write_dev_key_vault(&root, &[("openai_api_key", "sk-dev-value")]);
        let _test_passphrase = ScopedEnvVar::remove("EFFIGY_TEST_SECRETS_PASSPHRASE");
        let _public_passphrase = ScopedEnvVar::remove("EFFIGY_SECRET_PASSPHRASE");
        crate::runner::secret_session::forget_session_passphrase_for_tests();
        assert!(
            crate::runner::secret_session::local_dev_key_present(
                &root.join(".effigy/secrets/local.vault")
            ),
            "the fixture holds a local-dev key"
        );

        let runtime = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), false)
            .expect("optional keys must not fail a direct command");

        assert!(runtime.env.is_empty());
        assert!(!writes_runtime_secret_files(&runtime));
        assert!(
            !runtime.warnings.is_empty(),
            "a direct command reports what it could not deliver"
        );
        for warning in &runtime.warnings {
            assert!(
                warning.contains("the local-dev key only unlocks `effigy dev`"),
                "{warning}"
            );
            assert!(
                warning.contains("run `effigy secrets unlock` once at a terminal"),
                "{warning}"
            );
            assert!(
                !warning.contains("the vault is locked"),
                "a vault this machine can read is not reported locked: {warning}"
            );
        }
    }

    #[test]
    fn container_secret_runtime_warns_for_a_declared_key_the_vault_does_not_store() {
        let root = temp_repo("container-secret-unstored");
        write_optional_only_container_manifest(&root);
        write_test_vault(&root, "vault-passphrase", &[("unrelated", "hidden")]);
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let runtime = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), false)
            .expect("optional keys must not fail the bring-up");

        assert!(runtime.env.is_empty());
        assert!(!writes_runtime_secret_files(&runtime));
        assert_eq!(
            runtime.warnings.len(),
            3,
            "{}",
            runtime.warnings.join(" | ")
        );
        for key in ["openai_api_key", "recaptcha_secret", "mailchimp_api_key"] {
            assert!(runtime.warnings.iter().any(|warning| warning.contains(key)
                && warning.contains("not stored in the vault")
                && warning.contains(&format!("effigy secrets set {key}"))));
        }
    }

    #[test]
    fn container_secret_runtime_env_path_is_opt_in() {
        let mut policy = test_runtime_files_policy();

        assert_eq!(
            container_secret_runtime_env_path(&policy).as_deref(),
            Some("/run/effigy/secrets/runtime.env")
        );
        policy.source_secret_runtime_for_deferrals = false;
        assert_eq!(container_secret_runtime_env_path(&policy), None);
        policy.source_secret_runtime_for_deferrals = true;
        policy.secret_runtime_dir = None;
        assert_eq!(container_secret_runtime_env_path(&policy), None);
    }

    #[test]
    fn container_secret_runtime_env_path_skips_non_runtime_file_delivery() {
        let mut policy = test_runtime_files_policy();
        policy.secret_delivery = ManifestContainerSecretDelivery::ComposeEnv;

        assert_eq!(container_secret_runtime_env_path(&policy), None);
    }

    #[test]
    fn container_secret_runtime_env_file_uses_shell_quoted_assignments() {
        let rendered = render_runtime_env_file(&[(
            "API_TOKEN".to_owned(),
            SecretString::new("tok'en value".to_owned()),
        )]);

        assert_eq!(rendered, "API_TOKEN='tok'\"'\"'en value'\n");
    }

    #[test]
    fn container_secret_runtime_json_file_serializes_key_value_map() {
        let rendered = render_runtime_json_file(&[(
            "API_TOKEN".to_owned(),
            SecretString::new("tok'en\\value".to_owned()),
        )]);

        assert!(rendered.contains("\"API_TOKEN\": \"tok'en\\\\value\""));
    }

    /// A delivery names the keys the running service must reload, keeps
    /// naming them while that service has not restarted, and stops once it
    /// has. No line carries a value (effigy#1036).
    #[test]
    fn a_runtime_file_delivery_names_the_keys_a_running_service_still_owes() {
        let policy = test_runtime_files_policy();
        let runtime = ResolvedContainerSecretRuntime {
            delivery: ManifestContainerSecretDelivery::RuntimeFiles,
            env: vec![
                (
                    "OPENAI_API_KEY".to_owned(),
                    SecretString::new("sk-new-value".to_owned()),
                ),
                (
                    "UNCHANGED_KEY".to_owned(),
                    SecretString::new("kept-value".to_owned()),
                ),
            ],
            warnings: Vec::new(),
        };
        let running = ContainerStartIdentity {
            container: "c".repeat(64),
            started_at: Some("2026-11-18T10:00:00Z".to_owned()),
        };
        let delivered = r#"{"OPENAI_API_KEY":"sk-new-value","UNCHANGED_KEY":"kept-value"}"#;

        // The service holds the same unchanged value and a stale openai key.
        let held = r#"{"OPENAI_API_KEY":"sk-old-value","UNCHANGED_KEY":"kept-value"}"#;
        let owed = outstanding_refresh_keys(&running, None, Some(held), &runtime);
        assert_eq!(owed.keys, vec!["OPENAI_API_KEY".to_owned()]);
        let warnings = runtime_secret_refresh_warnings(
            &policy,
            "/run/effigy/secrets",
            &owed.keys,
            RuntimeDirStorage::Persistent,
        );
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("OPENAI_API_KEY"), "{}", warnings[0]);
        assert!(!warnings[0].contains("sk-new-value"), "{}", warnings[0]);
        assert!(!warnings[0].contains("sk-old-value"), "{}", warnings[0]);
        assert!(
            !warnings[0].contains("UNCHANGED_KEY"),
            "a value the service already reads is not reported: {}",
            warnings[0]
        );
        assert!(
            warnings[0].contains("stays until it does"),
            "the warning says it persists: {}",
            warnings[0]
        );
        assert!(
            warnings[0].contains("restart `app` in place"),
            "the remedy keeps the delivery: {}",
            warnings[0]
        );
        assert!(
            !warnings[0].contains("effigy container web down"),
            "a stack recreate would repeat the original problem: {}",
            warnings[0]
        );

        // The very same delivery again, with the service still running, owes
        // the same key: the warning does not vanish on the next apply.
        let record = RuntimeSecretRefresh {
            container: running.container.clone(),
            started_at: running.started_at.clone(),
            keys: owed.keys.clone(),
        };
        let again = outstanding_refresh_keys(&running, Some(&record), Some(delivered), &runtime);
        assert_eq!(again.keys, vec!["OPENAI_API_KEY".to_owned()]);
        assert_eq!(
            again.started_at,
            running.started_at.clone(),
            "the record keeps the start time it is anchored to"
        );

        // A start time the runtime cannot report never retires the record: an
        // unobservable restart is not a restart (effigy#1036 review).
        let unobservable = ContainerStartIdentity {
            container: running.container.clone(),
            started_at: None,
        };
        let blind =
            outstanding_refresh_keys(&unobservable, Some(&record), Some(delivered), &runtime);
        assert_eq!(
            blind.keys,
            vec!["OPENAI_API_KEY".to_owned()],
            "an absent start time keeps the outstanding warning"
        );
        assert_eq!(
            blind.started_at,
            running.started_at.clone(),
            "and keeps the anchor it already had"
        );

        // A record written while the start time was unknown adopts the first
        // one the runtime reports, and still owes its keys.
        let unanchored = RuntimeSecretRefresh {
            container: running.container.clone(),
            started_at: None,
            keys: vec!["OPENAI_API_KEY".to_owned()],
        };
        let adopted =
            outstanding_refresh_keys(&running, Some(&unanchored), Some(delivered), &runtime);
        assert_eq!(adopted.keys, vec!["OPENAI_API_KEY".to_owned()]);
        assert_eq!(adopted.started_at, running.started_at.clone());

        // Once that service restarts, nothing is owed: the file now carries
        // exactly what the delivery writes, and the old warning is retired.
        let restarted = ContainerStartIdentity {
            container: running.container.clone(),
            started_at: Some("2026-11-18T11:03:00Z".to_owned()),
        };
        assert!(
            outstanding_refresh_keys(&restarted, Some(&record), Some(delivered), &runtime)
                .keys
                .is_empty(),
            "a restarted service owes nothing for values it already reads"
        );

        // A replaced container is not a restart of the process that read the
        // files: the new one owes whatever this delivery changed for it.
        let replaced = ContainerStartIdentity {
            container: "d".repeat(64),
            started_at: Some("2026-11-18T11:03:00Z".to_owned()),
        };
        assert!(
            outstanding_refresh_keys(&replaced, Some(&record), Some(delivered), &runtime)
                .keys
                .is_empty()
        );

        // A service holding no file at all owes every delivered key.
        let absent = outstanding_refresh_keys(&running, None, None, &runtime);
        assert_eq!(absent.keys.len(), 2, "{absent:?}");
        assert!(absent.keys.contains(&"OPENAI_API_KEY".to_owned()));
        assert!(absent.keys.contains(&"UNCHANGED_KEY".to_owned()));

        // An empty delivery has nothing to say.
        let empty = ResolvedContainerSecretRuntime {
            delivery: ManifestContainerSecretDelivery::RuntimeFiles,
            env: Vec::new(),
            warnings: Vec::new(),
        };
        assert!(
            outstanding_refresh_keys(&running, Some(&record), None, &empty)
                .keys
                .is_empty()
        );
    }

    /// The remedy must hold for the files' storage: a tmpfs is cleared by the
    /// restart that would reload the service, so the warning never asks for one
    /// there, and every form names the resolving command a recreate needs
    /// (effigy#1036 review).
    #[test]
    fn a_runtime_file_refresh_remedy_matches_where_the_files_live() {
        let policy = test_runtime_files_policy();
        let keys = vec!["MAILCHIMP_API_KEY".to_owned()];
        let line = |storage| {
            runtime_secret_refresh_warnings(&policy, "/run/effigy/secrets", &keys, storage)
                .remove(0)
        };

        let ephemeral = line(RuntimeDirStorage::Ephemeral);
        assert!(ephemeral.contains("tmpfs"), "{ephemeral}");
        assert!(
            ephemeral.contains("restarting it clears these files"),
            "{ephemeral}"
        );
        assert!(
            ephemeral.contains("reload `app`'s own process"),
            "{ephemeral}"
        );
        assert!(
            ephemeral.contains("rerun `effigy dev`"),
            "a tmpfs restart needs the resolving command again: {ephemeral}"
        );
        assert!(
            !ephemeral.contains("restart `app` in place"),
            "a restart would clear a tmpfs: {ephemeral}"
        );

        let persistent = line(RuntimeDirStorage::Persistent);
        assert!(
            persistent.contains("restart `app` in place"),
            "{persistent}"
        );
        assert!(persistent.contains("rerun `effigy dev`"), "{persistent}");

        let unknown = line(RuntimeDirStorage::Unknown);
        assert!(
            unknown.contains("if `/run/effigy/secrets` is a tmpfs"),
            "{unknown}"
        );
        assert!(unknown.contains("rerun `effigy dev`"), "{unknown}");

        for line in [ephemeral, persistent, unknown] {
            assert!(
                !line.contains("mc-"),
                "a remedy never carries a value: {line}"
            );
            assert!(
                !line.contains("effigy container web down"),
                "no form recommends a recreate as the remedy: {line}"
            );
        }
    }

    /// The storage a delivery finds, from the inspect record: a tmpfs at the
    /// runtime directory is ephemeral even under a persistent parent, and a
    /// bind or volume is persistent.
    #[test]
    fn runtime_dir_storage_reads_the_container_mounts() {
        let record = serde_json::json!({
            "HostConfig": {"Tmpfs": {"/run/effigy/secrets": ""}},
            "Mounts": [{"Type": "bind", "Destination": "/workspace"}],
        });
        assert_eq!(
            runtime_dir_storage(&record, "/run/effigy/secrets"),
            RuntimeDirStorage::Ephemeral
        );
        assert_eq!(
            runtime_dir_storage(&record, "/run/effigy/secrets/nested"),
            RuntimeDirStorage::Ephemeral
        );

        let record = serde_json::json!({
            "Mounts": [{"Type": "volume", "Destination": "/run/effigy/secrets"}],
        });
        assert_eq!(
            runtime_dir_storage(&record, "/run/effigy/secrets"),
            RuntimeDirStorage::Persistent
        );

        let record = serde_json::json!({"Mounts": []});
        assert_eq!(
            runtime_dir_storage(&record, "/run/effigy/secrets"),
            RuntimeDirStorage::Unknown
        );

        // A different tmpfs does not describe this directory.
        let record = serde_json::json!({
            "HostConfig": {"Tmpfs": {"/tmp": ""}},
            "Mounts": [],
        });
        assert_eq!(
            runtime_dir_storage(&record, "/run/effigy/secrets"),
            RuntimeDirStorage::Unknown
        );
    }

    /// A `dev` session that borrows a stack a direct `container up` started
    /// writes the keys it resolved into the running service's runtime files,
    /// and tells that service which of them it must reload: the reused service
    /// here is a real container inventory with an exec that lands on disk
    /// (effigy#1036).
    #[test]
    fn a_reused_running_service_receives_the_resolved_keys_and_is_told_to_reload() {
        let _test = crate::contract_test_support::lock_test();
        let fixture = RunningServiceFixture::new("reused-runtime-files");
        let policy = fixture.policy();
        let runtime_dir = fixture.runtime_dir();
        assert_eq!(
            policy.secret_delivery,
            ManifestContainerSecretDelivery::RuntimeFiles
        );
        assert_eq!(
            policy.secret_runtime_dir.as_deref(),
            Some(runtime_dir.as_str())
        );

        // The stack a direct `container up` started holds an unrelated key and
        // none of this session's optional ones.
        fs::write(
            format!("{runtime_dir}/runtime.json"),
            r#"{"OTHER_KEY":"other-value"}"#,
        )
        .expect("seed the reused service's runtime file");

        let runtime = ResolvedContainerSecretRuntime {
            delivery: ManifestContainerSecretDelivery::RuntimeFiles,
            env: vec![(
                "MAILCHIMP_API_KEY".to_owned(),
                SecretString::new("mc-session-value".to_owned()),
            )],
            warnings: Vec::new(),
        };

        let binding = crate::runner::container_runtime_prep::ensure_primary_service_binding(
            &fixture.repo_root,
            &policy,
            None,
        )
        .expect("the running service binds");
        let warnings =
            materialize_container_secret_runtime(&fixture.repo_root, &policy, &runtime, &binding)
                .expect("the reused service receives the delivery");

        let delivered = fs::read_to_string(format!("{runtime_dir}/runtime.json"))
            .expect("the service's runtime file");
        assert!(
            delivered.contains("MAILCHIMP_API_KEY"),
            "the reused service received the session's key: {delivered}"
        );
        let env_file =
            fs::read_to_string(format!("{runtime_dir}/runtime.env")).expect("runtime env file");
        assert!(env_file.contains("MAILCHIMP_API_KEY="), "{env_file}");

        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("MAILCHIMP_API_KEY"), "{}", warnings[0]);
        assert!(
            warnings[0].contains("restart `app` in place"),
            "a persistent runtime directory reloads by restarting in place: {}",
            warnings[0]
        );
        assert!(
            !warnings[0].contains("mc-session-value"),
            "a warning never carries a value: {}",
            warnings[0]
        );

        // The same delivery again, with the service still running, keeps the
        // reminder: the values are on disk but this process never read them.
        let again =
            materialize_container_secret_runtime(&fixture.repo_root, &policy, &runtime, &binding)
                .expect("the same delivery again");
        assert_eq!(again.len(), 1, "{again:?}");
        assert!(again[0].contains("MAILCHIMP_API_KEY"), "{}", again[0]);
        let record = fixture.refresh_record();
        assert_eq!(
            record.keys,
            vec!["MAILCHIMP_API_KEY".to_owned()],
            "the outstanding keys are recorded with the service's start identity"
        );

        // The runtime stops reporting a start time: nothing proves a restart,
        // so the warning stays rather than retiring on an absent fact.
        fixture.hide_the_start_time();
        let unobservable =
            materialize_container_secret_runtime(&fixture.repo_root, &policy, &runtime, &binding)
                .expect("a delivery without a start time");
        assert_eq!(unobservable.len(), 1, "{unobservable:?}");
        assert!(
            unobservable[0].contains("MAILCHIMP_API_KEY"),
            "{}",
            unobservable[0]
        );

        // A tmpfs runtime directory changes the remedy: a restart clears it.
        fixture.mount_the_runtime_dir_as_tmpfs();
        let tmpfs =
            materialize_container_secret_runtime(&fixture.repo_root, &policy, &runtime, &binding)
                .expect("a delivery into a tmpfs runtime directory");
        assert_eq!(tmpfs.len(), 1, "{tmpfs:?}");
        assert!(tmpfs[0].contains("tmpfs"), "{}", tmpfs[0]);
        assert!(
            !tmpfs[0].contains("restart `app` in place"),
            "a restart would clear a tmpfs: {}",
            tmpfs[0]
        );

        // The service restarts: the recorded warning retires and the next
        // delivery of the same values says nothing.
        fixture.restart_the_service("2026-11-18T12:00:00Z");
        let after_restart =
            materialize_container_secret_runtime(&fixture.repo_root, &policy, &runtime, &binding)
                .expect("a delivery after the restart");
        assert!(after_restart.is_empty(), "{after_restart:?}");
        assert!(
            fixture.refresh_record_absent(),
            "a restarted service owes nothing"
        );
    }

    /// A running container this checkout owns, with a docker that proves the
    /// inventory and an exec that runs against the fixture on disk.
    struct RunningServiceFixture {
        repo_root: PathBuf,
        runtime_dir: String,
        started_at: PathBuf,
        mounts: PathBuf,
        _env: crate::contract_test_support::EnvGuard,
    }

    impl RunningServiceFixture {
        /// When the fake container's current process started, as the runtime
        /// reports it: the identity a restart changes.
        fn restart_the_service(&self, started_at: &str) {
            fs::write(&self.started_at, started_at).expect("record the restart");
        }

        /// A runtime that reports no start time for the container.
        fn hide_the_start_time(&self) {
            fs::remove_file(&self.started_at).expect("withhold the start time");
        }

        /// The container mounts the runtime directory as a tmpfs.
        fn mount_the_runtime_dir_as_tmpfs(&self) {
            fs::write(
                &self.mounts,
                format!(
                    r#"[{{"Type":"tmpfs","Destination":"{}"}}]"#,
                    self.runtime_dir
                ),
            )
            .expect("record the tmpfs mount");
        }

        fn refresh_record(&self) -> RuntimeSecretRefresh {
            recorded_runtime_secret_refresh(&self.refresh_path())
                .expect("the outstanding refresh is recorded")
        }

        fn refresh_record_absent(&self) -> bool {
            !self.refresh_path().exists()
        }

        fn refresh_path(&self) -> PathBuf {
            runtime_secret_refresh_path(&self.repo_root, &self.policy())
                .expect("the fixture records its refresh state")
        }

        fn new(name: &str) -> Self {
            let root = temp_repo(name);
            let repo_root = root.join("repo");
            fs::create_dir_all(&repo_root).expect("mkdir repo");
            let runtime_dir = root.join("container/secrets");
            fs::create_dir_all(&runtime_dir).expect("mkdir container runtime dir");
            fs::write(
                repo_root.join("effigy.toml"),
                format!(
                    r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.mailchimp_api_key]
required = false
targets = ["containers"]

[containers]
default = "web"

[containers.web]
compose_file = "docker-compose.yml"
primary_service = "app"
project_name = "started-web"
working_dir = "/workspace"

[containers.web.secrets]
delivery = "runtime-files"
runtime_dir = "{runtime}"
"#,
                    runtime = runtime_dir.display()
                ),
            )
            .expect("write manifest");
            fs::write(
                repo_root.join("docker-compose.yml"),
                "services:
  app:
    image: alpine:latest
",
            )
            .expect("write compose");

            let bin = root.join("bin");
            fs::create_dir_all(&bin).expect("mkdir bin");
            let log = root.join("docker.log");
            let docker = bin.join("docker");
            fs::write(
                &docker,
                r#"#!/bin/sh
printf '%s
' "$*" >> "$EFFIGY_TEST_DOCKER_LOG"
for arg in "$@"; do
  if [ "$arg" = "--format" ]; then
    printf '%s	Up 1 minute		%s	%s	app	False
' "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
  fi
done
case "$1" in
  inspect)
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true,"ExitCode":0,"StartedAt":"%s"},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"%s"}},"Mounts":'"$(cat "$EFFIGY_TEST_MOUNTS")"',"NetworkSettings":{"Ports":{}}}]
' "$EFFIGY_TEST_ID" "$EFFIGY_TEST_CONTAINER" "$(cat "$EFFIGY_TEST_STARTED_AT")" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
    exit 0
    ;;
esac
# The exec part of a compose invocation runs on this host, against the
# fixture's own files: a delivered runtime file really lands on disk.
while [ $# -gt 0 ]; do
  case "$1" in
    -f|-p|-e|--env) shift 2 ;;
    -T|-i|-t) shift ;;
    exec) shift; break ;;
    *) shift ;;
  esac
done
while [ $# -gt 0 ]; do
  case "$1" in
    -T|-i|-t|--) shift ;;
    -w|--workdir|-u|--user) shift 2 ;;
    *) break ;;
  esac
done
shift || exit 1
if [ "$1" = sh ] && [ "$2" = -lc ]; then
  cmd="$3"
  case "$cmd" in
    *chown*)
      # The image's workspace user is `dev`; on this host the fixture runs as
      # the login user, so the one name is mapped the way the container would
      # resolve it.
      cmd=$(printf '%s' "$cmd" | sed "s/dev/$EFFIGY_FAKE_WORKSPACE_USER/g")
      ;;
  esac
  sh -lc "$cmd"
  exit $?
fi
exit 1
"#,
            )
            .expect("write fake docker");
            let colima = bin.join("colima");
            fs::write(
                &colima,
                "#!/bin/sh
case \"$1\" in status) echo 'status: Running';; esac
exit 0
",
            )
            .expect("write fake colima");
            for program in [&docker, &colima] {
                let mut perms = fs::metadata(program).expect("stat fake").permissions();
                use std::os::unix::fs::PermissionsExt;
                perms.set_mode(0o755);
                fs::set_permissions(program, perms).expect("chmod fake");
            }

            let home = root.join("home");
            fs::create_dir_all(&home).expect("mkdir fake home");
            // The fake runtime reports this as the container's start time; a
            // restart rewrites the file.
            let started_at = root.join("started-at");
            fs::write(&started_at, "2026-11-18T10:00:00Z").expect("write start identity");
            // The container's mounts, as its inspect reports them: the
            // runtime directory is a persistent bind by default.
            let mounts = root.join("mounts.json");
            fs::write(
                &mounts,
                format!(
                    r#"[{{"Type":"bind","Destination":"{}"}}]"#,
                    runtime_dir.display()
                ),
            )
            .expect("write mount evidence");
            let env = crate::contract_test_support::EnvGuard::set_many(&[
                ("PATH", Some(format!("{}:/bin:/usr/bin", bin.display()))),
                ("HOME", Some(home.display().to_string())),
                ("DOCKER_HOST", Some("tcp://127.0.0.1:1".to_owned())),
                ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
                (
                    "EFFIGY_TEST_CONTAINERS_HOME",
                    Some(root.join(".effigy").display().to_string()),
                ),
                ("EFFIGY_TEST_DOCKER_LOG", Some(log.display().to_string())),
                ("EFFIGY_TEST_ID", Some("c".repeat(64))),
                (
                    "EFFIGY_TEST_CONTAINER",
                    Some("started-web-app-1".to_owned()),
                ),
                ("EFFIGY_TEST_PROJECT", Some("started-web".to_owned())),
                ("EFFIGY_TEST_WORKDIR", Some(repo_root.display().to_string())),
                ("EFFIGY_FAKE_WORKSPACE_USER", Some(current_user())),
                (
                    "EFFIGY_TEST_STARTED_AT",
                    Some(started_at.display().to_string()),
                ),
                ("EFFIGY_TEST_MOUNTS", Some(mounts.display().to_string())),
            ]);
            Self {
                repo_root,
                runtime_dir: runtime_dir.display().to_string(),
                started_at,
                mounts,
                _env: env,
            }
        }

        fn policy(&self) -> EffectiveContainerPolicy {
            effigy_containers::load_container_policy(&self.repo_root, None).expect("fixture policy")
        }

        fn runtime_dir(&self) -> String {
            self.runtime_dir.clone()
        }
    }

    /// The login user owns this test process, so the fake container's `chown`
    /// to the workspace user is the same owner and needs no privilege.
    fn current_user() -> String {
        std::process::Command::new("id")
            .arg("-un")
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|name| name.trim().to_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "dev".to_owned())
    }

    fn temp_repo(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "effigy-container-secret-env-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
        root
    }

    struct ScopedEnvVar {
        key: String,
        previous: Option<String>,
        _lock: crate::contract_test_support::ReentrantTestLockGuard,
    }

    impl ScopedEnvVar {
        fn set(key: &str, value: &str) -> Self {
            let lock = crate::contract_test_support::lock_test();
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key: key.to_owned(),
                previous,
                _lock: lock,
            }
        }

        /// Holds the test lock and guarantees the variable is not set, so a
        /// passphrase another test left behind can never answer this one.
        fn remove(key: &str) -> Self {
            let lock = crate::contract_test_support::lock_test();
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::remove_var(key);
            }
            Self {
                key: key.to_owned(),
                previous,
                _lock: lock,
            }
        }
    }

    fn write_optional_only_container_manifest(root: &Path) {
        fs::write(
            root.join("effigy.toml"),
            r#"
[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets/local.vault"

[secrets.keys.openai_api_key]
required = false
targets = ["containers"]

[secrets.keys.recaptcha_secret]
required = false
targets = ["containers"]

[secrets.keys.mailchimp_api_key]
required = false
targets = ["containers"]

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
"#,
        )
        .expect("write manifest");
    }

    /// A vault exactly as a first `effigy dev` upgrade leaves it: the payload
    /// carries the local-dev key, and the key file is owner-only.
    fn write_dev_key_vault(root: &Path, records: &[(&str, &str)]) {
        let mut payload = VaultPlaintextPayload::empty();
        for (name, value) in records {
            payload.records.insert(
                (*name).to_owned(),
                VaultSecretRecord::new(SecretValue::new(*value)),
            );
        }
        let key = effigy_secrets::LocalDevUnlockKey::generate().expect("generate local-dev key");
        let envelope = payload
            .encrypt_with_passphrase_and_local_dev_key("vault-passphrase", &key)
            .expect("encrypt test vault");
        let vault_path = root.join(".effigy/secrets/local.vault");
        effigy_test_support::write_private_fixture(
            &vault_path,
            envelope
                .to_json_pretty()
                .expect("serialize test vault")
                .as_bytes(),
        )
        .expect("write test vault");
        let key_path = effigy_secrets::local_dev_unlock_key_path(&vault_path);
        fs::write(&key_path, key.expose()).expect("write local-dev key");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&key_path, fs::Permissions::from_mode(0o600))
                .expect("secure local-dev key");
        }
    }

    impl Drop for ScopedEnvVar {
        fn drop(&mut self) {
            unsafe {
                if let Some(previous) = &self.previous {
                    std::env::set_var(&self.key, previous);
                } else {
                    std::env::remove_var(&self.key);
                }
            }
        }
    }

    fn write_test_vault(root: &Path, passphrase: &str, records: &[(&str, &str)]) {
        let mut payload = VaultPlaintextPayload::empty();
        for (name, value) in records {
            payload.records.insert(
                (*name).to_owned(),
                VaultSecretRecord::new(SecretValue::new(*value)),
            );
        }
        let envelope = payload
            .encrypt_with_passphrase(passphrase)
            .expect("encrypt test vault");
        let vault_path = root.join(".effigy/secrets/local.vault");
        effigy_test_support::write_private_fixture(
            &vault_path,
            envelope
                .to_json_pretty()
                .expect("serialize test vault")
                .as_bytes(),
        )
        .expect("write test vault");
    }

    fn test_runtime_files_policy() -> EffectiveContainerPolicy {
        EffectiveContainerPolicy {
            render: None,
            repo_root: std::path::PathBuf::from("/tmp"),
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Generated,
            compose_files: Vec::new(),
            managed_volumes: Vec::new(),
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services: Vec::new(),
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
            runtime_host_key: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: Vec::new(),
            service_aliases: Vec::new(),
            declared_ports: Vec::new(),
            ports_declared_explicitly: false,
            declared_mounts: Vec::new(),
            declared_media_mounts: Vec::new(),
            workspace_source: effigy_containers::artifact_source::WorkspaceSource::Checkout,
            pull_production_hook: None,
            health_check: None,
            health_timeout_secs: 60,
            secret_delivery: ManifestContainerSecretDelivery::RuntimeFiles,
            secret_runtime_dir: Some("/run/effigy/secrets".to_owned()),
            source_secret_runtime_for_deferrals: true,
            workspace_user: Some("dev".to_owned()),
            workspace_home: Some("/home/dev".to_owned()),
            on_task_exit: effigy_manifest::ManifestContainerOnTaskExit::Stop,
            shutdown: effigy_manifest::ManifestContainerShutdownMode::Graceful,
            detach_timeout_secs: 10,
            host_processes: Vec::new(),
        }
    }
}
