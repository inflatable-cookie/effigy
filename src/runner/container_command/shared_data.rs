//! Where a declared database data target runs (effigy#477).
//!
//! A `shared = true` backing service belongs to no checkout: its Compose
//! project, render and named data volume are machine-wide, and the environment's
//! own generated Compose removes the service. A seed or dump that resolved such
//! a service in the selected environment therefore has to exec in the shared
//! project's own render instead of the checkout-local one. An unshared target
//! keeps exactly the checkout-local route it had.
//!
//! A data command never publishes, starts or replaces a shared service. Before
//! the first statement it proves the declared shared render's files and the
//! declared shared container: running in the expected project, from the
//! declared render digest and for the declared service. It refuses with an
//! actionable error otherwise.
//!
//! A database client's credential travels by variable name only: the exec
//! adapter places the value in a private file the Compose client sources, on
//! the host or inside the Colima VM, so the value never reaches a command line,
//! a Compose invocation or a report.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Output;

use effigy_containers::exec::SharedProjectContainer;
use effigy_containers::{EffectiveContainerPolicy, SharedRender, SharedServiceBinding};
use effigy_manifest::{ManifestContainerServiceConfig, TaskManifest};

use super::RunnerError;

/// The shared binding a declared data target's service resolves to, if any.
///
/// `None` means the selected environment keeps the checkout-local route: the
/// service is not `shared = true` there, so nothing about a same-named shared
/// service in another environment may redirect this target.
pub(in crate::runner) fn resolve_shared_data_binding(
    policy: &EffectiveContainerPolicy,
    manifest: &TaskManifest,
    service: &str,
) -> Result<Option<SharedServiceBinding>, RunnerError> {
    let Some(selected) = declaration(policy, manifest, service) else {
        return Ok(None);
    };
    if selected.shared != Some(true) {
        return Ok(None);
    }
    let Some(binding) = policy
        .shared_services
        .iter()
        .find(|shared| shared.service_name == service)
    else {
        // A shared declaration always renders a binding. Falling back to the
        // checkout-local route here would silently seed a different database.
        return Err(RunnerError::task_invocation(format!(
            "container `{env}` declares service `{service}` with `shared = true`, but its effective policy has no shared binding for it. Nothing was executed. Re-run `effigy container up {env}` to resolve the shared service, or correct the manifest.",
            env = policy.name,
        )));
    };
    // The declaration is what names the shared project, so every environment
    // sharing this service must declare exactly the same identity. Two
    // different declarations are two Compose projects, and a data target may
    // not pick one of them by the environment it happened to run in.
    let conflicting = shared_declaration_environments(manifest, service)
        .into_iter()
        .filter(|(env, declaration)| {
            env != &policy.name && !same_shared_identity(declaration, selected)
        })
        .map(|(env, _)| env)
        .collect::<Vec<_>>();
    if !conflicting.is_empty() {
        let environments = conflicting
            .iter()
            .map(|env| format!("`{env}`"))
            .chain(std::iter::once(format!("`{}`", policy.name)))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(RunnerError::task_invocation(format!(
            "database target service `{service}` is declared `shared = true` with different machine-wide identities by containers {environments}. Different shared declarations are different Compose projects, so this target cannot resolve to one shared project; nothing was executed. Align the shared `{service}` declarations, then re-run.",
        )));
    }
    Ok(Some(binding.clone()))
}

/// The selected environment's declaration of one service, if it declares it.
fn declaration<'a>(
    policy: &EffectiveContainerPolicy,
    manifest: &'a TaskManifest,
    service: &str,
) -> Option<&'a ManifestContainerServiceConfig> {
    manifest
        .containers
        .as_ref()?
        .environments
        .get(&policy.name)?
        .services
        .get(service)
}

/// The declaration of one service in every environment that shares it.
fn shared_declaration_environments<'a>(
    manifest: &'a TaskManifest,
    service: &str,
) -> Vec<(String, &'a ManifestContainerServiceConfig)> {
    let Some(containers) = manifest.containers.as_ref() else {
        return Vec::new();
    };
    containers
        .environments
        .iter()
        .filter_map(|(env, config)| {
            let declaration = config.services.get(service)?;
            (declaration.shared == Some(true)).then(|| (env.clone(), declaration))
        })
        .collect()
}

/// Whether two shared declarations render the same machine-wide identity: the
/// catalog and every parameter, never the service's environment-local name.
fn same_shared_identity(
    left: &ManifestContainerServiceConfig,
    right: &ManifestContainerServiceConfig,
) -> bool {
    left.catalog == right.catalog
        && left.variant == right.variant
        && left.config == right.config
        && left.params == right.params
}

/// One shared data command, resolved but not yet run.
///
/// `command` is the in-container argv. A credential assignment in the plan's
/// `env` prefix becomes a named `compose exec -e NAME`, with the value carried
/// separately by the private credential transport, so the value never appears
/// in this plan's Composes command line or in a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct SharedDataExecPlan {
    pub service: String,
    pub command: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub stdin_file: Option<PathBuf>,
}

/// Resolve the command one data step runs in the shared project.
pub(in crate::runner) fn plan_shared_data_exec(
    binding: &SharedServiceBinding,
    service: &str,
    command: &[String],
    stdin_file: Option<&Path>,
) -> Result<SharedDataExecPlan, RunnerError> {
    if service != binding.service_name {
        return Err(RunnerError::task_invocation(format!(
            "shared data command resolved service `{service}` but its shared binding is `{}`; refusing to exec in a project that does not own the target",
            binding.service_name
        )));
    }
    let (env, command) = split_command_env(command)?;
    Ok(SharedDataExecPlan {
        service: binding.service_name.clone(),
        command,
        env,
        stdin_file: stdin_file.map(Path::to_path_buf),
    })
}

/// Split a leading `env NAME=value ...` prefix from the in-container command.
fn split_command_env(
    command: &[String],
) -> Result<(BTreeMap<String, String>, Vec<String>), RunnerError> {
    if command.first().map(String::as_str) != Some("env") {
        return Ok((BTreeMap::new(), command.to_vec()));
    }
    let mut env = BTreeMap::new();
    let mut index = 1;
    while let Some(entry) = command.get(index) {
        let Some((name, value)) = entry.split_once('=') else {
            break;
        };
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_');
        if !valid {
            break;
        }
        env.insert(name.to_owned(), value.to_owned());
        index += 1;
    }
    if env.is_empty() {
        return Ok((BTreeMap::new(), command.to_vec()));
    }
    let argv = command[index..].to_vec();
    if argv.is_empty() {
        return Err(RunnerError::task_invocation(
            "shared data command has an `env` prefix but no command to run",
        ));
    }
    Ok((env, argv))
}

