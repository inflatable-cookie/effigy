use crate::tests::prelude::{parse_command, BootstrapArgs, Command};

fn parse(args: &[&str]) -> Result<Command, effigy_cli::CliParseError> {
    parse_command(args.iter().map(|arg| (*arg).to_owned()))
}

fn bootstrap_args(command: Command) -> BootstrapArgs {
    match command {
        Command::Bootstrap(args) => args,
        other => panic!("expected bootstrap command, got {other:?}"),
    }
}
#[test]
fn parse_bootstrap_accepts_a_global_json_flag() {
    let args = bootstrap_args(effigy_cli::apply_global_json_flag(
        parse(&["bootstrap", "--plan"]).expect("parse should succeed"),
        true,
    ));
    assert!(args.output_json);
    assert!(args.plan);
}
#[test]
fn parse_bootstrap_missing_flag_values_fail() {
    for flag in ["--path", "--branch"] {
        let error = parse(&["bootstrap", "./repo", flag]).expect_err("missing value");
        assert!(
            error.to_string().contains("requires a value"),
            "{flag}: {error}"
        );
    }
}
#[test]
fn parse_bootstrap_rejects_unknown_flags() {
    let error = parse(&["bootstrap", "--bogus"]).expect_err("unknown flag");
    assert!(error.to_string().contains("--bogus"), "{error}");
}
