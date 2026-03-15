use super::super::prelude::{
    assert_parser_task_invocation_error, parse_unlock_contract_request, string_args,
};
use super::parser_task;

#[test]
fn builtin_unlock_parser_contracts_are_stable() {
    let task = parser_task();

    let parsed = parse_unlock_contract_request(&task, &string_args(&["--all", "--json"]))
        .expect("unlock parse");
    assert!(parsed.output_json);
    assert!(parsed.unlock_all_flag);
    assert!(parsed.scopes.is_empty());

    let parsed = parse_unlock_contract_request(
        &task,
        &string_args(&[
            "workspace",
            "shared:dev-stack",
            "task:dev",
            "profile:dev/admin",
        ]),
    )
    .expect("unlock parse");
    assert!(!parsed.output_json);
    assert!(!parsed.unlock_all_flag);
    assert_eq!(
        parsed.scopes,
        string_args(&[
            "workspace",
            "shared:dev-stack",
            "task:dev",
            "profile:dev/admin"
        ])
    );

    assert_parser_task_invocation_error(
        parse_unlock_contract_request(&task, &string_args(&[])),
        "`unlock` requires at least one scope (or `--all`)",
    );
}
