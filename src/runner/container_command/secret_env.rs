use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use effigy_containers::ContainerAction;
use effigy_containers::EffectiveContainerPolicy;
use effigy_core::shell::shell_quote;
use effigy_env::secret::SecretString;
use effigy_manifest::{
    ManifestContainerSecretDelivery, ManifestSecretTarget, ManifestSecretsBackend,
};

use crate::runner::error::RunnerError;
use crate::runner::exec_command::run_compose_exec_plan_with_options;
use crate::runner::manifest::{load_task_manifest, TASK_MANIFEST_FILE};

#[derive(Debug)]
pub(in crate::runner) struct ResolvedContainerSecretRuntime {
    pub delivery: ManifestContainerSecretDelivery,
    pub env: Vec<(String, SecretString)>,
}

pub(super) fn resolve_container_secret_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    force_unlock: bool,
) -> Result<ResolvedContainerSecretRuntime, RunnerError> {
    let manifest = load_task_manifest(&repo_root.join(TASK_MANIFEST_FILE))?;
    let delivery = policy.secret_delivery;
    let Some(secrets) = manifest.secrets.as_ref() else {
        return Ok(ResolvedContainerSecretRuntime {
            delivery,
            env: Vec::new(),
        });
    };
    let container_keys = secrets
        .keys
        .iter()
        .filter(|(_, key)| key.targets.contains(&ManifestSecretTarget::Containers))
        .collect::<Vec<_>>();
    if container_keys.is_empty() {
        return Ok(ResolvedContainerSecretRuntime {
            delivery,
            env: Vec::new(),
        });
    }

    let required_names = container_keys
        .iter()
        .filter(|(_, key)| key.required)
        .map(|(name, _)| (*name).clone())
        .collect::<Vec<_>>();
    if !matches!(secrets.backend, Some(ManifestSecretsBackend::EffigyVault)) {
        if required_names.is_empty() {
            return Ok(ResolvedContainerSecretRuntime {
                delivery,
                env: Vec::new(),
            });
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
            return Ok(ResolvedContainerSecretRuntime {
                delivery,
                env: Vec::new(),
            });
        }
        return Err(RunnerError::task_invocation(format!(
            "required container secrets are declared but the vault is missing at {}",
            vault_path.display()
        )));
    }

    let payload = if crate::runner::secret_session::local_dev_secret_access_active() {
        match crate::runner::secret_vault::read_effigy_vault_payload_for_local_dev(&vault_path) {
            Ok(payload) => payload,
            Err(_) => {
                let Some(passphrase) =
                    crate::runner::secret_session::read_local_dev_upgrade_passphrase(
                    required_names.is_empty() && !force_unlock,
                    "Vault passphrase (one-time local-dev setup): ",
                    "local-dev container secrets need one passphrase unlock to create the unattended dev key, and secret input requires an interactive TTY",
                )? else {
                    return Ok(ResolvedContainerSecretRuntime {
                        delivery,
                        env: Vec::new(),
                    });
                };
                let payload = crate::runner::secret_vault::read_effigy_vault_payload(
                    &vault_path,
                    passphrase.expose(),
                )?;
                crate::runner::secret_vault::write_effigy_vault_payload(
                    &vault_path,
                    &payload,
                    passphrase.expose(),
                )?;
                payload
            }
        }
    } else {
        let Some(passphrase) = crate::runner::secret_session::read_secret_passphrase(
            required_names.is_empty() && !force_unlock,
            "Vault passphrase: ",
            "container secrets require an unlocked vault passphrase and secret input requires an interactive TTY",
        )? else {
            return Ok(ResolvedContainerSecretRuntime {
                delivery,
                env: Vec::new(),
            });
        };
        crate::runner::secret_vault::read_effigy_vault_payload(&vault_path, passphrase.expose())?
    };
    let mut injected = Vec::new();
    let mut missing_required = Vec::new();
    for (name, key) in container_keys {
        match payload.records.get(name.as_str()) {
            Some(record) => injected.push((
                container_secret_env_name(name),
                SecretString::new(record.value.expose().to_owned()),
            )),
            None if key.required => missing_required.push(name.to_owned()),
            None => {}
        }
    }
    if !missing_required.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "required container secret(s) missing from the vault: {}",
            missing_required.join(", ")
        )));
    }
    Ok(ResolvedContainerSecretRuntime {
        delivery,
        env: injected,
    })
}

pub(super) fn materialize_container_secret_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    runtime: &ResolvedContainerSecretRuntime,
) -> Result<(), RunnerError> {
    if runtime.delivery != ManifestContainerSecretDelivery::RuntimeFiles || runtime.env.is_empty() {
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
    )?;
    write_file_into_primary_service(
        repo_root,
        policy,
        &runtime_json.path,
        runtime_json_path.as_str(),
        "write container secret runtime json",
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
    let output = run_compose_exec_plan_with_options(policy, &plan, true, None)?;
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
    let output = run_compose_exec_plan_with_options(policy, &plan, true, Some(host_source))?;
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

struct TempSecretFile {
    path: PathBuf,
}

impl TempSecretFile {
    fn write(label: &str, bytes: &[u8]) -> Result<Self, RunnerError> {
        let path = std::env::temp_dir().join(format!(
            "effigy-container-secret-runtime-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?
                .as_nanos()
        ));
        fs::write(&path, bytes).map_err(|error| {
            RunnerError::task_invocation(format!(
                "failed to write temporary container secret file {}: {error}",
                path.display()
            ))
        })?;
        Ok(Self { path })
    }
}

impl Drop for TempSecretFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        container_secret_runtime_env_path, render_runtime_env_file, render_runtime_json_file,
        resolve_container_secret_runtime,
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
identity = "passphrase"
unlock = "passphrase"

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
identity = "passphrase"
unlock = "passphrase"

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
identity = "passphrase"
unlock = "passphrase"

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
identity = "passphrase"
unlock = "passphrase"

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
    }

    impl ScopedEnvVar {
        fn set(key: &str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            unsafe {
                std::env::set_var(key, value);
            }
            Self {
                key: key.to_owned(),
                previous,
            }
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
            name: "web".to_owned(),
            driver: effigy_manifest::ManifestContainerDriver::Colima,
            startup: effigy_manifest::ManifestContainerStartup::Detached,
            profile: "effigy".to_owned(),
            compose_source: EffectiveComposeSource::Generated,
            compose_files: Vec::new(),
            compose_file_display: String::new(),
            managed_volumes: Vec::new(),
            shared_services: Vec::new(),
            project_name: "demo".to_owned(),
            primary_service: "app".to_owned(),
            dns_domain: None,
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
