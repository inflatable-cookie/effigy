use super::{parse_vt_emulator_flag, select_process_tabs};

#[test]
fn select_process_tabs_falls_back_to_default_order() {
    let selected = select_process_tabs(
        vec!["api".to_owned(), "db".to_owned()],
        Vec::<String>::new(),
    );
    assert_eq!(selected, vec!["api".to_owned(), "db".to_owned()]);
}

#[test]
fn select_process_tabs_prefers_explicit_tab_order() {
    let selected = select_process_tabs(
        vec!["api".to_owned(), "db".to_owned()],
        vec!["db".to_owned(), "api".to_owned(), "shell".to_owned()],
    );
    assert_eq!(
        selected,
        vec!["db".to_owned(), "api".to_owned(), "shell".to_owned()]
    );
}

#[test]
fn parse_vt_emulator_flag_defaults_to_enabled() {
    assert!(parse_vt_emulator_flag(None));
    assert!(parse_vt_emulator_flag(Some("true")));
    assert!(parse_vt_emulator_flag(Some("1")));
    assert!(parse_vt_emulator_flag(Some("yes")));
}

#[test]
fn parse_vt_emulator_flag_handles_disabled_values() {
    assert!(!parse_vt_emulator_flag(Some("0")));
    assert!(!parse_vt_emulator_flag(Some("false")));
    assert!(!parse_vt_emulator_flag(Some("FALSE")));
}
