use crate::BuiltinError;

use super::*;

#[test]
fn context_string_flag_value_missing_message_contract_is_stable() {
    let mut parser = BuiltinArgParser::new(&[]);
    let err = parser
        .context_string_flag_value("task", "--task")
        .expect_err("missing flag value should fail");
    assert_task_invocation(err, "task argument --task requires a value");
}

#[test]
fn context_bool_literal_flag_value_message_contract_is_stable() {
    let mut parser = BuiltinArgParser::new(&[]);
    let err = parser
        .context_bool_literal_flag_value("tasks", "--pretty")
        .expect_err("missing bool literal should fail");
    assert_task_invocation(
        err,
        "tasks argument --pretty requires a value (`true` or `false`)",
    );

    let args = vec!["invalid".to_owned()];
    let mut invalid_parser = BuiltinArgParser::new(&args);
    let err = invalid_parser
        .context_bool_literal_flag_value("tasks", "--pretty")
        .expect_err("invalid bool literal should fail");
    assert_task_invocation(
        err,
        "tasks argument --pretty value `invalid` is invalid (expected `true` or `false`)",
    );
}

#[test]
fn quoted_choice_flag_value_message_contract_is_stable() {
    let mut parser = BuiltinArgParser::new(&[]);
    let err = parser
        .quoted_choice_flag_value("--owner", "`effigy` or `external`", |_| None::<()>)
        .expect_err("missing enum choice should fail");
    assert_task_invocation(err, "`--owner` requires a value (`effigy` or `external`)");

    let args = vec!["wat".to_owned()];
    let mut invalid_parser = BuiltinArgParser::new(&args);
    let err = invalid_parser
        .quoted_choice_flag_value("--owner", "`effigy` or `external`", |value| match value {
            "effigy" | "external" => Some(()),
            _ => None,
        })
        .expect_err("invalid enum choice should fail");
    assert_task_invocation(
        err,
        "invalid `--owner` value `wat` (expected `effigy` or `external`)",
    );
}

#[test]
fn builtin_choice_flag_value_message_contract_is_stable() {
    let mut parser = BuiltinArgParser::new(&[]);
    let err = parser
        .builtin_choice_flag_value(
            "config",
            "--target",
            "package_manager, test",
            |_| None::<()>,
        )
        .expect_err("missing builtin choice should fail");
    assert_task_invocation(err, "`--target` requires a value for built-in `config`");

    let args = vec!["wat".to_owned()];
    let mut invalid_parser = BuiltinArgParser::new(&args);
    let err = invalid_parser
        .builtin_choice_flag_value("config", "--target", "package_manager, test", |value| {
            BuiltinArgParser::choice_ignore_ascii_case(
                value,
                &[("package_manager", ()), ("test", ())],
            )
        })
        .expect_err("invalid builtin choice should fail");
    assert_task_invocation(
        err,
        "invalid `--target` value `wat` for built-in `config` (supported: package_manager, test)",
    );
}

#[test]
fn choice_ignore_ascii_case_contract_is_stable() {
    let matched = BuiltinArgParser::choice_ignore_ascii_case(
        "CaRgO-NeXtEsT",
        &[("vitest", 1u8), ("cargo-nextest", 2u8)],
    );
    assert_eq!(matched, Some(2));

    let unmatched = BuiltinArgParser::choice_ignore_ascii_case("unknown", &[("vitest", 1u8)]);
    assert_eq!(unmatched, None);
}

#[test]
fn flag_string_value_missing_message_contract_is_stable() {
    let mut parser = BuiltinArgParser::new(&[]);
    let err = parser
        .flag_string_value("--from", "a file path")
        .expect_err("missing flag value should fail");
    assert_task_invocation(err, "`--from` requires a file path");
}

#[test]
fn first_positional_arg_contract_is_stable() {
    let args = vec![
        "--json".to_owned(),
        "--pretty".to_owned(),
        "candidates".to_owned(),
        "--prefix".to_owned(),
    ];
    assert_eq!(
        BuiltinArgParser::first_positional_arg(&args),
        Some("candidates")
    );
}

