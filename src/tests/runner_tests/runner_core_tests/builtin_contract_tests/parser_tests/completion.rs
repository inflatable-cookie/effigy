use super::parser_task;
use crate::runner::tests::prelude::{
    assert_parser_task_invocation_error, parse_completion_contract_request, string_args,
    CompletionParseContract,
};

#[test]
fn builtin_completion_parser_contracts_are_stable() {
    let task = parser_task();

    let candidates =
        parse_completion_contract_request(&task, &string_args(&["candidates", "--prefix", "api"]))
            .expect("completion parse");
    match candidates {
        CompletionParseContract::Candidates => {}
        CompletionParseContract::Shell { .. } => {
            panic!("expected completion candidates parser mode")
        }
    }

    let shell =
        parse_completion_contract_request(&task, &string_args(&["zsh", "--export", "--json"]))
            .expect("completion parse");
    match shell {
        CompletionParseContract::Candidates => panic!("expected completion shell parser mode"),
        CompletionParseContract::Shell {
            output_json,
            shell,
            action,
        } => {
            assert!(output_json);
            assert_eq!(shell, Some("zsh"));
            assert_eq!(action, Some("export"));
        }
    }

    assert_parser_task_invocation_error(
        parse_completion_contract_request(&task, &string_args(&["wat"])),
        "invalid shell `wat` for `builtin-parse` (expected `bash`, `zsh`, `fish`, or `candidates`)",
    );
}
