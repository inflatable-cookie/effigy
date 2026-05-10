use std::path::Path;

use effigy_cli::{HelpTopic, TaskInvocation, TasksArgs};

use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_topic;
use super::{cache, migrate, unlock};
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
    catalogs: &[effigy_manifest::LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<Option<String>, BuiltinError> {
    if let Some(nested) = nested_tasks_builtin(task, runtime_args) {
        return match nested {
            NestedTasksBuiltin::Migrate { task, args } => {
                migrate::run_builtin_migrate(&task, &args, target_root)
            }
            NestedTasksBuiltin::Unlock { task, args } => {
                unlock::run_builtin_unlock(ports, &task, &args, target_root)
            }
            NestedTasksBuiltin::Cache { task, runtime_args } => cache::run_builtin_cache(
                ports,
                &task,
                &runtime_args,
                target_root,
                catalogs,
                invocation_cwd,
            ),
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
                })
                .map(Some)
        },
    )
}

enum NestedTasksBuiltin {
    Migrate {
        task: TaskInvocation,
        args: Vec<String>,
    },
    Unlock {
        task: TaskInvocation,
        args: Vec<String>,
    },
    Cache {
        task: TaskInvocation,
        runtime_args: TaskRuntimeArgs,
    },
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
        "unlock" => Some(NestedTasksBuiltin::Unlock {
            task: nested_task(task, "unlock"),
            args: tail.to_vec(),
        }),
        "cache" => Some(NestedTasksBuiltin::Cache {
            task: nested_task(task, "cache"),
            runtime_args: nested_runtime_args(runtime_args, tail),
        }),
        _ => None,
    }
}

fn nested_task(task: &TaskInvocation, subcommand: &str) -> TaskInvocation {
    TaskInvocation {
        name: format!("{} {subcommand}", task.name),
        args: Vec::new(),
    }
}

fn nested_runtime_args(runtime_args: &TaskRuntimeArgs, tail: &[String]) -> TaskRuntimeArgs {
    TaskRuntimeArgs {
        repo_override: runtime_args.repo_override.clone(),
        verbose_root: runtime_args.verbose_root,
        env_schema_override: runtime_args.env_schema_override.clone(),
        passthrough: tail.to_vec(),
    }
}
