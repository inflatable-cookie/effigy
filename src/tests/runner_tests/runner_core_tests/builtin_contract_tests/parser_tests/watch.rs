use super::parser_task;
use crate::runner::tests::prelude::{
    assert_parser_task_invocation_error, parse_watch_contract_request, string_args,
};

#[test]
fn builtin_watch_parser_contracts_are_stable() {
    let task = parser_task();

    let parsed = parse_watch_contract_request(
        &task,
        &string_args(&[
            "--json",
            "--owner",
            "effigy",
            "--debounce-ms",
            "250",
            "--include",
            "src/**",
            "--exclude",
            "target/**",
            "--max-runs",
            "3",
            "build",
            "--",
            "--watch",
        ]),
    )
    .expect("watch parse");

    assert!(parsed.output_json);
    assert_eq!(parsed.owner, Some("effigy"));
    assert_eq!(parsed.debounce_ms, 250);
    assert_eq!(parsed.max_runs, Some(3));
    assert_eq!(parsed.target_name.as_deref(), Some("build"));
    assert_eq!(parsed.include, string_args(&["src/**"]));
    assert_eq!(parsed.exclude, string_args(&["target/**"]));
    assert_eq!(parsed.target_args, string_args(&["--", "--watch"]));

    assert_parser_task_invocation_error(
        parse_watch_contract_request(&task, &string_args(&["--owner"])),
        "`--owner` requires a value (`effigy` or `external`)",
    );
}
