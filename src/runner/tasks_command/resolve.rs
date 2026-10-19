//! `effigy tasks --resolve <selector>`: the route the one resolver gives a
//! selector, exactly as execution would take it (D1-10). A typed command
//! wins before any catalog; then the task, built-in, exec alias or deferral
//! that [`resolve_route`] chooses. Pure read: it plans, it never runs.

use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use serde_json::{json, Value};

use crate::runner::execute::route::{resolve_route, Route};
use crate::runner::task_engine::TaskTarget;

pub(super) fn resolve_probe(
    raw_selector: &str,
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
    deferred_builtins: &std::collections::BTreeSet<String>,
) -> Value {
    let (selector, selector_args) =
        match effigy_tasks::parse_task_reference_invocation(raw_selector) {
            Ok(parsed) => parsed,
            Err(error) => return error_probe(raw_selector, raw_selector, error),
        };
    let task_name = selector.task_name.clone();
    if selector.prefix.is_none() && is_typed_command(&task_name, deferred_builtins) {
        return json!({
            "selector": raw_selector,
            "status": "ok",
            "route": "typed-command",
            "catalog": null,
            "catalog_root": null,
            "task": task_name,
            "lock_scopes": [],
            "evidence": [format!("`{task_name}` is the built-in command `effigy {task_name}`; it runs before any catalog task of that name")],
            "error": null,
        });
    }
    let mut args = Vec::new();
    if let Some(repo) = repo_override {
        args.extend(["--repo".to_owned(), repo.display().to_string()]);
    }
    args.extend(selector_args.iter().cloned());
    let invocation = TaskInvocation {
        name: effigy_tasks::render_task_selector(&selector),
        args,
    };
    let preflight = match crate::runner::execute::api::build_execution_preflight(&invocation, cwd) {
        Ok(preflight) => preflight,
        Err(error) => return error_probe(raw_selector, &task_name, error.to_string()),
    };
    match resolve_route(&invocation, &preflight) {
        Ok(Route::Task { selection, .. }) => {
            let target = TaskTarget::for_selection(
                &preflight.resolved.resolved_root,
                &task_name,
                &selection,
            );
            let mut evidence = selection.evidence.clone();
            let mut lock_scopes = vec![format!("task:{}", target.label)];
            if let Some(lock) = &target.shared_lock {
                lock_scopes.push(format!("shared:{lock}"));
            }
            if selection.task.mode.as_deref() == Some("tui") {
                let profile = selector_args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "default".to_owned());
                if !effigy_tasks::has_concurrent_profile(selection.task, &profile) {
                    let available = effigy_tasks::available_concurrent_profiles(selection.task);
                    return error_probe(
                        raw_selector,
                        &task_name,
                        format!(
                            "managed profile `{profile}` not found for task `{task_name}`; available: {}",
                            if available.is_empty() {
                                "<none>".to_owned()
                            } else {
                                available.join(", ")
                            }
                        ),
                    );
                }
                evidence.push(format!(
                    "managed profile `{profile}` resolved via invocation `{raw_selector}`"
                ));
            }
            json!({
                "selector": raw_selector,
                "status": "ok",
                "route": "task",
                "catalog": selection.catalog.alias,
                "catalog_root": selection.catalog.catalog_root.display().to_string(),
                "task": task_name,
                "task_id": {
                    "root": preflight.resolved.resolved_root.display().to_string(),
                    "catalog": target.catalog,
                    "task": target.task,
                    "selector": target.label,
                },
                "lock_scopes": lock_scopes,
                "evidence": evidence,
                "error": null,
            })
        }
        Ok(Route::Defer { deferral, .. }) => json!({
            "selector": raw_selector,
            "status": "ok",
            "route": "defer",
            "catalog": deferral.source,
            "catalog_root": deferral.working_dir.display().to_string(),
            "task": task_name,
            "lock_scopes": [],
            "evidence": [format!("no task matched; `[defer]` from {} receives the request", deferral.source)],
            "error": null,
        }),
        Ok(route) => json!({
            "selector": raw_selector,
            "status": "ok",
            "route": route.kind(),
            "catalog": null,
            "catalog_root": null,
            "task": task_name,
            "lock_scopes": [],
            "evidence": [match route {
                Route::Builtin => format!("no task matched; built-in `{task_name}` runs"),
                _ => format!("no task matched; exec alias `{task_name}` runs in its container"),
            }],
            "error": null,
        }),
        Err(error) => error_probe(raw_selector, &task_name, error.to_string()),
    }
}

/// A word the CLI parses as a typed command, unless the root remaps it to
/// the repository with `[defer].builtins`.
fn is_typed_command(word: &str, deferred_builtins: &std::collections::BTreeSet<String>) -> bool {
    if deferred_builtins.contains(word) {
        return false;
    }
    !matches!(
        effigy_cli::parse_command([word.to_owned()]),
        Ok(effigy_cli::Command::Task(_))
    )
}

fn error_probe(raw_selector: &str, task: &str, error: String) -> Value {
    json!({
        "selector": raw_selector,
        "status": "error",
        "route": "error",
        "catalog": null,
        "catalog_root": null,
        "task": task,
        "lock_scopes": [],
        "evidence": [],
        "error": error,
    })
}
