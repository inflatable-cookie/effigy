use super::super::{RunnerError, TaskRuntimeArgs};
use super::response::run_help_then;
use super::support::reject_verbose_root_for_builtin;

pub(super) fn run_builtin_command<P, H, A, E>(
    args: &[String],
    render_help: H,
    parse: A,
    execute: E,
) -> Result<Option<String>, RunnerError>
where
    H: FnOnce(bool) -> Result<String, RunnerError>,
    A: FnOnce() -> Result<P, RunnerError>,
    E: FnOnce(P) -> Result<Option<String>, RunnerError>,
{
    run_help_then(args, render_help, || {
        let parsed = parse()?;
        execute(parsed)
    })
}

pub(super) fn run_passthrough_builtin_command<P, H, A, E>(
    task_name: &str,
    runtime_args: &TaskRuntimeArgs,
    render_help: H,
    parse: A,
    execute: E,
) -> Result<Option<String>, RunnerError>
where
    H: FnOnce(bool) -> Result<String, RunnerError>,
    A: FnOnce(&[String]) -> Result<P, RunnerError>,
    E: FnOnce(P) -> Result<Option<String>, RunnerError>,
{
    reject_verbose_root_for_builtin(task_name, runtime_args)?;
    let passthrough = &runtime_args.passthrough;
    run_builtin_command(passthrough, render_help, || parse(passthrough), execute)
}

#[cfg(test)]
#[path = "command_spec/tests.rs"]
mod tests;
