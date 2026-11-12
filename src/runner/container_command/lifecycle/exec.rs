use super::exec_operation_plan;
use crate::runner::container_command::shell_prep::{
    append_container_exec_env, maybe_refresh_workspace_effigy_for_shell,
    resolve_container_exec_working_dir_for_operation, resolve_container_shell_session,
};
use crate::runner::container_command::RunnerError;
use crate::runner::container_runtime::CONTAINER_HANDOFF_ENV_ASSIGNMENT;
use crate::runner::exec_command::{append_color_exec_env, run_compose_exec_plan_with_options};
use effigy_containers::{
    ContainerAction, ContainerCapturedExecOperation, ContainerComposeInvocationPlan,
    ContainerExecOperation, EffectiveContainerPolicy,
};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

pub(in crate::runner) fn run_container_exec_capture_with_options(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    run_container_exec_operation_capture(
        repo_root,
        name,
        ContainerCapturedExecOperation {
            service: service.map(str::to_owned),
            command: command.to_vec(),
            stdin_file: stdin_file.map(Path::to_path_buf),
            cwd: None,
            env: BTreeMap::new(),
        },
    )
}

/// Run one captured exec whose client credential travels by variable name only
/// (effigy#487). The value reaches the spawned command's environment, never its
/// argv: a checkout-local handoff puts it in the direct command's environment,
/// and every Compose route sources it from a private file the bootstrap reads.
pub(in crate::runner) fn run_container_exec_capture_with_credentials(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
    credential_env: &BTreeMap<String, String>,
    stdin_file: Option<&Path>,
) -> Result<Output, RunnerError> {
    if credential_env.is_empty() {
        return run_container_exec_capture_with_options(
            repo_root, name, service, command, stdin_file,
        );
    }
    if command.is_empty() {
        return Err(RunnerError::task_invocation(
            "container_exec requires at least one command argument",
        ));
    }
    // Inside the workspace container the requested command is already local:
    // the values go into its environment and never into its argv.
    if let Some(resolved) =
        crate::runner::container_runtime::resolve_handoff_local_execution(repo_root, name, service)
    {
        let execution = resolved?;
        let env = credential_env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        return crate::runner::container_runtime::run_handoff_local_direct_command_with_options(
            &execution, command, None, stdin_file, &env, true,
        );
    }

    let (policy, service, _) = resolve_container_shell_session(repo_root, name, service)?;
    let _operation_plan = exec_operation_plan(
        repo_root,
        &policy,
        ContainerExecOperation::captured(
            Some(service.clone()),
            command.to_vec(),
            stdin_file.map(Path::to_path_buf),
        ),
    );
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy, &service)?;
    let credential_names = credential_env
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let plan = captured_exec_compose_plan(
        repo_root,
        name,
        &policy,
        &service,
        command,
        None,
        &BTreeMap::new(),
        &credential_names,
    )?;
    crate::runner::exec_command::run_compose_exec_plan_with_credentials(
        &policy,
        &plan,
        stdin_file,
        credential_env,
    )
}

pub(in crate::runner) fn run_container_exec_operation_capture(
    repo_root: &Path,
    name: Option<&str>,
    operation: ContainerCapturedExecOperation,
) -> Result<Output, RunnerError> {
    run_container_exec_operation(repo_root, name, operation, true)
}

pub(in crate::runner) fn run_container_exec_stream(
    repo_root: &Path,
    name: Option<&str>,
    service: Option<&str>,
    command: &[String],
) -> Result<Output, RunnerError> {
    run_container_exec_operation(
        repo_root,
        name,
        ContainerCapturedExecOperation {
            service: service.map(str::to_owned),
            command: command.to_vec(),
            stdin_file: None,
            cwd: None,
            env: BTreeMap::new(),
        },
        false,
    )
}

fn run_container_exec_operation(
    repo_root: &Path,
    name: Option<&str>,
    operation: ContainerCapturedExecOperation,
    capture: bool,
) -> Result<Output, RunnerError> {
    if operation.command.is_empty() {
        return Err(RunnerError::task_invocation(
            "container_exec requires at least one command argument",
        ));
    }
    // Inside the workspace container the requested command is already
    // local: run it directly instead of probing a host runtime
    // (colima/nerdctl/docker) that is unreachable from inside.
    if let Some(resolved) = crate::runner::container_runtime::resolve_handoff_local_execution(
        repo_root,
        name,
        operation.service.as_deref(),
    ) {
        let execution = resolved?;
        let env = operation
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string_lossy().into_owned()))
            .collect::<Vec<_>>();
        let mut argv = Vec::with_capacity(operation.command.len());
        argv.extend(operation.command.iter().cloned());
        return crate::runner::container_runtime::run_handoff_local_direct_command_with_options(
            &execution,
            &argv,
            operation.cwd.as_deref(),
            operation.stdin_file.as_deref(),
            &env,
            capture,
        );
    }

    let (policy, service, _) =
        resolve_container_shell_session(repo_root, name, operation.service.as_deref())?;
    let _operation_plan = exec_operation_plan(
        repo_root,
        &policy,
        ContainerExecOperation::captured(
            Some(service.clone()),
            operation.command.clone(),
            operation.stdin_file.clone(),
        ),
    );
    maybe_refresh_workspace_effigy_for_shell(repo_root, &policy, &service)?;
    let plan = captured_exec_compose_plan(
        repo_root,
        name,
        &policy,
        &service,
        &operation.command,
        operation.cwd.as_deref(),
        &operation.env,
        &[],
    )?;
    run_compose_exec_plan_with_options(&policy, &plan, capture, operation.stdin_file.as_deref())
}

