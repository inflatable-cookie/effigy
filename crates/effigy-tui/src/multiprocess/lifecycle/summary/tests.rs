use std::collections::HashMap;

use super::{collect_non_zero_exits, format_process_status};
use effigy_ui::theme::Theme;

#[test]
fn collect_non_zero_exits_merges_and_sorts_failures() {
    let observed = HashMap::from([
        ("api".to_owned(), "exit=1".to_owned()),
        ("zeta".to_owned(), "signal=2".to_owned()),
    ]);
    let diagnostics = vec![
        ("db".to_owned(), "exit=0".to_owned()),
        ("worker".to_owned(), "signal=15".to_owned()),
        ("cache".to_owned(), "exit=7".to_owned()),
    ];

    let non_zero = collect_non_zero_exits(observed, diagnostics);

    assert_eq!(
        non_zero,
        vec![
            ("api".to_owned(), "exit=1".to_owned()),
            ("cache".to_owned(), "exit=7".to_owned()),
            ("zeta".to_owned(), "signal=2".to_owned()),
        ]
    );
}

#[test]
fn format_process_status_plain_success_and_failure_are_deterministic() {
    let theme = Theme::default();
    assert_eq!(
        format_process_status("exit=0", "12s", false, theme),
        "OK 12s".to_owned()
    );

    let theme = Theme::default();
    assert_eq!(
        format_process_status("signal=15", "3s", false, theme),
        "OK 3s".to_owned()
    );

    let theme = Theme::default();
    assert_eq!(
        format_process_status("exit=9", "5s", false, theme),
        "exit=9 5s".to_owned()
    );
}
