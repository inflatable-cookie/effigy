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

#[derive(Debug)]
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

/// Resolve the secrets a task's environment ensure delivers, and report keys
/// it could not deliver instead of failing the task.
pub(in crate::runner) fn compose_secret_env_for_apply(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<Vec<(String, OsString)>, RunnerError> {
    let runtime = resolve_container_secret_runtime(repo_root, policy, secrets_required())?;
    for warning in &runtime.warnings {
        eprintln!("[warn] {warning}");
    }
    Ok(compose_env(&runtime))
}

pub(super) fn secrets_required() -> bool {
    std::env::var(SECRETS_REQUIRED_ENV)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(super) fn resolve_container_secret_runtime(
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
    } else {
        match crate::runner::secret_session::read_vault_passphrase(
            &vault_path,
            optional_only,
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
                container_secret_env_name(name),
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
fn writes_runtime_secret_files(runtime: &ResolvedContainerSecretRuntime) -> bool {
    runtime.delivery == ManifestContainerSecretDelivery::RuntimeFiles && !runtime.env.is_empty()
}

pub(super) fn materialize_container_secret_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    runtime: &ResolvedContainerSecretRuntime,
    binding: &effigy_containers::exec::VerifiedComposeContainer,
) -> Result<(), RunnerError> {
    if !writes_runtime_secret_files(runtime) {
        return Ok(());
    }
    let Some(runtime_dir) = policy.secret_runtime_dir.as_deref() else {
        return Err(RunnerError::task_invocation(format!(
            "container `{}` uses `secrets.delivery = \"runtime-files\"` but no runtime_dir was configured",
            policy.name
        )));
    };
    let runtime_env_path = format!("{runtime_dir}/runtime.env");
    let runtime_json_path = format!("{runtime_dir}/runtime.json");

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

    Ok(())
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

fn container_secret_env_name(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
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
        container_secret_runtime_env_path, render_runtime_env_file, render_runtime_json_file,
        resolve_container_secret_runtime, writes_runtime_secret_files,
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
        write_test_vault(&root, "vault-passphrase", &[("api_token", "tok_secret")]);
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let runtime = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), true)
            .expect("resolve secrets");

        assert_eq!(runtime.env.len(), 1);
        assert_eq!(runtime.env[0].0, "API_TOKEN");
        assert_eq!(runtime.env[0].1.expose(), "tok_secret");
    }

    #[test]
    fn container_secret_env_forced_unlock_honors_optional_container_values() {
        let root = temp_repo("container-secret-force-required-missing");
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
        write_test_vault(&root, "vault-passphrase", &[]);
        let _env = ScopedEnvVar::set("EFFIGY_TEST_SECRETS_PASSPHRASE", "vault-passphrase");

        let runtime = resolve_container_secret_runtime(&root, &test_runtime_files_policy(), true)
            .expect("optional missing value should not fail");

        assert!(runtime.env.is_empty());
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
        assert_eq!(runtime.env.len(), 3);
        assert!(runtime
            .env
            .iter()
            .any(|(key, value)| key == "OPENAI_API_KEY" && value.expose() == "sk-dev-value"));
        assert!(runtime
            .env
            .iter()
            .any(|(key, value)| key == "RECAPTCHA_SECRET" && value.expose() == "rc-dev-value"));
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
        assert!(runtime
            .warnings
            .iter()
            .any(|warning| warning.contains("openai_api_key")
                && warning.contains("not stored in the vault")
                && warning.contains("effigy secrets set openai_api_key")));
    }

    #[test]
    fn container_secret_runtime_env_path_is_opt_in() {
        let policy = test_runtime_files_policy();

        assert_eq!(
            container_secret_runtime_env_path(&policy).as_deref(),
            Some("/run/effigy/secrets/runtime.env")
        );
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
        fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
        fs::write(
            &vault_path,
            envelope.to_json_pretty().expect("serialize test vault"),
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
        fs::create_dir_all(vault_path.parent().expect("vault parent")).expect("mkdir vault parent");
        fs::write(
            vault_path,
            envelope.to_json_pretty().expect("serialize test vault"),
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
            compose_file_display: String::new(),
            managed_volumes: Vec::new(),
            borrowed_volumes: Vec::new(),
            data_peers: Vec::new(),
            data_from: None,
            shared_services: Vec::new(),
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
            dns_base_domain: None,
            dns_tls: false,
            dns_port: None,
            dns_routes: Vec::new(),
            service_aliases: Vec::new(),
            declared_ports: Vec::new(),
            ports_declared_explicitly: false,
            declared_mounts: Vec::new(),
            declared_media_mounts: Vec::new(),
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
