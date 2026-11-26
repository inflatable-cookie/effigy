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
const UNLOCK_FIX: &str =
    "run `effigy secrets unlock` at a terminal, or run `effigy dev` interactively";

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
                "set `backend = \"effigy-vault\"` under `[secrets]` and store values with `effigy secrets set <KEY>`",
            ));
        }
        return Err(RunnerError::task_invocation(
            "required container secrets need `[secrets].backend = \"effigy-vault\"`\nnext: set `backend = \"effigy-vault\"` under `[secrets]` in `effigy.toml`",
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
                "run `effigy secrets init` and store values with `effigy secrets set <KEY>`",
            ));
        }
        return Err(RunnerError::task_invocation(format!(
            "required container secrets ({}) are declared, but the vault is missing\nvault: {}\nnext: run `effigy secrets init` and add each key with `effigy secrets set <KEY>`",
            required_names.join(", "),
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
                        "local-dev container secrets need one passphrase unlock to create the unattended dev key, and secret input requires an interactive TTY\nnext: run `effigy secrets unlock` or `effigy dev` at an interactive terminal",
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
            "container secrets require an unlocked vault passphrase and secret input requires an interactive TTY\nnext: run `effigy secrets unlock` at a terminal, or run `effigy dev` interactively",
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
                "container secret `{name}` was not delivered: declared in `effigy.toml` but not stored in the vault; run `effigy secrets set {name}`"
            )),
        }
    }
    if !missing_required.is_empty() {
        let next = if let [single] = missing_required.as_slice() {
            format!("run `effigy secrets set {single}`")
        } else {
            format!(
                "run `effigy secrets set <KEY>` for each missing key ({})",
                missing_required.join(", ")
            )
        };
        return Err(RunnerError::task_invocation(format!(
            "required container secret(s) missing from the vault: {}\nnext: {next}",
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

/// Write this delivery's runtime files, and say what changed under the running
/// service, in this apply only.
///
/// The file is read back before it is rewritten, so the warning names only the
/// keys whose value actually changed; an apply that writes exactly what the
/// service already holds says nothing. Nothing is persisted between applies:
/// the delivery reports what it changed, and the next apply compares against
/// the file it finds (effigy#1036). Warnings name keys, never values.
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
    // difference is what this delivery says out loud, once.
    let previous = read_primary_service_file(repo_root, policy, &runtime_json_path, binding)?;
    let changed = changed_runtime_secret_keys(previous.as_deref(), runtime);

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

    Ok(runtime_secret_refresh_warnings(
        policy,
        runtime_dir,
        &changed,
    ))
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

/// The declared keys whose value differs from the runtime file this delivery
/// replaces. A key the service already reads is not reported, and a service
/// that holds no file at all is told about every delivered key.
pub(in crate::runner) fn changed_runtime_secret_keys(
    held: Option<&str>,
    runtime: &ResolvedContainerSecretRuntime,
) -> Vec<String> {
    if runtime.env.is_empty() {
        return Vec::new();
    }
    let held_document: serde_json::Map<String, serde_json::Value> = held
        .and_then(|document| serde_json::from_str(document).ok())
        .unwrap_or_default();
    runtime
        .env
        .iter()
        .filter(|(key, value)| {
            held_document
                .get(key)
                .and_then(serde_json::Value::as_str)
                .is_none_or(|held| held != value.expose())
        })
        .map(|(key, _)| key.clone())
        .collect()
}

/// The one line a changed delivery prints, in that apply only.
///
/// The delivery cannot know whether a consumer reads these files per request or
/// once at startup, so it states both facts. It prescribes nothing: runtime
/// files are written after services start on every path, so a startup-only
/// reader cannot be served by this delivery, and restarting the container may
/// clear a tmpfs `runtime_dir`. That delivery-timing limit is its own lead
/// (planner ruling 9b1019a7). It names keys, never a value (effigy#1036).
pub(in crate::runner) fn runtime_secret_refresh_warnings(
    policy: &EffectiveContainerPolicy,
    runtime_dir: &str,
    changed: &[String],
) -> Vec<String> {
    if changed.is_empty() {
        return Vec::new();
    }
    vec![format!(
        "container secret(s) {} were written to {runtime_dir}/runtime.json after `{service}` started; services that read this file per request see them now; a service that reads it only at startup needs its process restarted to see them",
        changed.join(", "),
        service = policy.primary_service,
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
        changed_runtime_secret_keys, container_secret_runtime_env_path,
        materialize_container_secret_runtime, render_runtime_env_file, render_runtime_json_file,
        resolve_container_secret_runtime, runtime_secret_refresh_warnings,
        writes_runtime_secret_files, ResolvedContainerSecretRuntime,
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

    /// A delivery reports the keys it changed under the running service, in
    /// that apply only, and no line carries a value (effigy#1036).
    #[test]
    fn a_runtime_file_delivery_reports_only_the_keys_it_changes() {
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

        // The service holds the same unchanged value and a stale openai key.
        let held = r#"{"OPENAI_API_KEY":"sk-old-value","UNCHANGED_KEY":"kept-value"}"#;
        let changed = changed_runtime_secret_keys(Some(held), &runtime);
        assert_eq!(changed, vec!["OPENAI_API_KEY".to_owned()]);

        let warnings = runtime_secret_refresh_warnings(&policy, "/run/effigy/secrets", &changed);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        let line = &warnings[0];
        assert!(line.contains("OPENAI_API_KEY"), "{line}");
        assert!(!line.contains("sk-new-value"), "{line}");
        assert!(!line.contains("sk-old-value"), "{line}");
        assert!(
            !line.contains("UNCHANGED_KEY"),
            "a value the service already reads is not reported: {line}"
        );
        assert!(
            line.contains("were written to /run/effigy/secrets/runtime.json after `app` started"),
            "{line}"
        );
        assert!(
            line.contains("services that read this file per request see them now"),
            "{line}"
        );
        assert!(
            line.contains(
                "a service that reads it only at startup needs its process restarted to see them"
            ),
            "{line}"
        );
        assert!(
            !line.contains("stays until"),
            "the warning is one apply's report, not a persisted record: {line}"
        );
        // Facts only: the delivery prescribes no remedy, because runtime files
        // are written after services start on every path and a container
        // restart may clear a tmpfs runtime directory (planner ruling
        // 9b1019a7).
        for prescribed in [
            "stop the stack",
            "effigy dev` again",
            "effigy container",
            "restart the container",
        ] {
            assert!(
                !line.contains(prescribed),
                "the warning prescribes nothing ({prescribed}): {line}"
            );
        }

        // An apply that writes what the service already holds says nothing.
        let matching = r#"{"OPENAI_API_KEY":"sk-new-value","UNCHANGED_KEY":"kept-value"}"#;
        assert!(
            changed_runtime_secret_keys(Some(matching), &runtime).is_empty(),
            "an identical re-apply is silent"
        );

        // A service holding no file at all is told about every delivered key.
        assert_eq!(
            changed_runtime_secret_keys(None, &runtime),
            vec!["OPENAI_API_KEY".to_owned(), "UNCHANGED_KEY".to_owned()]
        );

        // An empty delivery has nothing to say.
        let empty = ResolvedContainerSecretRuntime {
            delivery: ManifestContainerSecretDelivery::RuntimeFiles,
            env: Vec::new(),
            warnings: Vec::new(),
        };
        assert!(changed_runtime_secret_keys(None, &empty).is_empty());
        assert!(runtime_secret_refresh_warnings(&policy, "/run/effigy/secrets", &[]).is_empty());
    }

    /// A `dev` session that borrows a stack a direct `container up` started
    /// writes the keys it resolved into the running service's runtime files and
    /// reports the change once; the next apply of the same values is silent and
    /// nothing is carried between applies (effigy#1036, planner ruling
    /// 9b1019a7).
    #[test]
    fn a_reused_running_service_receives_the_resolved_keys_and_warns_once() {
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
            warnings[0]
                .contains("a service that reads it only at startup needs its process restarted"),
            "a startup-only reader is told the fact, not a remedy: {}",
            warnings[0]
        );
        assert!(
            !warnings[0].contains("stop the stack") && !warnings[0].contains("effigy container"),
            "the warning prescribes nothing: {}",
            warnings[0]
        );
        assert!(
            !warnings[0].contains("mc-session-value"),
            "a warning never carries a value: {}",
            warnings[0]
        );

        // A later apply of the same values says nothing: the file it replaces
        // already carries them.
        let again =
            materialize_container_secret_runtime(&fixture.repo_root, &policy, &runtime, &binding)
                .expect("the same delivery again");
        assert!(again.is_empty(), "{again:?}");

        // Nothing about the warning is carried between applies: no state file,
        // no record directory.
        assert!(
            !fixture
                .repo_root
                .join(".effigy/runtime/container-secrets")
                .exists(),
            "the delivery keeps no state across applies"
        );
    }

    /// A running container this checkout owns, with a docker that proves the
    /// inventory and an exec that runs against the fixture on disk.
    struct RunningServiceFixture {
        repo_root: PathBuf,
        runtime_dir: String,
        _env: crate::contract_test_support::EnvGuard,
    }

    impl RunningServiceFixture {
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
    printf '[{"Id":"%s","Name":"/%s","State":{"Status":"running","Running":true,"ExitCode":0},"Config":{"Labels":{"com.docker.compose.project":"%s","com.docker.compose.service":"app","com.docker.compose.oneoff":"False","com.docker.compose.project.working_dir":"%s"}},"Mounts":[],"NetworkSettings":{"Ports":{}}}]
' "$EFFIGY_TEST_ID" "$EFFIGY_TEST_CONTAINER" "$EFFIGY_TEST_PROJECT" "$EFFIGY_TEST_WORKDIR"
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
            ]);
            Self {
                repo_root,
                runtime_dir: runtime_dir.display().to_string(),
                _env: env,
            }
        }

        fn policy(&self) -> EffectiveContainerPolicy {
            crate::runner::test_support::load_fixture_policy(&self.repo_root, None)
                .expect("fixture policy")
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
            fs_events: Default::default(),
        }
    }
}