/// Run one reset, import or logical dump in the declared shared project.
pub(in crate::runner) fn run_shared_data_command(
    policy: &EffectiveContainerPolicy,
    binding: &SharedServiceBinding,
    command: &[String],
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    let plan = plan_shared_data_exec(binding, &binding.service_name, command, stdin_file)?;
    verify_shared_data_target(policy, binding)?;
    let mut tail = vec!["exec".to_owned(), "-T".to_owned()];
    for name in plan.env.keys() {
        tail.push("-e".to_owned());
        tail.push(name.clone());
    }
    tail.push(plan.service.clone());
    tail.extend(plan.command.clone());
    let output = super::support::run_shared_service_exec_capture(
        policy,
        binding,
        tail.iter().map(String::as_str),
        &plan.env,
        plan.stdin_file.as_deref(),
    )?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "shared {service} data command failed ({code})\nproject: {project}\nstdout:\n{stdout}\nstderr:\n{stderr}",
            service = binding.service_name,
            project = binding.project_name,
            code = effigy_ui::human::exit_code(output.status.code()),
            stdout = String::from_utf8_lossy(&output.stdout),
            stderr = String::from_utf8_lossy(&output.stderr),
        )));
    }
    Ok(output)
}

/// Prove the declared shared service can receive the command before running
/// anything against it: the expected render's every file is present, readable
/// and unchanged, and the runtime reports exactly one running container of the
/// declared service in the declared project, created from the declared digest.
pub(in crate::runner) fn verify_shared_data_target(
    policy: &EffectiveContainerPolicy,
    binding: &SharedServiceBinding,
) -> Result<(), RunnerError> {
    let render = expected_shared_render(policy, binding)?;
    verify_shared_render_files(binding, &policy.name, render)?;
    let containers = effigy_containers::exec::shared_project_containers(
        &policy.repo_root,
        policy,
        &binding.project_name,
    )
    .map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot read the runtime's shared {service} containers (project `{project}`): {error}. Nothing was executed against it. Make the runtime readable, then re-run.",
            service = binding.service_name,
            project = binding.project_name,
        ))
    })?;
    verify_shared_data_containers(binding, &policy.name, &containers)
}

/// The render this policy expects beside the binding's own Compose file.
fn expected_shared_render<'a>(
    policy: &'a EffectiveContainerPolicy,
    binding: &SharedServiceBinding,
) -> Result<&'a SharedRender, RunnerError> {
    let expected = binding.compose_file.parent();
    let render = policy.render.as_ref().and_then(|rendered| {
        rendered
            .shared
            .iter()
            .find(|render| Some(render.directory.as_path()) == expected)
    });
    render.ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "shared {service} (project `{project}`) has no rendered service under `{render}`, so a data command cannot prove its files. Nothing was executed. Run `effigy container up {env}` to render and start the shared service, then re-run.",
            service = binding.service_name,
            project = binding.project_name,
            render = expected
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| binding.compose_file.display().to_string()),
            env = policy.name,
        ))
    })
}

/// Every file the expected render binds must be the file on disk. A missing,
/// unreadable or altered file means the render the container claims is not the
/// render this declaration resolves, so nothing runs against it.
fn verify_shared_render_files(
    binding: &SharedServiceBinding,
    env: &str,
    render: &SharedRender,
) -> Result<(), RunnerError> {
    for (relative, expected) in &render.files {
        let path = render.directory.join(relative);
        match std::fs::read(&path) {
            Ok(bytes) if bytes == *expected => {}
            Ok(_) => {
                return Err(RunnerError::task_invocation(format!(
                    "shared {service} (project `{project}`) render file `{path}` was altered on disk, so it no longer matches the render this data target resolved. Nothing was executed, published or replaced. Run `effigy container up {env}` to publish the declared render, then re-run.",
                    service = binding.service_name,
                    project = binding.project_name,
                    path = path.display(),
                    env = env,
                )))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(RunnerError::task_invocation(format!(
                    "shared {service} (project `{project}`) render file `{path}` is missing, so the declared shared service cannot be proved. Nothing was started, replaced or published. Run `effigy container up {env}` to publish the shared render, then re-run.",
                    service = binding.service_name,
                    project = binding.project_name,
                    path = path.display(),
                    env = env,
                )))
            }
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "shared {service} (project `{project}`) render file `{path}` cannot be read: {error}. Nothing was executed or replaced. Make the file readable, or run `effigy container up {env}` to republish the render, then re-run.",
                    service = binding.service_name,
                    project = binding.project_name,
                    path = path.display(),
                    env = env,
                )))
            }
        }
    }
    Ok(())
}

