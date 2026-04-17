use super::parser_task;
use crate::runner::tests::prelude::{
    assert_parser_task_invocation_error, parse_config_contract_request, string_args,
    ConfigParseContract,
};

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
            inspect: false,
            inspect_path: None,
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
            inspect: false,
            inspect_path: None,
            schema: false,
            minimal: false,
            output_json: false,
            target: None,
            runner: None,
        }
    );

    let parsed = parse_config_contract_request(
        &task,
        &string_args(&[
            "--inspect",
            "--path",
            "docs_policy.indexes.vision",
            "--json",
        ]),
    )
    .expect("config parse");
    assert_eq!(
        parsed,
        ConfigParseContract {
            inspect: true,
            inspect_path: Some("docs_policy.indexes.vision".to_owned()),
            schema: false,
            minimal: false,
            output_json: true,
            target: None,
            runner: None,
        }
    );

    assert_parser_task_invocation_error(
        parse_config_contract_request(&task, &string_args(&["--schema", "--runner", "jest"])),
        "invalid `--runner` value `jest`",
    );
}