#[test]
fn required_and_unknown_subcommand_message_contracts_are_stable() {
    let mut parser = BuiltinArgParser::new(&[]);
    let err = parser
        .required_subcommand("cache", "`inspect` or `invalidate`")
        .expect_err("missing subcommand should fail");
    assert_task_invocation(
        err,
        "`cache` requires a subcommand: `inspect` or `invalidate`",
    );

    assert_eq!(
        BuiltinArgParser::builtin_unknown_subcommand_message(
            "cache",
            "drop",
            "`inspect` or `invalidate`"
        ),
        "unknown cache subcommand `drop` (expected `inspect` or `invalidate`)",
    );
}

#[test]
fn parse_loop_collect_unknown_contract_is_stable() {
    let args = vec![
        "--json".to_owned(),
        "--wat".to_owned(),
        "target".to_owned(),
        "--after-break".to_owned(),
    ];
    let mut parser = BuiltinArgParser::new(&args);
    let mut json = false;
    let mut target = None::<String>;
    let unknown = parser
        .parse_loop_collect_unknown(|parser, arg| {
            if parser.consume_json_flag(arg, &mut json) {
                return Ok(ParseLoopAction::Handled);
            }
            if arg.starts_with('-') {
                return Ok(ParseLoopAction::Unknown);
            }
            target = Some(arg.to_owned());
            Ok(ParseLoopAction::Break)
        })
        .expect("parse loop should succeed");

    assert!(json);
    assert_eq!(target.as_deref(), Some("target"));
    assert_eq!(unknown, vec!["--wat".to_owned()]);
    assert_eq!(parser.remaining(), &["--after-break".to_owned()]);
}

#[test]
fn parse_loop_require_no_unknown_with_prefix_contract_is_stable() {
    let args = vec!["--wat".to_owned()];
    let mut parser = BuiltinArgParser::new(&args);
    let err = parser
        .parse_loop_require_no_unknown_with_prefix("completion", "candidates", |_parser, _arg| {
            Ok(ParseLoopAction::Unknown)
        })
        .expect_err("unknown prefixed parse loop should fail");
    assert_task_invocation(
        err,
        "unknown argument(s) for built-in `completion`: candidates --wat",
    );
}

#[test]
fn positional_task_invocation_contract_is_stable() {
    let args = vec!["target".to_owned(), "--flag".to_owned(), "value".to_owned()];
    let mut parser = BuiltinArgParser::new(&args);
    let name = parser.next().expect("first positional arg");
    let invocation = parser.positional_task_invocation(name);
    assert_eq!(invocation.name, "target");
    assert_eq!(
        invocation.args,
        vec!["--flag".to_owned(), "value".to_owned()]
    );
}

#[test]
fn unknown_if_flag_or_contract_is_stable() {
    let parser = BuiltinArgParser::new(&[]);

    let unknown = parser
        .unknown_if_flag_or("--wat", |_| Ok(ParseLoopAction::Handled))
        .expect("flag path should succeed");
    assert!(matches!(unknown, ParseLoopAction::Unknown));

    let handled = parser
        .unknown_if_flag_or("target", |_| Ok(ParseLoopAction::Handled))
        .expect("positional path should succeed");
    assert!(matches!(handled, ParseLoopAction::Handled));
}

#[test]
fn consume_any_bool_flag_contract_is_stable() {
    let parser = BuiltinArgParser::new(&[]);
    let mut output_json = false;
    let mut fix = false;
    let mut verbose = false;

    assert!(parser.consume_any_bool_flag(
        "--fix",
        &mut [
            ("--json", &mut output_json),
            ("--fix", &mut fix),
            ("--verbose", &mut verbose),
        ],
    ));
    assert!(!output_json);
    assert!(fix);
    assert!(!verbose);

    assert!(!parser.consume_any_bool_flag(
        "--wat",
        &mut [
            ("--json", &mut output_json),
            ("--fix", &mut fix),
            ("--verbose", &mut verbose),
        ],
    ));
}

fn assert_task_invocation(error: BuiltinError, expected: &str) {
    match error {
        BuiltinError::TaskInvocation(message) => assert_eq!(message, expected),
        other => panic!("expected BuiltinError::TaskInvocation, received: {other}"),
    }
}
