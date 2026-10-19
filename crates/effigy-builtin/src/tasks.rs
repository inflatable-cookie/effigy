use std::path::Path;

use effigy_cli::{HelpTopic, TaskInvocation, TasksArgs};

use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_topic;
use super::migrate;
use crate::BuiltinError;
use crate::BuiltinRuntimePorts;
use effigy_tasks::TaskRuntimeArgs;
#[path = "tasks/request.rs"]
mod request;

pub(super) fn run_builtin_tasks(
    ports: &dyn BuiltinRuntimePorts,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    _catalogs: &[effigy_manifest::LoadedCatalog],
    _invocation_cwd: &Path,
) -> Result<Option<String>, BuiltinError> {
    if let Some(nested) = nested_tasks_builtin(task, runtime_args) {
        return match nested {
            NestedTasksBuiltin::Migrate { task, args } => {
                migrate::run_builtin_migrate(&task, &args, target_root)
            }
            NestedTasksBuiltin::Retired(message) => Err(BuiltinError::task_invocation(message)),
        };
    }

    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| render_builtin_help_topic(HelpTopic::Tasks, "tasks", output_json),
        |args| request::parse_tasks_request(task, args),
        |request: request::TasksRequest| {
            if !request.output_json && !request.pretty_json {
                return Err(BuiltinError::task_invocation(format!(
                    "`--pretty` is only supported together with `--json` for built-in `{}`",
                    task.name
                )));
            }
            ports
                .run_tasks(TasksArgs {
                    repo_override: Some(target_root.to_path_buf()),
                    task_name: request.task_name,
                    resolve_selector: request.resolve_selector,
                    status_selector: request.status_selector,
                    status_all: request.status_all,
                    output_json: request.output_json,
                    pretty_json: request.pretty_json,
                    request: None,
                    qa: None,
                })
                .map(Some)
        },
    )
}

/// `tasks unlock` has no job: a run's gate is released by the kernel when
/// its holder exits, and is never stolen from a live one.
pub(crate) const RETIRED_UNLOCK: &str = "`effigy tasks unlock` was removed in v0.15. A task's lock is \
    its run's kernel gate: it is released the moment that run exits, including after a crash or \
    SIGKILL, and is never taken from a live run. Find the live holder with \
    `effigy tasks status <task>`; wait for it with `effigy --lock-wait-ms <ms> <task>`, or stop \
    that run";
pub(crate) const RETIRED_CACHE: &str = "`effigy tasks cache` was removed in v0.15 with the task \
    cache. Effigy no longer skips tasks from a fingerprint; let the tool's own cache decide, and \
    delete `.effigy/cache`";

enum NestedTasksBuiltin {
    Migrate {
        task: TaskInvocation,
        args: Vec<String>,
    },
    /// Removed in v0.15; one-release refusal naming the replacement.
    Retired(&'static str),
}

fn nested_tasks_builtin(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
) -> Option<NestedTasksBuiltin> {
    let (subcommand, tail) = runtime_args.passthrough.split_first()?;
    match subcommand.as_str() {
        "migrate" => Some(NestedTasksBuiltin::Migrate {
            task: nested_task(task, "migrate"),
            args: tail.to_vec(),
        }),
        "unlock" => Some(NestedTasksBuiltin::Retired(RETIRED_UNLOCK)),
        "cache" => Some(NestedTasksBuiltin::Retired(RETIRED_CACHE)),
        _ => None,
    }
}

fn nested_task(task: &TaskInvocation, subcommand: &str) -> TaskInvocation {
    TaskInvocation {
        name: format!("{} {subcommand}", task.name),
        args: Vec::new(),
    }
}