/// Build the Compose `exec` plan for one captured operation. The project,
/// service, working directory and color/handoff environment are the same for
/// every caller; `credential_names` adds name-only `-e NAME` passthroughs whose
/// values the caller keeps out of argv.
#[allow(clippy::too_many_arguments)]
fn captured_exec_compose_plan(
    repo_root: &Path,
    name: Option<&str>,
    policy: &EffectiveContainerPolicy,
    service: &str,
    command: &[String],
    cwd: Option<&Path>,
    env: &BTreeMap<String, OsString>,
    credential_names: &[&str],
) -> Result<ContainerComposeInvocationPlan, RunnerError> {
    let mut args = vec![OsString::from("exec"), OsString::from("-T")];
    if let Some(working_dir) =
        resolve_container_exec_working_dir_for_operation(repo_root, name, policy, service, cwd)?
    {
        args.push(OsString::from("-w"));
        args.push(OsString::from(working_dir));
    }
    append_color_exec_env(&mut args, false);
    append_container_exec_env(&mut args, env);
    for key in credential_names {
        args.push(OsString::from("-e"));
        args.push(OsString::from(*key));
    }
    args.push(OsString::from("-e"));
    args.push(OsString::from(CONTAINER_HANDOFF_ENV_ASSIGNMENT));
    args.push(OsString::from(service));
    args.extend(command.iter().map(OsString::from));
    effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args,
        ContainerAction::Exec,
        "docker compose exec",
    )
    .map_err(RunnerError::from)
}

/// Proof for effigy#487: the captured data exec plan passes each database
/// credential by variable name only, for every environment declaration, and
/// keeps the declared Compose project.
#[cfg(test)]
mod credential_plan_tests {
    use super::captured_exec_compose_plan;
    use crate::contract_test_support::EnvGuard;
    use effigy_containers::load_container_policy;
    use std::collections::BTreeMap;
    use std::fs;

    const MANIFEST: &str = r#"
[containers]
default = "stack"

[containers.stack]
primary_service = "db"
project_name = "credential-stack"
working_dir = "/workspace"

[containers.stack.services.db]
catalog = "mariadb"
version = "10.11"

[containers.hybrid]
primary_service = "db"
project_name = "credential-hybrid"
working_dir = "/workspace"

[containers.hybrid.services.db]
catalog = "postgres"
version = "16"
"#;

    fn repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempfile::tempdir().expect("temporary fixture root");
        let home = temp.path().join("home");
        fs::create_dir_all(&home).expect("mkdir home");
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).expect("mkdir repo");
        fs::write(repo.join("effigy.toml"), MANIFEST).expect("write manifest");
        (temp, repo)
    }

    #[test]
    fn captured_data_exec_plan_passes_credentials_by_name_only() {
        let (_temp, repo) = repo();
        let home = repo.parent().expect("parent").join("home");
        let _env = EnvGuard::set_many(&[
            ("HOME", Some(home.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ]);
        let command = vec![
            "mysqldump".to_owned(),
            "-uroot".to_owned(),
            "acowtancy".to_owned(),
        ];

        for (environment, project, credential_name) in [
            ("stack", "credential-stack", "MYSQL_PWD"),
            ("hybrid", "credential-hybrid", "PGPASSWORD"),
        ] {
            let policy = load_container_policy(&repo, Some(environment)).expect("policy");
            let plan = captured_exec_compose_plan(
                &repo,
                Some(environment),
                &policy,
                "db",
                &command,
                None,
                &BTreeMap::new(),
                &[credential_name],
            )
            .expect("plan");
            let args = plan
                .args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();

            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "-e" && pair[1] == credential_name),
                "the credential name must be passed through: {args:?}"
            );
            assert!(
                !args
                    .iter()
                    .any(|arg| arg.starts_with(&format!("{credential_name}="))),
                "the value must never appear in the argv: {args:?}"
            );
            assert!(
                args.windows(2)
                    .any(|pair| pair[0] == "-p" && pair[1] == project),
                "the declared Compose project must be unchanged: {args:?}"
            );
            assert_eq!(
                args.windows(2)
                    .filter(|pair| pair[0] == "-e" && pair[1] == credential_name)
                    .count(),
                1,
                "exactly one name-only passthrough per credential: {args:?}"
            );
        }
    }
}