/// The container-identity refusal, separate from the runtime read so the
/// stopped, missing, stale and ambiguous cases are provable against a fake
/// inventory.
fn verify_shared_data_containers(
    binding: &SharedServiceBinding,
    env: &str,
    containers: &[SharedProjectContainer],
) -> Result<(), RunnerError> {
    let service = binding.service_name.as_str();
    let project = binding.project_name.as_str();
    let declared = containers
        .iter()
        .filter(|container| container.service.as_deref() == Some(service))
        .collect::<Vec<_>>();
    if declared.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "shared {service} (project `{project}`) has no container for service `{service}`, so it is not running and a data command cannot exec in it. Nothing was started or replaced. Run `effigy container up {env}`, then re-run.",
        )));
    }
    let running = declared
        .iter()
        .filter(|container| container.running)
        .collect::<Vec<_>>();
    if running.is_empty() {
        let names = declared
            .iter()
            .map(|container| container.container_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(RunnerError::task_invocation(format!(
            "shared {service} (project `{project}`) is stopped (container {names}), so a data command cannot exec in it. A data command never starts or replaces a shared service. Run `effigy container up {env}`, then re-run.",
        )));
    }
    if running.len() > 1 {
        let names = running
            .iter()
            .map(|container| container.container_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(RunnerError::task_invocation(format!(
            "shared {service} (project `{project}`) has more than one running container for the service ({names}), so Effigy cannot prove which one the target means; nothing was executed. Stop the extra container(s) or re-run `effigy container up {env}`.",
        )));
    }
    let container = running[0];
    if container.render.as_deref() != Some(binding.render_digest.as_str()) {
        let render = container
            .render
            .as_deref()
            .map(|render| format!("render `{render}`"))
            .unwrap_or_else(|| "no render label".to_owned());
        return Err(RunnerError::task_invocation(format!(
            "shared {service} (project `{project}`) is running as {name} from {render}, not the declared render `{digest}`, so it does not match the declaration this data command resolved. Nothing was stopped, replaced or published. Run `effigy container up {env}` to reconcile the shared service, then re-run.",
            name = container.container_name,
            digest = binding.render_digest,
            env = env,
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        plan_shared_data_exec, resolve_shared_data_binding, run_shared_data_command,
        verify_shared_data_containers, verify_shared_data_target,
    };
    use crate::contract_test_support::{lock_test, EnvGuard};
    use crate::runner::container_command::data::{
        run_container_data_dump_with_hooks, run_container_data_seed,
    };
    use crate::runner::db_seed::{run_builtin_db_seed_task_with, DB_SEEDS_JSON_ENV};
    use effigy_containers::exec::SharedProjectContainer;
    use effigy_containers::{
        load_container_policy, EffectiveContainerPolicy, SharedServiceBinding,
    };
    use effigy_manifest::TaskManifest;
    use effigy_test_support::Fixture;
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::Output;

    /// One declared shared MariaDB in `stack` and `hybrid`, with hybrid
    /// borrowing stack's checkout-local volumes: `data_from` is a
    /// checkout-local feature and must not redirect the shared target.
    const STACK_HYBRID_MANIFEST: &str = r#"
[data.targets.acowtancy]
service = "mysql"
database = "acowtancy"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "acowtancy"
password = "secret"
shared = true

[containers.hybrid]
primary_service = "app"
data_from = "stack"

[containers.hybrid.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.hybrid.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "acowtancy"
password = "secret"
shared = true
"#;

    /// The default environment declares no `mysql`; only `hybrid` shares it, so
    /// a seed that reloads the default environment cannot resolve the target.
    const HYBRID_ONLY_MANIFEST: &str = r#"
[data.targets.acowtancy]
service = "mysql"
database = "acowtancy"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.postgres]
catalog = "postgres"
database = "acowtancy"

[containers.hybrid]
primary_service = "app"

[containers.hybrid.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.hybrid.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "acowtancy"
password = "secret"
shared = true
"#;

    struct FixtureRepo {
        _fixture: Fixture,
        root: PathBuf,
        home: PathBuf,
        log: PathBuf,
        bin: PathBuf,
    }

    impl FixtureRepo {
        fn new(prefix: &str, manifest: &str) -> Self {
            let fixture = Fixture::workspace(prefix).expect("fixture");
            let root = fixture.path().canonicalize().expect("canonical fixture");
            fs::write(root.join("effigy.toml"), manifest).expect("write manifest");
            let home = root.join("home");
            let bin = home.join("bin");
            let log = home.join("runtime.log");
            fs::create_dir_all(&bin).expect("mkdir fake runtime bin");
            Self {
                _fixture: fixture,
                root,
                home,
                log,
                bin,
            }
        }

        /// The fake runtime serves the shared project inventory and records
        /// every invocation, including a compose exec's stdin, so the test
        /// reads exactly what the adapter ran.
        fn install_fake_runtime(&self) {
            let script = format!(
                "#!/bin/sh\nprintf 'call %s\\n' \"$*\" >> {log}\nprintf 'credential-env=%s\\n' \"${{MYSQL_PWD:+present}}\" >> {log}\ncase \"$*\" in\n  *'--version'*) printf 'fake 1.0\\n' ;;\n  *'ps --all'*) printf '%s\\n' \"$EFFIGY_TEST_PS_ROW\" ;;\n  *'ssh --profile'*) for last; do :; done; if grep -q '^export MYSQL_PWD=' \"$last\" 2>/dev/null; then printf 'credential-file=present\\n' >> {log}; else printf 'credential-file=absent\\n' >> {log}; fi ;;\n  *' exec '*) while IFS= read -r line || [ -n \"$line\" ]; do printf 'stdin %s\\n' \"$line\" >> {log}; done; printf 'dump-body\\n'; i=0; while [ \"$i\" -lt 12000 ]; do printf '%s\\n' '0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789'; i=$((i+1)); done ;;\nesac\nexit 0\n",
                log = self.log.display()
            );
            self.write_runtime(script.as_str());
        }

        /// A runtime whose inventory read fails, without reaching the host.
        fn install_unreadable_runtime(&self) {
            self.write_runtime("#!/bin/sh\nprintf 'runtime unavailable\\n' >&2\nexit 1\n");
        }

        fn write_runtime(&self, script: &str) {
            for name in ["docker", "colima"] {
                let executable = self.bin.join(name);
                fs::write(&executable, script).expect("write fake runtime");
                fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
                    .expect("chmod fake runtime");
            }
        }

        /// The process environment every policy load and runtime call needs.
        fn env(&self) -> EnvGuard {
            self.env_with_backend("docker")
        }

        fn env_with_backend(&self, backend: &str) -> EnvGuard {
            let path = format!(
                "{}:{}",
                self.bin.display(),
                std::env::var("PATH").unwrap_or_default()
            );
            EnvGuard::set_many(&[
                ("HOME", Some(self.home.display().to_string())),
                ("PATH", Some(path)),
                ("EFFIGY_COMPOSE_BACKEND", Some(backend.to_owned())),
                ("EFFIGY_TEST_PS_ROW", None),
            ])
        }

        fn manifest(&self) -> TaskManifest {
            crate::runner::manifest::load_task_manifest(&self.root.join("effigy.toml"))
                .expect("load fixture manifest")
        }

        fn policy(&self, name: &str) -> EffectiveContainerPolicy {
            load_container_policy(&self.root, Some(name)).expect("load fixture policy")
        }

        fn publish_shared_render(&self, policy: &EffectiveContainerPolicy) {
            for render in &policy.render.as_ref().expect("rendered policy").shared {
                effigy_containers::policy::render::publish_shared_render(render)
                    .expect("publish shared render");
            }
        }

        fn calls(&self) -> String {
            fs::read_to_string(&self.log).unwrap_or_default()
        }

        fn clear_calls(&self) {
            let _ = fs::remove_file(&self.log);
        }

        fn set_inventory_row(&self, project: &str, service: &str, digest: &str, status: &str) {
            let row = format!("fake-{service}-1\t{status}\t{digest}\t{project}\t{service}");
            std::env::set_var("EFFIGY_TEST_PS_ROW", &row);
        }

        /// Stage one seed file the way the public data-seed command does, and
        /// return the metadata its built-in task reads.
        fn stage_seed(&self) -> (PathBuf, BTreeMap<String, String>) {
            let staging = self.root.join(".effigy/local/db-seeds");
            fs::create_dir_all(&staging).expect("mkdir staging");
            let staged = staging.join("acowtancy--seed.sql");
            fs::write(&staged, "select 1;\n").expect("write staged seed");
            let metadata = serde_json::json!([{
                "target": "acowtancy",
                "staged_path": ".effigy/local/db-seeds/acowtancy--seed.sql",
            }]);
            let mut env = BTreeMap::new();
            env.insert(DB_SEEDS_JSON_ENV.to_owned(), metadata.to_string());
            (staged, env)
        }

        fn seed_source(&self) -> PathBuf {
            let source = self.root.join("seed-source.sql");
            fs::write(&source, "select 1;\n").expect("write seed source");
            source
        }
    }

    fn shared_binding(
        policy: &EffectiveContainerPolicy,
        manifest: &TaskManifest,
    ) -> SharedServiceBinding {
        resolve_shared_data_binding(policy, manifest, "mysql")
            .expect("resolve shared target")
            .expect("declared shared target")
    }

    fn binding_fixture() -> SharedServiceBinding {
        SharedServiceBinding {
            service_name: "mysql".to_owned(),
            catalog: "mariadb".to_owned(),
            domain_label: "mysql".to_owned(),
            project_name: "effigy-shared-mariadb-0721de3e".to_owned(),
            compose_file: PathBuf::from("/shared/mariadb/abc/compose.yml"),
            host: "127.0.0.1".to_owned(),
            host_port: 33_066,
            container_port: 3306,
            host_env_vars: vec![],
            port_env_vars: vec![],
            render_digest: "abc".to_owned(),
        }
    }

    #[test]
    fn stack_and_hybrid_declarations_resolve_to_one_machine_project() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-identity-", STACK_HYBRID_MANIFEST);
        let _env = repo.env();
        let manifest = repo.manifest();
        let stack = repo.policy("stack");
        let hybrid = repo.policy("hybrid");
        assert_eq!(hybrid.data_from.as_deref(), Some("stack"));

        let stack_binding = shared_binding(&stack, &manifest);
        let hybrid_binding = shared_binding(&hybrid, &manifest);

        assert_eq!(stack_binding.project_name, hybrid_binding.project_name);
        assert_eq!(stack_binding.compose_file, hybrid_binding.compose_file);
        assert_eq!(stack_binding.render_digest, hybrid_binding.render_digest);
        assert!(!stack_binding.render_digest.is_empty());
        assert!(stack_binding
            .project_name
            .starts_with("effigy-shared-mariadb-"));
        // A shared service belongs to no checkout, so stack and hybrid cannot
        // route to their own environments' projects.
        assert_ne!(stack_binding.project_name, stack.project_name);
        assert_ne!(stack_binding.project_name, hybrid.project_name);
        let shared_root = repo.home.join(".effigy/shared-services");
        assert!(
            stack_binding.compose_file.starts_with(&shared_root),
            "shared render belongs to the machine home: {}",
            stack_binding.compose_file.display()
        );
        assert!(
            !stack_binding
                .compose_file
                .components()
                .any(|component| component.as_os_str() == "objects"),
            "shared render is not part of a checkout generation: {}",
            stack_binding.compose_file.display()
        );
    }

    #[test]
    fn unshared_target_keeps_the_checkout_local_route() {
        let _lock = lock_test();
        let repo = FixtureRepo::new(
            "effigy-shared-data-unshared-",
            r#"
[data.targets.acowtancy]
service = "mysql"
database = "acowtancy"

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.mysql]
catalog = "mariadb"
database = "acowtancy"
"#,
        );
        let _env = repo.env();
        let manifest = repo.manifest();
        let policy = repo.policy("web");
        assert!(
            resolve_shared_data_binding(&policy, &manifest, "mysql")
                .expect("resolve target")
                .is_none(),
            "an unshared service must not be redirected"
        );
    }

    #[test]
    fn a_local_service_of_the_same_name_in_another_environment_is_not_hijacked() {
        let _lock = lock_test();
        let repo = FixtureRepo::new(
            "effigy-shared-data-local-sibling-",
            r#"
[data.targets.acowtancy]
service = "mysql"
database = "acowtancy"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.mysql]
catalog = "mariadb"
database = "acowtancy"
shared = true

[containers.hybrid]
primary_service = "app"

[containers.hybrid.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.hybrid.services.mysql]
catalog = "mariadb"
database = "acowtancy"
"#,
        );
        let _env = repo.env();
        let manifest = repo.manifest();
        assert!(
            resolve_shared_data_binding(&repo.policy("stack"), &manifest, "mysql")
                .expect("resolve stack")
                .is_some()
        );
        assert!(
            resolve_shared_data_binding(&repo.policy("hybrid"), &manifest, "mysql")
                .expect("resolve hybrid")
                .is_none(),
            "hybrid declares mysql locally, so a stack shared service cannot capture it"
        );
    }

    #[test]
    fn different_shared_identities_across_environments_are_refused() {
        let _lock = lock_test();
        let repo = FixtureRepo::new(
            "effigy-shared-data-ambiguous-",
            r#"
[data.targets.acowtancy]
service = "mysql"
database = "acowtancy"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "acowtancy"
shared = true

[containers.hybrid]
primary_service = "app"

[containers.hybrid.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.hybrid.services.mysql]
catalog = "mariadb"
version = "11.4"
database = "acowtancy"
shared = true
"#,
        );
        let _env = repo.env();
        let manifest = repo.manifest();
        let error = resolve_shared_data_binding(&repo.policy("stack"), &manifest, "mysql")
            .expect_err("two shared identities cannot resolve to one project");
        let message = error.to_string();
        assert!(
            message.contains("different machine-wide identities"),
            "{message}"
        );
        assert!(message.contains("`hybrid`"), "{message}");
        assert!(message.contains("`stack`"), "{message}");
    }

    #[test]
    fn plan_forwards_the_credential_by_name_only() {
        let binding = binding_fixture();
        let command = vec![
            "env".to_owned(),
            "MYSQL_PWD=secret".to_owned(),
            "mysql".to_owned(),
            "-uroot".to_owned(),
            "acowtancy".to_owned(),
        ];
        let plan = plan_shared_data_exec(&binding, "mysql", &command, Some(Path::new("/seed.sql")))
            .expect("plan");

        assert_eq!(
            plan.env.get("MYSQL_PWD").map(String::as_str),
            Some("secret")
        );
        assert_eq!(plan.command, vec!["mysql", "-uroot", "acowtancy"]);
        assert!(
            !plan.command.iter().any(|word| word.contains("secret")),
            "credential value leaked into the in-container command: {:?}",
            plan.command
        );
        assert_eq!(plan.stdin_file.as_deref(), Some(Path::new("/seed.sql")));
    }

    fn container(
        name: &str,
        service: &str,
        render: Option<&str>,
        running: bool,
    ) -> SharedProjectContainer {
        SharedProjectContainer {
            container_name: name.to_owned(),
            running,
            render: render.map(str::to_owned),
            service: Some(service.to_owned()),
        }
    }

    fn refusal(binding: &SharedServiceBinding, containers: &[SharedProjectContainer]) -> String {
        verify_shared_data_containers(binding, "stack", containers)
            .expect_err("refusal")
            .to_string()
    }

    #[test]
    fn refusal_names_missing_stopped_stale_and_ambiguous_containers() {
        let binding = binding_fixture();

        assert!(verify_shared_data_containers(
            &binding,
            "stack",
            &[container("shared-mysql-1", "mysql", Some("abc"), true)]
        )
        .is_ok());

        let missing = refusal(&binding, &[]);
        assert!(
            missing.contains("no container for service `mysql`"),
            "{missing}"
        );
        assert!(missing.contains("effigy container up stack"), "{missing}");

        let stopped = refusal(
            &binding,
            &[container("shared-mysql-1", "mysql", Some("abc"), false)],
        );
        assert!(stopped.contains("is stopped"), "{stopped}");
        assert!(stopped.contains("never starts or replaces"), "{stopped}");

        let stale = refusal(
            &binding,
            &[container("shared-mysql-1", "mysql", Some("def"), true)],
        );
        assert!(stale.contains("not the declared render `abc`"), "{stale}");
        assert!(stale.contains("Nothing was stopped"), "{stale}");

        let ambiguous = refusal(
            &binding,
            &[
                container("shared-mysql-1", "mysql", Some("abc"), true),
                container("shared-mysql-2", "mysql", Some("abc"), true),
            ],
        );
        assert!(
            ambiguous.contains("more than one running container"),
            "{ambiguous}"
        );

        // A container of the declared project but another service is not this
        // target, whatever its name looks like.
        let other_service = refusal(
            &binding,
            &[container("shared-mysql-1", "dbgate", Some("abc"), true)],
        );
        assert!(
            other_service.contains("no container for service `mysql`"),
            "{other_service}"
        );
    }

    /// The complete shared render must be the bytes this declaration resolves:
    /// a missing, unreadable or altered bound file refuses before any exec.
    #[test]
    fn altered_missing_and_unreadable_render_files_are_refused() {
        let _lock = lock_test();
        for (case, expected) in [
            ("altered", "was altered on disk"),
            ("missing", "is missing"),
            ("unreadable", "cannot be read"),
        ] {
            let repo = FixtureRepo::new(
                &format!("effigy-shared-data-render-{case}-"),
                STACK_HYBRID_MANIFEST,
            );
            repo.install_fake_runtime();
            let _env = repo.env();
            let manifest = repo.manifest();
            let stack = repo.policy("stack");
            repo.publish_shared_render(&stack);
            let binding = shared_binding(&stack, &manifest);
            repo.set_inventory_row(
                &binding.project_name,
                &binding.service_name,
                &binding.render_digest,
                "Up 2 minutes",
            );
            let render = stack
                .render
                .as_ref()
                .expect("rendered")
                .shared
                .iter()
                .find(|render| Some(render.directory.as_path()) == binding.compose_file.parent())
                .expect("expected shared render");
            let (relative, bytes) = render.files.iter().next().expect("bound file");
            let path = render.directory.join(relative);
            match case {
                "altered" => fs::write(&path, b"altered bytes").expect("alter render file"),
                "missing" => fs::remove_file(&path).expect("remove render file"),
                "unreadable" => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))
                        .expect("chmod render file");
                }
                other => panic!("unknown case {other}"),
            }
            repo.clear_calls();
            let error = verify_shared_data_target(&stack, &binding).expect_err("refusal");
            assert!(error.to_string().contains(expected), "{case}: {error}");
            assert_eq!(
                repo.calls(),
                "",
                "{case}: no runtime call before the refusal"
            );
            // Leave the fixture readable for the fixture's own cleanup.
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
            let _ = fs::write(&path, bytes);
        }
    }

    /// The whole seed and dump path against a fake runtime: reset, stdin import
    /// and logical dump all exec in the declared shared project's own render,
    /// for both `stack` and `hybrid`.
    #[test]
    fn seed_and_dump_run_in_the_shared_project_for_stack_and_hybrid() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-route-", STACK_HYBRID_MANIFEST);
        repo.install_fake_runtime();
        let _env = repo.env();
        let manifest = repo.manifest();
        let stack = repo.policy("stack");
        let hybrid = repo.policy("hybrid");
        repo.publish_shared_render(&stack);
        repo.publish_shared_render(&hybrid);
        let binding = shared_binding(&stack, &manifest);
        assert_eq!(binding, shared_binding(&hybrid, &manifest));
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            &binding.render_digest,
            "Up 2 minutes",
        );

        for (policy, dump_name) in [(&stack, "stack"), (&hybrid, "hybrid")] {
            repo.clear_calls();
            let (staged, seed_env) = repo.stage_seed();
            run_builtin_db_seed_task_with(
                &repo.root,
                &manifest,
                policy,
                &seed_env,
                |policy, binding, command, stdin_file| {
                    run_shared_data_command(policy, binding, command, stdin_file)
                },
            )
            .expect("seed through the shared project");
            assert!(!staged.exists(), "staged seed must be cleaned on success");
            let calls = repo.calls();
            let exec_calls = calls
                .lines()
                .filter(|line| line.contains(" exec "))
                .collect::<Vec<_>>();
            assert_eq!(exec_calls.len(), 2, "reset and import exec: {calls}");
            for line in exec_calls {
                assert!(
                    line.contains(&binding.project_name),
                    "shared project missing from {line}"
                );
                assert!(
                    line.contains(&binding.compose_file.display().to_string()),
                    "shared render missing from {line}"
                );
                assert!(!line.contains("secret"), "credential in argv: {line}");
                assert!(
                    line.contains("exec -T -e MYSQL_PWD mysql"),
                    "the credential must travel by name: {line}"
                );
            }
            assert!(
                calls.contains("credential-env=present"),
                "the private transport must put the value in the Compose client's environment: {calls}"
            );
            assert!(calls.contains("DROP DATABASE IF EXISTS"), "{calls}");
            assert!(calls.contains("stdin select 1;"), "{calls}");
            let reset = calls.find("DROP DATABASE IF EXISTS").expect("reset call");
            let import = calls.find("stdin select 1;").expect("import call");
            assert!(reset < import, "reset must precede the import: {calls}");

            repo.clear_calls();
            let dump_path = repo.root.join(format!("{dump_name}.sql"));
            let rendered = run_container_data_dump_with_hooks(
                &repo.root,
                Some(&policy.name),
                &[effigy_cli::ContainerDbDumpInput {
                    target: Some("acowtancy".to_owned()),
                    path: dump_path.clone(),
                }],
                false,
                true,
                |_, _, _, _| unreachable!("a shared target must not exec locally"),
                |policy, binding, command, stdin_file| {
                    run_shared_data_command(policy, binding, command, stdin_file)
                },
                |_, _, _, _| Ok(serde_json::Value::Null),
            )
            .expect("dump through the shared project");
            let report: serde_json::Value = serde_json::from_str(&rendered).expect("json report");
            assert_eq!(report["ok"], true);
            assert_eq!(report["dumps"][0]["service"], "mysql");
            assert!(
                !rendered.contains("secret"),
                "credential in report: {rendered}"
            );
            // The whole dump, not a bounded prefix: the adapter's deadline
            // path retains complete output for the artifact.
            let dump = fs::read(&dump_path).expect("dump artifact");
            assert!(dump.starts_with(b"dump-body\n"), "dump prefix");
            assert_eq!(
                dump.len(),
                "dump-body\n".len() + 12_000 * 101,
                "complete dump"
            );

            let calls = repo.calls();
            assert!(
                calls.contains(&binding.project_name),
                "shared project missing from {calls}"
            );
            assert!(
                calls.contains(&binding.compose_file.display().to_string()),
                "shared render missing from {calls}"
            );
            assert!(calls.contains("mysqldump"), "{calls}");
            assert!(calls.contains("credential-env=present"), "{calls}");
            assert!(!calls.contains("secret"), "credential in argv: {calls}");
        }
    }

    /// The Colima adapter runs the shared exec through the VM tool, and the
    /// credential still crosses only as the container's own variable name.
    #[test]
    fn shared_exec_uses_the_colima_vm_adapter_without_a_credential_value() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-colima-", STACK_HYBRID_MANIFEST);
        repo.install_fake_runtime();
        let _env = repo.env_with_backend("colima");
        let manifest = repo.manifest();
        let stack = repo.policy("stack");
        repo.publish_shared_render(&stack);
        let binding = shared_binding(&stack, &manifest);
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            &binding.render_digest,
            "Up 2 minutes",
        );

        let (staged, seed_env) = repo.stage_seed();
        run_builtin_db_seed_task_with(
            &repo.root,
            &manifest,
            &stack,
            &seed_env,
            |policy, binding, command, stdin_file| {
                run_shared_data_command(policy, binding, command, stdin_file)
            },
        )
        .expect("seed through the shared project on colima");
        assert!(!staged.exists());
        let calls = repo.calls();
        assert!(
            calls.contains(&format!("call ssh --profile {} -- sh -c", stack.profile)),
            "colima VM transport missing: {calls}"
        );
        assert!(
            calls.contains("exec sudo --preserve-env=MYSQL_PWD nerdctl"),
            "the VM must preserve the credential name into nerdctl: {calls}"
        );
        assert!(
            calls.contains(&format!("'-f' '{}'", binding.compose_file.display())),
            "the VM must run the shared render: {calls}"
        );
        assert!(
            calls.contains(&format!("'-p' '{}'", binding.project_name))
                && calls.contains("'exec' '-T' '-e' 'MYSQL_PWD' 'mysql'"),
            "the VM must exec the declared service with the credential name: {calls}"
        );
        assert!(calls.contains("credential-file=present"), "{calls}");
        assert!(!calls.contains("secret"), "credential in argv: {calls}");
        assert!(
            !calls.contains("MYSQL_ROOT_PASSWORD"),
            "the credential is not read from the container env: {calls}"
        );
    }

    #[test]
    fn seed_refuses_before_any_statement_when_the_shared_target_is_not_proven() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-refusal-", STACK_HYBRID_MANIFEST);
        repo.install_fake_runtime();
        let _env = repo.env();
        let manifest = repo.manifest();
        let stack = repo.policy("stack");
        let binding = shared_binding(&stack, &manifest);

        // Missing render: refused without consulting the runtime or publishing.
        repo.clear_calls();
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            &binding.render_digest,
            "Up 2 minutes",
        );
        let (staged, seed_env) = repo.stage_seed();
        let error = run_builtin_db_seed_task_with(
            &repo.root,
            &manifest,
            &stack,
            &seed_env,
            |policy, binding, command, stdin_file| {
                run_shared_data_command(policy, binding, command, stdin_file)
            },
        )
        .expect_err("unpublished render must refuse");
        assert!(error.to_string().contains("is missing"), "{error}");
        assert!(
            error.to_string().contains("to publish the shared render"),
            "{error}"
        );
        assert_eq!(repo.calls(), "", "no runtime call before the refusal");
        assert!(staged.exists(), "staging custody is unchanged on refusal");

        // Stopped container: refused, and nothing was executed.
        repo.publish_shared_render(&stack);
        repo.clear_calls();
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            &binding.render_digest,
            "Exited (0) 2 minutes ago",
        );
        let error = run_builtin_db_seed_task_with(
            &repo.root,
            &manifest,
            &stack,
            &seed_env,
            |policy, binding, command, stdin_file| {
                run_shared_data_command(policy, binding, command, stdin_file)
            },
        )
        .expect_err("stopped shared service must refuse");
        assert!(error.to_string().contains("is stopped"), "{error}");
        assert!(!repo.calls().contains(" exec "), "{}", repo.calls());

        // Stale render digest: refused rather than replaced.
        repo.clear_calls();
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            "stale-digest",
            "Up 2 minutes",
        );
        let error = run_builtin_db_seed_task_with(
            &repo.root,
            &manifest,
            &stack,
            &seed_env,
            |policy, binding, command, stdin_file| {
                run_shared_data_command(policy, binding, command, stdin_file)
            },
        )
        .expect_err("stale shared render must refuse");
        assert!(
            error.to_string().contains("not the declared render"),
            "{error}"
        );
        assert!(!repo.calls().contains(" exec "), "{}", repo.calls());
    }

    /// The public `container data seed` path proves the shared target before
    /// any environment activation, so a stopped shared service refuses instead
    /// of being started or replaced by the seed.
    #[test]
    fn public_seed_proves_a_shared_target_before_activation() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-public-refusal-", STACK_HYBRID_MANIFEST);
        repo.install_fake_runtime();
        let _env = repo.env();
        let manifest = repo.manifest();
        let stack = repo.policy("stack");
        repo.publish_shared_render(&stack);
        let binding = shared_binding(&stack, &manifest);
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            &binding.render_digest,
            "Exited (0) 2 minutes ago",
        );
        let source = repo.seed_source();

        let error = run_container_data_seed(
            &repo.root,
            None,
            &[effigy_cli::BootstrapDbSeedInput {
                target: Some("acowtancy".to_owned()),
                path: source,
            }],
            true,
            false,
            true,
        )
        .expect_err("a stopped shared service must refuse the public seed");
        assert!(error.to_string().contains("is stopped"), "{error}");
        let calls = repo.calls();
        assert!(
            !calls.contains(" up ") && !calls.contains(" start "),
            "the seed must not activate or reconcile the shared service: {calls}"
        );
        assert!(
            !calls.contains(" exec "),
            "no statement may run before the proof: {calls}"
        );
    }

    /// The public seed runs the environment `--name` selected, not the
    /// manifest default: only `hybrid` declares the shared mysql.
    #[test]
    fn public_seed_routes_the_selected_hybrid_environment() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-public-hybrid-", HYBRID_ONLY_MANIFEST);
        repo.install_fake_runtime();
        let _env = repo.env();
        let manifest = repo.manifest();
        let hybrid = repo.policy("hybrid");
        repo.publish_shared_render(&hybrid);
        let binding = shared_binding(&hybrid, &manifest);
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            &binding.render_digest,
            "Up 2 minutes",
        );
        let source = repo.seed_source();

        let rendered = run_container_data_seed(
            &repo.root,
            Some("hybrid"),
            &[effigy_cli::BootstrapDbSeedInput {
                target: Some("acowtancy".to_owned()),
                path: source,
            }],
            true,
            false,
            true,
        )
        .expect("public seed through hybrid's shared mysql");
        let report: serde_json::Value = serde_json::from_str(&rendered).expect("json report");
        assert_eq!(report["container"], "hybrid");
        assert_eq!(report["ok"], true);

        let calls = repo.calls();
        let exec_calls = calls
            .lines()
            .filter(|line| line.contains(" exec "))
            .collect::<Vec<_>>();
        assert_eq!(exec_calls.len(), 2, "{calls}");
        for line in exec_calls {
            assert!(
                line.contains(&binding.project_name),
                "hybrid's shared project missing from {line}"
            );
            assert!(!line.contains("secret"), "credential in argv: {line}");
            assert!(
                line.contains("exec -T -e MYSQL_PWD mysql"),
                "the credential must travel by name: {line}"
            );
        }
        assert!(calls.contains("credential-env=present"), "{calls}");
        assert!(
            !calls.contains(" up ") && !calls.contains(" start "),
            "a shared seed activates nothing: {calls}"
        );
    }

    const UNSHARED_TARGET_NO_SHARED_SERVICES_MANIFEST: &str = r#"
