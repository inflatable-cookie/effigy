use super::super::prelude::{
    assert_parser_task_invocation_error, parse_config_contract_request, string_args,
    ConfigParseContract,
};
use super::parser_task;

#[test]
fn builtin_config_parser_contracts_are_stable() {
    let task = parser_task();

    let parsed = parse_config_contract_request(
        &task,
        &string_args(&[
            "--schema",
            "--minimal",
            "--target",
            "test",
            "--runner",
            "nextest",
            "--json",
        ]),
    )
    .expect("config parse");
    assert_eq!(
        parsed,
        ConfigParseContract {
            schema: true,
            minimal: true,
            output_json: true,
            target: Some("test"),
            runner: Some("cargo-nextest"),
        }
    );

    let parsed = parse_config_contract_request(&task, &string_args(&[])).expect("config parse");
    assert_eq!(
        parsed,
        ConfigParseContract {
            schema: false,
            minimal: false,
            output_json: false,
            target: None,
            runner: None,
        }
    );

    assert_parser_task_invocation_error(
        parse_config_contract_request(&task, &string_args(&["--schema", "--runner", "jest"])),
        "invalid `--runner` value `jest`",
    );
}
