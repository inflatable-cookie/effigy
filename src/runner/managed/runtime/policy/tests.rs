use super::{enforce_non_zero_exit_policy, normalize_non_zero_exits};
use crate::runner::RunnerError;

#[test]
fn normalize_non_zero_exits_sorts_and_deduplicates() {
    let exits = normalize_non_zero_exits(vec![
        ("jobs".to_owned(), "exit=3".to_owned()),
        ("api".to_owned(), "exit=2".to_owned()),
        ("jobs".to_owned(), "exit=3".to_owned()),
    ]);

    assert_eq!(
        exits,
        vec![
            ("api".to_owned(), "exit=2".to_owned()),
            ("jobs".to_owned(), "exit=3".to_owned()),
        ]
    );
}

#[test]
fn enforce_non_zero_exit_policy_returns_structured_error_when_enabled() {
    let err = enforce_non_zero_exit_policy(
        "dev",
        "default",
        true,
        vec![
            ("jobs".to_owned(), "exit=3".to_owned()),
            ("api".to_owned(), "exit=2".to_owned()),
            ("jobs".to_owned(), "exit=3".to_owned()),
        ],
    )
    .expect_err("non-zero exits should fail when enabled");

    match err {
        RunnerError::TaskManagedNonZeroExit {
            task,
            profile,
            processes,
        } => {
            assert_eq!(task, "dev");
            assert_eq!(profile, "default");
            assert_eq!(
                processes,
                vec![
                    ("api".to_owned(), "exit=2".to_owned()),
                    ("jobs".to_owned(), "exit=3".to_owned()),
                ]
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn enforce_non_zero_exit_policy_allows_failures_when_disabled() {
    enforce_non_zero_exit_policy(
        "dev",
        "default",
        false,
        vec![("api".to_owned(), "exit=7".to_owned())],
    )
    .expect("disabled fail-on-non-zero should not error");
}
