use super::prelude::*;

#[test]
fn builtin_parser_contract_table_is_stable() {
    enum CaseKind {
        WatchOk {
            args: &'static [&'static str],
            output_json: bool,
            owner: Option<&'static str>,
            debounce_ms: u64,
            max_runs: Option<usize>,
            target_name: Option<&'static str>,
            include: &'static [&'static str],
            exclude: &'static [&'static str],
            target_args: &'static [&'static str],
        },
        CompletionOkCandidates {
            args: &'static [&'static str],
        },
        CompletionOkShell {
            args: &'static [&'static str],
            output_json: bool,
            shell: Option<&'static str>,
        },
        UnlockOk {
            args: &'static [&'static str],
            output_json: bool,
            unlock_all_flag: bool,
            scopes: &'static [&'static str],
        },
        ConfigOk {
            args: &'static [&'static str],
            schema: bool,
            minimal: bool,
            output_json: bool,
            target: Option<&'static str>,
            runner: Option<&'static str>,
        },
        WatchErr {
            args: &'static [&'static str],
            expected: &'static str,
        },
        CompletionErr {
            args: &'static [&'static str],
            expected: &'static str,
        },
        UnlockErr {
            args: &'static [&'static str],
            expected: &'static str,
        },
        ConfigErr {
            args: &'static [&'static str],
            expected: &'static str,
        },
    }

    let cases = [
        CaseKind::WatchOk {
            args: &[
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
            ],
            output_json: true,
            owner: Some("effigy"),
            debounce_ms: 250,
            max_runs: Some(3),
            target_name: Some("build"),
            include: &["src/**"],
            exclude: &["target/**"],
            target_args: &["--", "--watch"],
        },
        CaseKind::CompletionOkCandidates {
            args: &["candidates", "--prefix", "api"],
        },
        CaseKind::CompletionOkShell {
            args: &["zsh", "--json"],
            output_json: true,
            shell: Some("zsh"),
        },
        CaseKind::UnlockOk {
            args: &["--all", "--json"],
            output_json: true,
            unlock_all_flag: true,
            scopes: &[],
        },
        CaseKind::UnlockOk {
            args: &["workspace", "task:dev", "profile:dev/admin"],
            output_json: false,
            unlock_all_flag: false,
            scopes: &["workspace", "task:dev", "profile:dev/admin"],
        },
        CaseKind::ConfigOk {
            args: &[
                "--schema",
                "--minimal",
                "--target",
                "test",
                "--runner",
                "nextest",
                "--json",
            ],
            schema: true,
            minimal: true,
            output_json: true,
            target: Some("test"),
            runner: Some("cargo-nextest"),
        },
        CaseKind::ConfigOk {
            args: &[],
            schema: false,
            minimal: false,
            output_json: false,
            target: None,
            runner: None,
        },
        CaseKind::WatchErr {
            args: &["--owner"],
            expected: "`--owner` requires a value (`effigy` or `external`)",
        },
        CaseKind::CompletionErr {
            args: &["wat"],
            expected:
                "invalid shell `wat` for `completion` (expected `bash`, `zsh`, `fish`, or `candidates`)",
        },
        CaseKind::UnlockErr {
            args: &[],
            expected: "`unlock` requires at least one scope (or `--all`)",
        },
        CaseKind::ConfigErr {
            args: &["--schema", "--runner", "jest"],
            expected: "invalid `--runner` value `jest`",
        },
    ];

    let task = TaskInvocation {
        name: "builtin-parse".to_owned(),
        args: Vec::new(),
    };

    assert_case_table(cases, |case| match case {
        CaseKind::WatchOk {
            args,
            output_json,
            owner,
            debounce_ms,
            max_runs,
            target_name,
            include,
            exclude,
            target_args,
        } => {
            let parsed =
                parse_watch_contract_request(&task, &string_args(args)).expect("watch parse");
            assert_eq!(parsed.output_json, output_json);
            assert_eq!(parsed.owner, owner);
            assert_eq!(parsed.debounce_ms, debounce_ms);
            assert_eq!(parsed.max_runs, max_runs);
            assert_eq!(parsed.target_name.as_deref(), target_name);
            assert_eq!(parsed.include, string_args(include));
            assert_eq!(parsed.exclude, string_args(exclude));
            assert_eq!(parsed.target_args, string_args(target_args));
        }
        CaseKind::CompletionOkCandidates { args } => {
            let parsed = parse_completion_contract_request(&task, &string_args(args))
                .expect("completion parse");
            match parsed {
                CompletionParseContract::Candidates => {}
                CompletionParseContract::Shell { .. } => {
                    panic!("expected completion candidates parser mode")
                }
            }
        }
        CaseKind::CompletionOkShell {
            args,
            output_json,
            shell,
        } => {
            let parsed = parse_completion_contract_request(&task, &string_args(args))
                .expect("completion parse");
            match parsed {
                CompletionParseContract::Candidates => {
                    panic!("expected completion shell parser mode")
                }
                CompletionParseContract::Shell {
                    output_json: parsed_json,
                    shell: parsed_shell,
                } => {
                    assert_eq!(parsed_json, output_json);
                    assert_eq!(parsed_shell, shell);
                }
            }
        }
        CaseKind::UnlockOk {
            args,
            output_json,
            unlock_all_flag,
            scopes,
        } => {
            let parsed =
                parse_unlock_contract_request(&task, &string_args(args)).expect("unlock parse");
            assert_eq!(parsed.output_json, output_json);
            assert_eq!(parsed.unlock_all_flag, unlock_all_flag);
            assert_eq!(parsed.scopes, string_args(scopes));
        }
        CaseKind::ConfigOk {
            args,
            schema,
            minimal,
            output_json,
            target,
            runner,
        } => {
            let parsed =
                parse_config_contract_request(&task, &string_args(args)).expect("config parse");
            assert_eq!(
                parsed,
                ConfigParseContract {
                    schema,
                    minimal,
                    output_json,
                    target,
                    runner,
                }
            );
        }
        CaseKind::WatchErr { args, expected } => {
            assert_parser_task_invocation_error(
                parse_watch_contract_request(&task, &string_args(args)),
                expected,
            );
        }
        CaseKind::CompletionErr { args, expected } => {
            assert_parser_task_invocation_error(
                parse_completion_contract_request(&task, &string_args(args)),
                expected,
            );
        }
        CaseKind::UnlockErr { args, expected } => {
            assert_parser_task_invocation_error(
                parse_unlock_contract_request(&task, &string_args(args)),
                expected,
            );
        }
        CaseKind::ConfigErr { args, expected } => {
            assert_parser_task_invocation_error(
                parse_config_contract_request(&task, &string_args(args)),
                expected,
            );
        }
    });
}
