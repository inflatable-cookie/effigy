use crate::runner::container_command::support::{
    resolve_repo_root_or_invocation_cwd_scope, ContainerRepoScope,
};
use crate::runner::container_command::RunnerError;
use effigy_containers::load_container_policy_for_inspection;
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
            let rendered = annotate_fs_events_status(&repo_root, name, output_json, rendered);
            Ok(annotate_host_process_status(
                &repo_root,
                name,
                output_json,
                rendered,
            ))
        }
        ContainerRepoScope::InvocationCwd(cwd) => {
            // Several environments may answer here, so no single relay field
            // belongs to this report.
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

/// The report's `fs_events` field, refined by what only the host knows: the
/// backend that makes a relay unnecessary, and a stack relay run whose
/// supervisor is gone.
fn annotate_fs_events_status(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
    rendered: String,
) -> String {
    let Ok(policy) = load_container_policy_for_inspection(repo_root, name) else {
        return rendered;
    };
    let status = crate::runner::fs_relay::observed_status(repo_root, &policy);
    if output_json {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&rendered) else {
            return rendered;
        };
        let (Some(object), Ok(status)) = (value.as_object_mut(), serde_json::to_value(&status))
        else {
            return rendered;
        };
        object.insert("fs_events".to_owned(), status);
        return value.to_string();
    }
    format!("{rendered}\nfs_events: {}", status.summary())
}

/// Each declared host process is a kernel run: status reports its run, state
/// and last exit beside the services (052 §10.2).
pub(super) fn annotate_host_process_status(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
    rendered: String,
) -> String {
    let Ok(policy) = load_container_policy_for_inspection(repo_root, name) else {
        return rendered;
    };
    let rows = crate::runner::host_process::host_process_status(repo_root, &policy);
    let gateway_routes = crate::runner::container_command::gateway_registration::gateway_route_table_path()
        .ok()
        .and_then(|path| effigy_gateway::routes::RouteTable::load(&path).ok())
        .map(|table| {
            table
                .all_routes()
                .into_iter()
                .filter(|route| route.project == repo_root.to_string_lossy())
                .map(|route| {
                    let exposure = route.tls.then(|| {
                        crate::runner::container_command::gateway_registration::observe_tls_route_exposure(
                            &route.domain,
                        )
                    });
                    serde_json::json!({
                        "domain": route.domain,
                        "tls": route.tls,
                        "exposure": exposure.map(|value| value.as_str()),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if rows.is_empty() && gateway_routes.is_empty() {
        return rendered;
    }
    if output_json {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&rendered) else {
            return rendered;
        };
        let Some(object) = value.as_object_mut() else {
            return rendered;
        };
        if !rows.is_empty() {
            object.insert(
                "host_processes".to_owned(),
                serde_json::to_value(&rows).unwrap_or(serde_json::Value::Null),
            );
        }
        return annotate_gateway_route_status(value.to_string(), gateway_routes, true);
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
    annotate_gateway_route_status(text, gateway_routes, false)
}

pub(super) fn annotate_gateway_route_status(
    rendered: String,
    gateway_routes: Vec<serde_json::Value>,
    output_json: bool,
) -> String {
    if gateway_routes.is_empty() {
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
            "gateway_routes".to_owned(),
            serde_json::Value::Array(gateway_routes),
        );
        return value.to_string();
    }
    let mut text = rendered;
    for route in gateway_routes {
        text.push('\n');
        match route["exposure"].as_str() {
            Some(exposure) => text.push_str(&format!(
                "[gateway] {} TLS exposure {exposure}",
                route["domain"].as_str().unwrap_or("unknown"),
            )),
            None => text.push_str(&format!(
                "[gateway] {} registered",
                route["domain"].as_str().unwrap_or("unknown"),
            )),
        }
    }
    text
}
