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
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn run_builtin_command_short_circuits_parse_and_execute_for_help() {
        let args = vec!["--help".to_owned()];
        let parse_calls = RefCell::new(0usize);
        let execute_calls = RefCell::new(0usize);
        let out = run_builtin_command(
            &args,
            |_| Ok("help".to_owned()),
            || {
                *parse_calls.borrow_mut() += 1;
                Ok("parsed")
            },
            |_| {
                *execute_calls.borrow_mut() += 1;
                Ok(Some("executed".to_owned()))
            },
        )
        .expect("help path should succeed");
        assert_eq!(out, Some("help".to_owned()));
        assert_eq!(*parse_calls.borrow(), 0);
        assert_eq!(*execute_calls.borrow(), 0);
    }

    #[test]
    fn run_builtin_command_runs_parse_and_execute_without_help() {
        let args = vec!["--json".to_owned()];
        let parse_calls = RefCell::new(0usize);
        let execute_calls = RefCell::new(0usize);
        let out = run_builtin_command(
            &args,
            |_| Ok("help".to_owned()),
            || {
                *parse_calls.borrow_mut() += 1;
                Ok("parsed")
            },
            |_| {
                *execute_calls.borrow_mut() += 1;
                Ok(Some("executed".to_owned()))
            },
        )
        .expect("execute path should succeed");
        assert_eq!(out, Some("executed".to_owned()));
        assert_eq!(*parse_calls.borrow(), 1);
        assert_eq!(*execute_calls.borrow(), 1);
    }
}
