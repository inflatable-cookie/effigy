use super::{parse_task_reference_invocation, RunnerError};

#[test]
fn parse_task_reference_invocation_table_valid_cases() {
    struct Case {
        raw: &'static str,
        prefix: Option<&'static str>,
        task: &'static str,
        args: &'static [&'static str],
    }

    let cases = [
        Case {
            raw: "test",
            prefix: None,
            task: "test",
            args: &[],
        },
        Case {
            raw: "farmyard/test vitest",
            prefix: Some("farmyard"),
            task: "test",
            args: &["vitest"],
        },
        Case {
            raw: r#"../froyo/validate "user service" escaped\ arg"#,
            prefix: Some("../froyo"),
            task: "validate",
            args: &["user service", "escaped arg"],
        },
        Case {
            raw: r#"test vitest "tests/api/user test.ts""#,
            prefix: None,
            task: "test",
            args: &["vitest", "tests/api/user test.ts"],
        },
        Case {
            raw: r#"test '' "" one"#,
            prefix: None,
            task: "test",
            args: &["", "", "one"],
        },
    ];

    for case in cases {
        let (selector, args) =
            parse_task_reference_invocation(case.raw).expect("parse task reference invocation");
        assert_eq!(selector.prefix.as_deref(), case.prefix, "raw={}", case.raw);
        assert_eq!(selector.task_name, case.task, "raw={}", case.raw);
        let expected = case
            .args
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>();
        assert_eq!(args, expected, "raw={}", case.raw);
    }
}

#[test]
fn parse_task_reference_invocation_table_invalid_cases() {
    let cases = [
        ("test \"unterminated", "unterminated quote"),
        ("test 'unterminated", "unterminated quote"),
        ("test vitest \\", "trailing escape"),
        ("", "task reference is required"),
        ("   ", "task reference is required"),
    ];

    for (raw, expected) in cases {
        let err = parse_task_reference_invocation(raw).expect_err("expected parse failure");
        match err {
            RunnerError::TaskInvocation(message) => {
                assert!(message.contains(expected), "raw={raw}, message={message}");
            }
            other => panic!("raw={raw}, unexpected error: {other}"),
        }
    }
}
