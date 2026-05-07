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
            bundle: None,
            runner: Some("cargo-nextest"),
            user_inspect: false,
            user_path: false,
            user_get: None,
            set_container_backend: None,
            set_container_profile: None,
            unset_container_backend: false,
            unset_container_profile: false,
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
            bundle: None,
            runner: None,
            user_inspect: false,
            user_path: false,
            user_get: None,
            set_container_backend: None,
            set_container_profile: None,
            unset_container_backend: false,
            unset_container_profile: false,
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
            bundle: None,
            runner: None,
            user_inspect: false,
            user_path: false,
            user_get: None,
            set_container_backend: None,
            set_container_profile: None,
            unset_container_backend: false,
            unset_container_profile: false,
        }
    );

    let parsed = parse_config_contract_request(
        &task,
        &string_args(&["--schema", "--target", "bundle", "--bundle", "decodelabs"]),
    )
    .expect("config parse");
    assert_eq!(
        parsed,
        ConfigParseContract {
            inspect: false,
            inspect_path: None,
            schema: true,
            minimal: false,
            output_json: false,
            target: Some("bundle"),
            bundle: Some("decodelabs".to_owned()),
            runner: None,
            user_inspect: false,
            user_path: false,
            user_get: None,
            set_container_backend: None,
            set_container_profile: None,
            unset_container_backend: false,
            unset_container_profile: false,
        }
    );

    let parsed = parse_config_contract_request(&task, &string_args(&["--user-inspect", "--json"]))
        .expect("config parse");
    assert_eq!(
        parsed,
        ConfigParseContract {
            inspect: false,
            inspect_path: None,
            schema: false,
            minimal: false,
            output_json: true,
            target: None,
            bundle: None,
            runner: None,
            user_inspect: true,
            user_path: false,
            user_get: None,
            set_container_backend: None,
            set_container_profile: None,
            unset_container_backend: false,
            unset_container_profile: false,
        }
    );

    let parsed = parse_config_contract_request(
        &task,
        &string_args(&["get", "containers.backend", "--json"]),
    )
    .expect("config parse");
    assert_eq!(
        parsed,
        ConfigParseContract {
            inspect: false,
            inspect_path: None,
            schema: false,
            minimal: false,
            output_json: true,
            target: None,
            bundle: None,
            runner: None,
            user_inspect: false,
            user_path: false,
            user_get: Some("containers.backend"),
            set_container_backend: None,
            set_container_profile: None,
            unset_container_backend: false,
            unset_container_profile: false,
        }
    );

    let parsed = parse_config_contract_request(
        &task,
        &string_args(&[
            "--set-container-backend",
            "containerd",
            "--set-container-profile",
            "effigy",
        ]),
    )
    .expect("config parse");
    assert_eq!(
        parsed,
        ConfigParseContract {
            inspect: false,
            inspect_path: None,
            schema: false,
            minimal: false,
            output_json: false,
            target: None,
            bundle: None,
            runner: None,
            user_inspect: false,
            user_path: false,
            user_get: None,
            set_container_backend: Some("containerd"),
            set_container_profile: Some("effigy".to_owned()),
            unset_container_backend: false,
            unset_container_profile: false,
        }
    );

    assert_parser_task_invocation_error(
        parse_config_contract_request(&task, &string_args(&["--schema", "--runner", "jest"])),
        "invalid `--runner` value `jest`",
    );
    assert_parser_task_invocation_error(
        parse_config_contract_request(&task, &string_args(&["get", "containers.unknown"])),
        "unknown config get key `containers.unknown`",
    );
}
