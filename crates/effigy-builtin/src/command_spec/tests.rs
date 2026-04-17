use std::cell::RefCell;

use super::run_builtin_command;

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
