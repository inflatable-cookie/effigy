use crate::runner::container_command::support::{
    resolve_repo_root_or_invocation_cwd_scope, ContainerRepoScope,
};
use crate::runner::container_command::RunnerError;
use effigy_containers::load_container_policy;
use effigy_runtime::read::{
    run_container_status, run_container_status_all, run_container_status_under_path,
};

pub(in crate::runner::container_command) fn run_container_status_command(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    global: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    if global {
        if repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container status --global` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_status_all(output_json).map_err(Into::into);
    }

    match resolve_repo_root_or_invocation_cwd_scope(repo_override)? {
        ContainerRepoScope::RepoRoot(repo_root) => {
            let rendered = run_container_status(&repo_root, name, output_json)?;
            Ok(annotate_host_process_status(
                &repo_root,
                name,
                output_json,
                rendered,
            ))
        }
        ContainerRepoScope::InvocationCwd(cwd) => {
            let rendered = run_container_status_under_path(&cwd, name, output_json)?;
            Ok(annotate_host_process_status(
                &cwd,
                name,
                output_json,
                rendered,
            ))
        }
    }
}

/// Each declared host process is a kernel run: status reports its run, state
/// and last exit beside the services (052 §10.2).
pub(super) fn annotate_host_process_status(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
    rendered: String,
) -> String {
    let Ok(policy) = load_container_policy(repo_root, name) else {
        return rendered;
    };
    if policy.host_processes.is_empty() {
        return rendered;
    }
    let rows = crate::runner::host_process::host_process_status(repo_root, &policy);
    if rows.is_empty() {
        return rendered;
    }
    if output_json {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&rendered) else {
            return rendered;
        };
        let Some(object) = value.as_object_mut() else {
            return rendered;
        };
        object.insert(
            "host_processes".to_owned(),
            serde_json::to_value(&rows).unwrap_or(serde_json::Value::Null),
        );
        return value.to_string();
    }
    let mut text = rendered;
    for row in rows {
        text.push('\n');
        text.push_str(&format!(
            "[host process] {} {}",
            row.name,
            row.state.as_str()
        ));
        if let Some(run) = &row.run {
            text.push_str(&format!(" (run {run}"));
            if let Some(pid) = row.pid {
                text.push_str(&format!(", pid {pid}"));
            }
            if let Some(exit) = row.last_exit {
                text.push_str(&format!(", last exit {exit}"));
            }
            text.push(')');
        }
        if let Some(listener) = &row.listener {
            text.push_str(&format!(" listener {}", listener.as_str()));
        }
        if let Some(log) = &row.log {
            text.push_str(&format!(" log {log}"));
        }
        if let Some(diagnostic) = &row.diagnostic {
            text.push_str(&format!(": {diagnostic}"));
        }
    }
    text
}