[data.targets.acowtancy]
service = "postgres"
database = "acowtancy"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"
working_dir = "/workspace"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.postgres]
catalog = "postgres"
database = "acowtancy"
"#;

    const UNSHARED_TARGET_MANIFEST: &str = r#"
[data.targets.acowtancy]
service = "postgres"
database = "acowtancy"

[containers]
default = "stack"

[containers.stack]
primary_service = "app"

[containers.stack.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.stack.services.postgres]
catalog = "postgres"
database = "acowtancy"

[containers.stack.services.mysql]
catalog = "mariadb"
version = "10.11"
database = "acowtancy"
password = "secret"
shared = true
"#;

    fn assert_no_shared_activity(
        case: &str,
        repo: &FixtureRepo,
        shared: &SharedServiceBinding,
        render_before: &[u8],
    ) {
        let calls = repo.calls();
        for forbidden in [" up ", " start ", " stop ", " rm "] {
            assert!(
                !calls.contains(forbidden),
                "{case}: the seed must not {forbidden:?} anything: {calls}"
            );
        }
        assert!(
            !calls.contains(&shared.project_name),
            "{case}: the unrelated shared project must not be touched: {calls}"
        );
        assert_eq!(
            fs::read(&shared.compose_file).expect("shared render"),
            render_before,
            "{case}: the shared render must not be republished"
        );
    }

    /// The same unshared target in an environment with **no** shared services:
    /// the seed keeps its prior behaviour and activates the stopped local
    /// environment instead of refusing (effigy#477 review).
    #[test]
    fn public_unshared_seed_still_activates_an_environment_without_shared_services() {
        let _lock = lock_test();
        let repo = FixtureRepo::new(
            "effigy-shared-data-unshared-no-shared-",
            UNSHARED_TARGET_NO_SHARED_SERVICES_MANIFEST,
        );
        repo.install_fake_runtime();
        let _env = repo.env();
        let stack = repo.policy("stack");
        assert!(stack.shared_services.is_empty());
        repo.clear_calls();
        let source = repo.seed_source();

        let result = run_container_data_seed(
            &repo.root,
            None,
            &[effigy_cli::BootstrapDbSeedInput {
                target: Some("acowtancy".to_owned()),
                path: source,
            }],
            true,
            false,
            true,
        );
        let calls = repo.calls();
        if let Err(error) = &result {
            let message = error.to_string();
            assert!(
                !message.contains("must not start it because bringing it up would reconcile"),
                "an environment with no shared services must not be refused: {message}"
            );
        }
        // Prior behaviour: the seed's activation brings the stopped local
        // stack up. The fake runtime starts nothing, so the seed then fails at
        // readiness, which is the point: it was never refused for sharing.
        assert!(
            calls.contains(&format!("-p {} up -d", stack.project_name)),
            "the seed must activate the stopped local environment: {calls}"
        );
    }

    /// A public seed for an unshared target must never start, publish or
    /// replace an unrelated shared service during its activation (effigy#477
    /// review). The environment is stopped, so the seed refuses before any
    /// bring-up; the unrelated shared container stays stopped and its published
    /// render stays byte-identical.
    #[test]
    fn public_unshared_seed_never_touches_an_unrelated_shared_service() {
        let _lock = lock_test();
        let repo = FixtureRepo::new(
            "effigy-shared-data-unshared-builtin-",
            UNSHARED_TARGET_MANIFEST,
        );
        repo.install_fake_runtime();
        let _env = repo.env();
        let stack = repo.policy("stack");
        let shared = stack
            .shared_services
            .first()
            .expect("the unrelated shared service")
            .clone();
        repo.publish_shared_render(&stack);
        let render_before = fs::read(&shared.compose_file).expect("published shared render");
        repo.set_inventory_row(
            &shared.project_name,
            &shared.service_name,
            &shared.render_digest,
            "Exited (0) 2 minutes ago",
        );
        repo.clear_calls();
        let source = repo.seed_source();

        let error = run_container_data_seed(
            &repo.root,
            None,
            &[effigy_cli::BootstrapDbSeedInput {
                target: Some("acowtancy".to_owned()),
                path: source,
            }],
            true,
            false,
            true,
        )
        .expect_err("a stopped environment must refuse instead of reconciling shared services");
        let message = error.to_string();
        assert!(message.contains("is not running"), "{message}");
        assert!(message.contains("effigy container up stack"), "{message}");
        assert_no_shared_activity("builtin", &repo, &shared, &render_before);
    }

    /// An app-defined `data:seed` task likewise never touches the unrelated
    /// shared service. Its activation runs through the data-seed session
    /// context, which refuses shared reconciliation.
    #[test]
    fn public_app_defined_seed_never_touches_an_unrelated_shared_service() {
        let _lock = lock_test();
        let manifest =
            format!("{UNSHARED_TARGET_MANIFEST}\n[tasks.\"data:seed\"]\nrun = \"true\"\n");
        let repo = FixtureRepo::new("effigy-shared-data-unshared-app-task-", &manifest);
        repo.install_fake_runtime();
        let _env = repo.env();
        let stack = repo.policy("stack");
        let shared = stack
            .shared_services
            .first()
            .expect("the unrelated shared service")
            .clone();
        repo.publish_shared_render(&stack);
        let render_before = fs::read(&shared.compose_file).expect("published shared render");
        repo.set_inventory_row(
            &shared.project_name,
            &shared.service_name,
            &shared.render_digest,
            "Exited (0) 2 minutes ago",
        );
        repo.clear_calls();
        let source = repo.seed_source();

        run_container_data_seed(
            &repo.root,
            None,
            &[effigy_cli::BootstrapDbSeedInput {
                target: Some("acowtancy".to_owned()),
                path: source,
            }],
            true,
            false,
            true,
        )
        .expect("the app-defined seed task runs without shared reconciliation");
        assert_no_shared_activity("app-task", &repo, &shared, &render_before);
    }

    /// Two concurrent data commands never share a credential file, so neither
    /// can source a partially written value (effigy#477 review).
    #[test]
    fn concurrent_shared_seeds_keep_their_credentials_separate() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-concurrent-", STACK_HYBRID_MANIFEST);
        repo.install_fake_runtime();
        let _env = repo.env();
        let manifest = repo.manifest();
        let stack = repo.policy("stack");
        repo.publish_shared_render(&stack);
        let binding = shared_binding(&stack, &manifest);
        repo.set_inventory_row(
            &binding.project_name,
            &binding.service_name,
            &binding.render_digest,
            "Up 2 minutes",
        );
        let command = vec![
            "env".to_owned(),
            "MYSQL_PWD=secret".to_owned(),
            "mysql".to_owned(),
            "-uroot".to_owned(),
            "-e".to_owned(),
            "DROP DATABASE IF EXISTS `acowtancy`; CREATE DATABASE `acowtancy`;".to_owned(),
        ];
        let handles = (0..4)
            .map(|_| {
                let policy = stack.clone();
                let binding = binding.clone();
                let command = command.clone();
                std::thread::spawn(move || {
                    run_shared_data_command(&policy, &binding, &command, None)
                        .expect("concurrent shared exec")
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().expect("thread");
        }
        let calls = repo.calls();
        assert_eq!(
            calls.lines().filter(|line| line.contains(" exec ")).count(),
            4,
            "{calls}"
        );
        assert_eq!(
            calls.matches("credential-env=present").count(),
            4,
            "every exec receives the value through its own file: {calls}"
        );
        assert!(!calls.contains("secret"), "credential in argv: {calls}");
        let leftovers = fs::read_dir(repo.home.join(".effigy/data-credentials"))
            .expect("credentials directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("credential entries");
        assert!(
            leftovers.is_empty(),
            "each exec removes its own credential file"
        );
    }

    #[test]
    fn dump_refuses_when_the_shared_runtime_is_unreadable() {
        let _lock = lock_test();
        let repo = FixtureRepo::new("effigy-shared-data-unreadable-", STACK_HYBRID_MANIFEST);
        repo.install_unreadable_runtime();
        let _env = repo.env();
        let stack = repo.policy("stack");
        repo.publish_shared_render(&stack);
        // A runtime whose inventory read fails must refuse rather than fall
        // back to a local service of the same name.
        let error = run_container_data_dump_with_hooks(
            &repo.root,
            Some("stack"),
            &[effigy_cli::ContainerDbDumpInput {
                target: Some("acowtancy".to_owned()),
                path: repo.root.join("stack.sql"),
            }],
            false,
            true,
            |_, _, _, _| unreachable!("a shared target must not exec locally"),
            |policy, binding, command, stdin_file| {
                run_shared_data_command(policy, binding, command, stdin_file)
            },
            |_, _, _, _| Ok(serde_json::Value::Null),
        )
        .expect_err("unreadable shared runtime must refuse");
        assert!(
            error.to_string().contains("cannot read the runtime"),
            "{error}"
        );
        assert!(!repo.root.join("stack.sql").exists());
    }

    #[test]
    fn local_dump_never_calls_the_shared_route() {
        let _lock = lock_test();
        let repo = FixtureRepo::new(
            "effigy-shared-data-local-dump-",
            r#"
[data.targets.acowtancy]
service = "mysql"
database = "acowtancy"

[containers]
default = "web"

[containers.web]
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"

[containers.web.services.mysql]
catalog = "mariadb"
database = "acowtancy"
"#,
        );
        let _env = repo.env();
        let dump_path = repo.root.join("app.sql");
        let rendered = run_container_data_dump_with_hooks(
            &repo.root,
            Some("web"),
            &[effigy_cli::ContainerDbDumpInput {
                target: Some("acowtancy".to_owned()),
                path: dump_path.clone(),
            }],
            false,
            true,
            |_, _, service, _| {
                assert_eq!(service, Some("mysql"));
                Ok(Output {
                    status: std::os::unix::process::ExitStatusExt::from_raw(0),
                    stdout: b"local-body\n".to_vec(),
                    stderr: Vec::new(),
                })
            },
            |_, _, _, _| unreachable!("an unshared target must keep the checkout-local route"),
            |_, _, _, _| Ok(serde_json::Value::Null),
        )
        .expect("local dump");
        let report: serde_json::Value = serde_json::from_str(&rendered).expect("json");
        assert_eq!(report["ok"], true);
        assert_eq!(
            fs::read_to_string(&dump_path).expect("dump artifact"),
            "local-body\n"
        );
    }
}
