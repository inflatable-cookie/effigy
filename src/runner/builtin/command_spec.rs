use super::super::RunnerError;
use super::response::run_help_then;

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

#[cfg(test)]
#[path = "command_spec/tests.rs"]
mod tests;
